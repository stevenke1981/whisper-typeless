//! 系統匣圖示（Win32 `Shell_NotifyIconW`，不用額外 crate）。
//!
//! 圖示與右鍵選單跑在自己的執行緒（含一個 message-only 視窗與訊息迴圈）。
//! 該執行緒**只**呼叫 [`Notifier::send`]`(CoreEvent::Tray(..))`，不碰任何 UI 狀態；
//! 畫面層在自己的執行緒上套用事件。錄音中圖示換成紅色版本（與工作列圖示同一組繪製）。
//!
//! 純邏輯（選單文字、選單項目 → `TrayAction`、提示文字）與 Win32 外殼分開，前者有單元測試。

use crate::core::TrayAction;

/// 選單項目 id。
pub const ID_TOGGLE_WINDOW: u16 = 1;
pub const ID_TOGGLE_RECORDING: u16 = 2;
pub const ID_SETTINGS: u16 = 3;
pub const ID_QUIT: u16 = 4;

/// 選單項目文字（依序：視窗、錄音、設定、結束）。
pub fn menu_labels(recording: bool, window_hidden: bool) -> [&'static str; 4] {
    [
        if window_hidden {
            "顯示主視窗"
        } else {
            "隱藏主視窗"
        },
        if recording {
            "停止錄音"
        } else {
            "開始錄音"
        },
        "設定",
        "結束",
    ]
}

/// 選單項目 id → 動作。
pub fn action_for_menu_id(id: u16) -> Option<TrayAction> {
    match id {
        ID_TOGGLE_WINDOW => Some(TrayAction::ToggleWindow),
        ID_TOGGLE_RECORDING => Some(TrayAction::ToggleRecording),
        ID_SETTINGS => Some(TrayAction::OpenSettings),
        ID_QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

/// 圖示提示文字。
pub fn tooltip(recording: bool) -> &'static str {
    if recording {
        "whisper-typeless（錄音中）"
    } else {
        "whisper-typeless"
    }
}

#[cfg(windows)]
pub use imp::TrayHandle;

#[cfg(not(windows))]
pub use stub::TrayHandle;

#[cfg(not(windows))]
mod stub {
    use crate::core::Notifier;

    /// 非 Windows 沒有系統匣。
    pub struct TrayHandle;

    impl TrayHandle {
        pub fn spawn(_notifier: Notifier) -> Option<Self> {
            None
        }
        pub fn set_recording(&self, _recording: bool) {}
        pub fn set_window_hidden(&self, _hidden: bool) {}
    }
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
    use winapi::shared::windef::{HICON, HWND, POINT};
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::shellapi::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    };
    use winapi::um::winuser::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
        DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, PostMessageW, PostQuitMessage,
        RegisterClassExW, RegisterWindowMessageW, SetForegroundWindow, SetMenuDefaultItem,
        TrackPopupMenu, TranslateMessage, HWND_MESSAGE, MF_SEPARATOR, MF_STRING, MSG, TPM_NONOTIFY,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONUP,
        WM_NULL, WM_RBUTTONUP, WNDCLASSEXW,
    };

    use super::{action_for_menu_id, menu_labels, tooltip};
    use super::{ID_QUIT, ID_SETTINGS, ID_TOGGLE_RECORDING, ID_TOGGLE_WINDOW};
    use crate::core::{CoreEvent, Notifier, TrayAction};

    /// 圖示回呼訊息（lparam 低字組為滑鼠訊息）。
    const WM_TRAY_CALLBACK: UINT = WM_APP + 1;
    /// 畫面層通知「狀態變了，請更新圖示」。
    const WM_TRAY_STATE: UINT = WM_APP + 2;
    const ICON_UID: UINT = 1;
    const CLASS_NAME: &str = "WhisperTypelessTrayWindow";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 畫面執行緒與系統匣執行緒共用的狀態。
    #[derive(Default)]
    struct Shared {
        recording: AtomicBool,
        window_hidden: AtomicBool,
    }

    /// 系統匣執行緒內的環境（只在該執行緒存取）。
    struct Ctx {
        notifier: Notifier,
        shared: Arc<Shared>,
        icon_idle: HICON,
        icon_rec: HICON,
        taskbar_created: UINT,
    }

    thread_local! {
        static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
    }

    /// 系統匣圖示的擁有者；Drop 時移除圖示並結束執行緒。
    pub struct TrayHandle {
        hwnd: isize,
        shared: Arc<Shared>,
        thread: Option<JoinHandle<()>>,
    }

    impl TrayHandle {
        /// 啟動系統匣執行緒；圖示加入失敗（例如沒有 Explorer）時回傳 `None`，
        /// 呼叫端應退回「沒有系統匣」的行為（關閉視窗 = 結束）。
        pub fn spawn(notifier: Notifier) -> Option<Self> {
            let shared = Arc::new(Shared::default());
            let (tx, rx) = mpsc::channel::<Result<isize, String>>();
            let thread_shared = Arc::clone(&shared);
            let thread = std::thread::Builder::new()
                .name("tray".into())
                .spawn(move || run(notifier, thread_shared, tx))
                .ok()?;
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(hwnd)) => Some(Self {
                    hwnd,
                    shared,
                    thread: Some(thread),
                }),
                Ok(Err(e)) => {
                    tracing::warn!("系統匣無法啟用：{e}");
                    let _ = thread.join();
                    None
                }
                Err(_) => {
                    tracing::warn!("系統匣啟動逾時");
                    None
                }
            }
        }

        /// 更新錄音狀態（換紅色圖示 / 提示文字 / 選單文字）。
        pub fn set_recording(&self, recording: bool) {
            if self.shared.recording.swap(recording, Ordering::AcqRel) != recording {
                self.poke();
            }
        }

        /// 更新視窗是否隱藏（選單文字「顯示 / 隱藏主視窗」）。
        pub fn set_window_hidden(&self, hidden: bool) {
            self.shared.window_hidden.store(hidden, Ordering::Release);
        }

        fn poke(&self) {
            // SAFETY: PostMessageW 可跨執行緒呼叫；視窗已不存在時只會失敗。
            unsafe { PostMessageW(self.hwnd as HWND, WM_TRAY_STATE, 0, 0) };
        }
    }

    impl Drop for TrayHandle {
        fn drop(&mut self) {
            // SAFETY: 同上。WM_CLOSE 由系統匣執行緒處理：NIM_DELETE 後結束訊息迴圈。
            unsafe { PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0) };
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    fn run(notifier: Notifier, shared: Arc<Shared>, ready: mpsc::Sender<Result<isize, String>>) {
        // SAFETY: 單純的 Win32 呼叫；所有指標在呼叫期間有效。
        unsafe {
            let hinst = GetModuleHandleW(std::ptr::null());
            let class = wide(CLASS_NAME);
            let mut wc: WNDCLASSEXW = std::mem::zeroed();
            wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
            wc.lpfnWndProc = Some(wnd_proc);
            wc.hInstance = hinst;
            wc.lpszClassName = class.as_ptr();
            RegisterClassExW(&wc); // 已註冊時回傳 0，視窗仍可建立
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                hinst,
                std::ptr::null_mut(),
            );
            if hwnd.is_null() {
                let _ = ready.send(Err("建立訊息視窗失敗".into()));
                return;
            }
            let ctx = Ctx {
                notifier,
                shared,
                icon_idle: crate::ui::window_icon::create_hicon(false) as HICON,
                icon_rec: crate::ui::window_icon::create_hicon(true) as HICON,
                taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            };
            CTX.with(|c| *c.borrow_mut() = Some(ctx));
            if !notify(hwnd, NIM_ADD) {
                CTX.with(|c| free_ctx(c.borrow_mut().take()));
                DestroyWindow(hwnd);
                let _ = ready.send(Err("加入通知區域圖示失敗".into()));
                return;
            }
            let _ = ready.send(Ok(hwnd as isize));

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            CTX.with(|c| free_ctx(c.borrow_mut().take()));
        }
    }

    unsafe fn free_ctx(ctx: Option<Ctx>) {
        if let Some(ctx) = ctx {
            for icon in [ctx.icon_idle, ctx.icon_rec] {
                if !icon.is_null() {
                    // SAFETY: 圖示由 create_hicon 建立，只在此釋放一次。
                    unsafe { DestroyIcon(icon) };
                }
            }
        }
    }

    /// NIM_ADD / NIM_MODIFY / NIM_DELETE；回傳是否成功。
    unsafe fn notify(hwnd: HWND, op: u32) -> bool {
        let (icon, recording) = CTX.with(|c| {
            c.borrow()
                .as_ref()
                .map(|c| {
                    let rec = c.shared.recording.load(Ordering::Acquire);
                    (if rec { c.icon_rec } else { c.icon_idle }, rec)
                })
                .unwrap_or((std::ptr::null_mut(), false))
        });
        // SAFETY: 全零的 NOTIFYICONDATAW 合法；cbSize 與欄位依文件設定。
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = ICON_UID;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_TRAY_CALLBACK;
            nid.hIcon = icon;
            let tip = wide(tooltip(recording));
            let n = tip.len().min(nid.szTip.len() - 1);
            nid.szTip[..n].copy_from_slice(&tip[..n]);
            Shell_NotifyIconW(op, &mut nid) != 0
        }
    }

    fn send(action: TrayAction) {
        let notifier = CTX.with(|c| c.borrow().as_ref().map(|c| c.notifier.clone()));
        if let Some(n) = notifier {
            n.send(CoreEvent::Tray(action));
        }
    }

    unsafe fn show_menu(hwnd: HWND) {
        let (recording, hidden) = CTX.with(|c| {
            c.borrow()
                .as_ref()
                .map(|c| {
                    (
                        c.shared.recording.load(Ordering::Acquire),
                        c.shared.window_hidden.load(Ordering::Acquire),
                    )
                })
                .unwrap_or((false, false))
        });
        let labels = menu_labels(recording, hidden);
        // SAFETY: 選單只在此函式內建立與銷毀。
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            let ids = [ID_TOGGLE_WINDOW, ID_TOGGLE_RECORDING, ID_SETTINGS, ID_QUIT];
            for (i, (id, label)) in ids.iter().zip(labels).enumerate() {
                if i == 3 {
                    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                }
                AppendMenuW(menu, MF_STRING, *id as usize, wide(label).as_ptr());
            }
            SetMenuDefaultItem(menu, ID_TOGGLE_WINDOW as u32, 0);
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            // 使用者剛點了圖示：讓選單視窗成為前景，點別處才會關閉選單。
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            if let Some(action) = u16::try_from(cmd).ok().and_then(action_for_menu_id) {
                send(action);
            }
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: UINT,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let taskbar_created = CTX.with(|c| c.borrow().as_ref().map_or(0, |c| c.taskbar_created));
        // SAFETY: 以下皆為對本視窗的 Win32 呼叫。
        unsafe {
            match msg {
                WM_TRAY_CALLBACK => {
                    match (lparam as u32) & 0xFFFF {
                        WM_LBUTTONUP => send(TrayAction::ToggleWindow),
                        WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                        _ => {}
                    }
                    0
                }
                WM_TRAY_STATE => {
                    notify(hwnd, NIM_MODIFY);
                    0
                }
                m if taskbar_created != 0 && m == taskbar_created => {
                    // Explorer 重新啟動：圖示消失了，重新加入。
                    notify(hwnd, NIM_ADD);
                    0
                }
                WM_CLOSE => {
                    DestroyWindow(hwnd);
                    0
                }
                WM_DESTROY => {
                    notify(hwnd, NIM_DELETE);
                    PostQuitMessage(0);
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_state() {
        let l = menu_labels(false, true);
        assert_eq!(l, ["顯示主視窗", "開始錄音", "設定", "結束"]);
        let l = menu_labels(true, false);
        assert_eq!(l, ["隱藏主視窗", "停止錄音", "設定", "結束"]);
    }

    #[test]
    fn menu_ids_map_to_actions() {
        assert_eq!(
            action_for_menu_id(ID_TOGGLE_WINDOW),
            Some(TrayAction::ToggleWindow)
        );
        assert_eq!(
            action_for_menu_id(ID_TOGGLE_RECORDING),
            Some(TrayAction::ToggleRecording)
        );
        assert_eq!(
            action_for_menu_id(ID_SETTINGS),
            Some(TrayAction::OpenSettings)
        );
        assert_eq!(action_for_menu_id(ID_QUIT), Some(TrayAction::Quit));
        assert_eq!(action_for_menu_id(0), None);
        assert_eq!(action_for_menu_id(99), None);
    }

    #[test]
    fn tooltip_marks_recording() {
        assert!(tooltip(true).contains("錄音中"));
        assert!(!tooltip(false).contains("錄音中"));
    }
}
