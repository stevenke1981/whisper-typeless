//! Win32 輔助：取得自己的 HWND、不搶焦點地縮放視窗、切換置頂。
//!
//! AcmeUI（winit）不公開原生 HWND，因此以 `EnumWindows` 依行程 id 與可見性尋找。
//! 所有 `SetWindowPos` 都帶 `SWP_NOACTIVATE`，不會把焦點從使用者正在輸入的視窗搶走。
//!
//! 注意：視窗**不**加 `WS_EX_NOACTIVATE`——那會讓視窗永遠拿不到鍵盤焦點，
//! Esc / Space / E 快捷鍵與文字輸入（含輸入法）全部失效。熱鍵 / PTT 路徑改由
//! 「永不送出 `WindowRequest::Focus`、縮放 / 置頂一律 `SWP_NOACTIVATE`」保證不搶焦點。

/// 邏輯尺寸 × DPI 縮放 → 實體像素（至少 1）。
pub fn scaled_px(logical: f32, scale: f32) -> i32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    ((logical * scale).round() as i32).max(1)
}

/// 螢幕座標的矩形（實體像素）：左上角 + 寬高。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PxRect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
}

/// 把視窗矩形限制在工作區內：過大時縮小寬 / 高到工作區大小，
/// 再平移使其完全落在工作區（避開工作列與螢幕外）。純函式，供單元測試。
pub fn clamp_to_work_area(rect: PxRect, work: PxRect) -> PxRect {
    if work.w <= 0 || work.h <= 0 {
        return rect;
    }
    let w = rect.w.clamp(1, work.w);
    let h = rect.h.clamp(1, work.h);
    let x = rect.x.clamp(work.x, work.right() - w);
    let y = rect.y.clamp(work.y, work.bottom() - h);
    PxRect { x, y, w, h }
}

#[cfg(windows)]
mod imp {
    use super::{clamp_to_work_area, PxRect};
    use winapi::shared::minwindef::{BOOL, LPARAM, TRUE};
    use winapi::shared::windef::{HWND, RECT};
    use winapi::um::processthreadsapi::GetCurrentProcessId;
    use winapi::um::winuser::{
        EnumWindows, GetMonitorInfoW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, MonitorFromWindow, SetForegroundWindow, SetWindowPos,
        ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOW,
        SW_SHOWNOACTIVATE,
    };

    struct Search {
        pid: u32,
        title: Vec<u16>,
        found: HWND,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam 是 find_own_hwnd 傳入的 &mut Search，列舉期間有效。
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;
        // SAFETY: hwnd 由 EnumWindows 提供。
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        // SAFETY: 同上。
        if pid != search.pid || unsafe { IsWindowVisible(hwnd) } == 0 {
            return TRUE;
        }
        let mut buf = [0u16; 256];
        // SAFETY: buf 長度正確；GetWindowTextW 最多寫入 len - 1 個字元加結尾 0。
        let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        let len = len.max(0) as usize;
        // 標題相符才算（避開檔案對話框、確認視窗等同行程的其他視窗）。
        if search.title.is_empty() || buf[..len] == search.title[..] {
            search.found = hwnd;
            return 0; // 停止列舉
        }
        TRUE
    }

    /// 本行程中標題為 `title` 的可見頂層視窗（`title` 為空字串時取第一個可見視窗）。
    pub fn find_own_hwnd(title: &str) -> Option<isize> {
        // SAFETY: 單純的 Win32 呼叫。
        let mut search = Search {
            pid: unsafe { GetCurrentProcessId() },
            title: title.encode_utf16().collect(),
            found: std::ptr::null_mut(),
        };
        // SAFETY: enum_proc 只在此呼叫期間使用 search。
        unsafe { EnumWindows(Some(enum_proc), &mut search as *mut Search as LPARAM) };
        (!search.found.is_null()).then_some(search.found as isize)
    }

    /// 目前視窗矩形（實體像素）。
    pub fn window_rect(hwnd: isize) -> Option<PxRect> {
        let mut rc: RECT = unsafe { std::mem::zeroed() };
        // SAFETY: rc 是有效的輸出緩衝；失敗回傳 0。
        if unsafe { GetWindowRect(hwnd as HWND, &mut rc) } == 0 {
            return None;
        }
        Some(PxRect::new(
            rc.left,
            rc.top,
            rc.right - rc.left,
            rc.bottom - rc.top,
        ))
    }

    /// 視窗所在（或最近）螢幕的工作區（排除工作列）。
    pub fn work_area(hwnd: isize) -> Option<PxRect> {
        // SAFETY: 單純的 Win32 呼叫；MONITORINFO 的 cbSize 已設定。
        unsafe {
            let monitor = MonitorFromWindow(hwnd as HWND, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return None;
            }
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return None;
            }
            let rc = info.rcWork;
            Some(PxRect::new(
                rc.left,
                rc.top,
                rc.right - rc.left,
                rc.bottom - rc.top,
            ))
        }
    }

    /// 把視窗移到 / 縮到工作區內（不啟用視窗、不改 Z 順序）。
    pub fn fit_to_work_area(hwnd: isize) -> bool {
        let (Some(rect), Some(work)) = (window_rect(hwnd), work_area(hwnd)) else {
            return false;
        };
        place(hwnd, clamp_to_work_area(rect, work), rect)
    }

    fn place(hwnd: isize, to: PxRect, from: PxRect) -> bool {
        if to == from {
            return true;
        }
        // SAFETY: hwnd 來自 find_own_hwnd；失敗只回傳 0。
        unsafe {
            SetWindowPos(
                hwnd as HWND,
                std::ptr::null_mut(),
                to.x,
                to.y,
                to.w,
                to.h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) != 0
        }
    }

    /// 變更視窗外框大小（實體像素），保持左上角，並限制在目前螢幕工作區內
    /// （過高會縮小、超出會平移）。不啟用視窗。
    pub fn set_size(hwnd: isize, width: i32, height: i32) -> bool {
        let Some(cur) = window_rect(hwnd) else {
            return false;
        };
        let wanted = PxRect::new(cur.x, cur.y, width.max(1), height.max(1));
        let target = match work_area(hwnd) {
            Some(work) => clamp_to_work_area(wanted, work),
            None => wanted,
        };
        place(hwnd, target, cur)
    }

    /// 隱藏視窗（SW_HIDE）。不影響焦點視窗以外的任何東西。
    pub fn hide(hwnd: isize) -> bool {
        // SAFETY: hwnd 來自 find_own_hwnd；ShowWindow 回傳先前是否可見，不代表失敗。
        unsafe { ShowWindow(hwnd as HWND, SW_HIDE) };
        true
    }

    /// 視窗是否被最小化。
    pub fn is_iconic(hwnd: isize) -> bool {
        // SAFETY: 單純查詢。
        unsafe { IsIconic(hwnd as HWND) != 0 }
    }

    /// 顯示視窗。`activate` 為 true 時還原並設為前景（只用於使用者點系統匣的情形，
    /// 熱鍵 / PTT 路徑一律傳 false）；false 時以 SW_SHOWNOACTIVATE 顯示。
    pub fn show(hwnd: isize, activate: bool) -> bool {
        // SAFETY: 同上。
        unsafe {
            let h = hwnd as HWND;
            if activate {
                let cmd = if IsIconic(h) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOW
                };
                ShowWindow(h, cmd);
                SetForegroundWindow(h);
            } else {
                ShowWindow(h, SW_SHOWNOACTIVATE);
            }
        }
        true
    }

    /// 切換置頂，不啟用視窗。
    pub fn set_topmost(hwnd: isize, topmost: bool) -> bool {
        let after = if topmost {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        // SAFETY: 同上。
        unsafe {
            SetWindowPos(
                hwnd as HWND,
                after,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            ) != 0
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn find_own_hwnd(_title: &str) -> Option<isize> {
        None
    }
    pub fn set_size(_hwnd: isize, _width: i32, _height: i32) -> bool {
        false
    }
    pub fn fit_to_work_area(_hwnd: isize) -> bool {
        false
    }
    pub fn set_topmost(_hwnd: isize, _topmost: bool) -> bool {
        false
    }
    pub fn hide(_hwnd: isize) -> bool {
        false
    }
    pub fn is_iconic(_hwnd: isize) -> bool {
        false
    }
    pub fn show(_hwnd: isize, _activate: bool) -> bool {
        false
    }
}

pub use imp::{find_own_hwnd, fit_to_work_area, hide, is_iconic, set_size, set_topmost, show};

/// 視窗實際顯示狀態與期望（`AppViewState::window_hidden`）之間要做的動作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Hide,
    Show,
}

/// 純函式：期望隱藏而目前顯示 → 隱藏；期望顯示而目前隱藏 → 顯示；其餘不動。
pub fn visibility_action(want_hidden: bool, shown: bool) -> Option<Visibility> {
    match (want_hidden, shown) {
        (true, true) => Some(Visibility::Hide),
        (false, false) => Some(Visibility::Show),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: PxRect = PxRect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1008,
    };

    #[test]
    fn clamp_keeps_rect_already_inside() {
        let r = PxRect::new(100, 100, 840, 880);
        assert_eq!(clamp_to_work_area(r, WORK), r);
    }

    #[test]
    fn clamp_moves_up_when_bottom_hidden_behind_taskbar() {
        // 觀察到的情形：y=228、高 1020 → 縮到 1008 並貼齊頂端。
        let out = clamp_to_work_area(PxRect::new(540, 228, 840, 1020), WORK);
        assert_eq!(out, PxRect::new(540, 0, 840, 1008));
        let out = clamp_to_work_area(PxRect::new(540, 228, 840, 930), WORK);
        assert_eq!(out, PxRect::new(540, 78, 840, 930));
    }

    #[test]
    fn clamp_pulls_in_from_right_and_left_edges() {
        let out = clamp_to_work_area(PxRect::new(1800, 10, 570, 810), WORK);
        assert_eq!((out.x, out.y), (1350, 10));
        let out = clamp_to_work_area(PxRect::new(-300, 10, 570, 810), WORK);
        assert_eq!(out.x, 0);
    }

    #[test]
    fn clamp_shrinks_oversized_and_respects_offset_work_area() {
        let work = PxRect::new(-1920, 40, 1920, 1000);
        let out = clamp_to_work_area(PxRect::new(-100, 900, 3000, 2000), work);
        assert_eq!(out, work);
        let out = clamp_to_work_area(PxRect::new(-1900, 1000, 500, 300), work);
        assert_eq!(out, PxRect::new(-1900, 740, 500, 300));
    }

    #[test]
    fn clamp_ignores_degenerate_work_area() {
        let r = PxRect::new(5, 5, 100, 100);
        assert_eq!(clamp_to_work_area(r, PxRect::new(0, 0, 0, 0)), r);
    }

    #[test]
    fn visibility_diff() {
        assert_eq!(visibility_action(true, true), Some(Visibility::Hide));
        assert_eq!(visibility_action(false, false), Some(Visibility::Show));
        assert_eq!(visibility_action(true, false), None);
        assert_eq!(visibility_action(false, true), None);
    }

    #[test]
    fn scaling_rounds_and_guards_bad_scale() {
        assert_eq!(scaled_px(380.0, 1.0), 380);
        assert_eq!(scaled_px(380.0, 1.5), 570);
        assert_eq!(scaled_px(52.0, 1.25), 65);
        assert_eq!(scaled_px(52.0, 0.0), 52);
        assert_eq!(scaled_px(52.0, f32::NAN), 52);
        assert_eq!(scaled_px(0.0, 1.0), 1);
    }
}
