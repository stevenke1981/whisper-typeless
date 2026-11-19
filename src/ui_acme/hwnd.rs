//! Win32 輔助：取得自己的 HWND、不搶焦點地縮放視窗、切換置頂。
//!
//! AcmeUI（winit）不公開原生 HWND，因此以 `EnumWindows` 依行程 id 與可見性尋找。
//! 所有呼叫都使用 `SWP_NOACTIVATE`，不會把焦點從使用者正在輸入的視窗搶走。

/// 邏輯尺寸 × DPI 縮放 → 實體像素（至少 1）。
pub fn scaled_px(logical: f32, scale: f32) -> i32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    ((logical * scale).round() as i32).max(1)
}

#[cfg(windows)]
mod imp {
    use winapi::shared::minwindef::{BOOL, LPARAM, TRUE};
    use winapi::shared::windef::HWND;
    use winapi::um::processthreadsapi::GetCurrentProcessId;
    use winapi::um::winuser::{
        EnumWindows, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindowVisible,
        SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_NOACTIVATE,
    };

    struct Search {
        pid: u32,
        found: HWND,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam 是 find_own_hwnd 傳入的 &mut Search，列舉期間有效。
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;
        // SAFETY: hwnd 由 EnumWindows 提供。
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        // SAFETY: 同上。
        if pid == search.pid && unsafe { IsWindowVisible(hwnd) } != 0 {
            search.found = hwnd;
            return 0; // 停止列舉
        }
        TRUE
    }

    /// 本行程第一個可見的頂層視窗。
    pub fn find_own_hwnd() -> Option<isize> {
        // SAFETY: 單純的 Win32 呼叫。
        let mut search = Search {
            pid: unsafe { GetCurrentProcessId() },
            found: std::ptr::null_mut(),
        };
        // SAFETY: enum_proc 只在此呼叫期間使用 search。
        unsafe { EnumWindows(Some(enum_proc), &mut search as *mut Search as LPARAM) };
        (!search.found.is_null()).then_some(search.found as isize)
    }

    /// 變更視窗外框大小（實體像素），保持位置與 Z 順序，不啟用視窗。
    pub fn set_size(hwnd: isize, width: i32, height: i32) -> bool {
        // SAFETY: hwnd 來自 find_own_hwnd；失敗只回傳 0。
        unsafe {
            SetWindowPos(
                hwnd as HWND,
                std::ptr::null_mut(),
                0,
                0,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
            ) != 0
        }
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

    /// 加上 `WS_EX_NOACTIVATE`：點擊視窗不會取得鍵盤焦點。
    pub fn set_no_activate(hwnd: isize) {
        // SAFETY: 同上。
        unsafe {
            let style = GetWindowLongPtrW(hwnd as HWND, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd as HWND, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn find_own_hwnd() -> Option<isize> {
        None
    }
    pub fn set_size(_hwnd: isize, _width: i32, _height: i32) -> bool {
        false
    }
    pub fn set_topmost(_hwnd: isize, _topmost: bool) -> bool {
        false
    }
    pub fn set_no_activate(_hwnd: isize) {}
}

pub use imp::{find_own_hwnd, set_no_activate, set_size, set_topmost};

#[cfg(test)]
mod tests {
    use super::*;

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
