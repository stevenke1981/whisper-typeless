use slint::ComponentHandle;

use crate::AppWindow;

// ─── v2.2 浮動模式切換 ────────────────────────────────────────────────────────

/// 視窗尺寸（單位 px，logical）
const FULL_W: f32 = 560.0;
const FULL_H: f32 = 680.0;
const FLOAT_W: f32 = 380.0;
const FLOAT_BAR_H: f32 = 52.0;
const FLOAT_EXP_H: f32 = 540.0;

pub(super) fn apply_window_geometry(ui: &AppWindow) {
    let floating = ui.get_floating_mode();
    let expanded = ui.get_floating_expanded();
    let (w, h) = if !floating {
        (FULL_W, FULL_H)
    } else if expanded {
        (FLOAT_W, FLOAT_EXP_H)
    } else {
        (FLOAT_W, FLOAT_BAR_H)
    };
    ui.window().set_size(slint::LogicalSize::new(w, h));

    #[cfg(windows)]
    set_topmost_windows(ui, floating);
}

#[cfg(windows)]
fn set_topmost_windows(ui: &AppWindow, topmost: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::{SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE};

    let slint_wh = ui.window().window_handle();
    let Ok(raw) = slint_wh.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = raw.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as HWND;
    let target = if topmost {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    // SAFETY: hwnd 取自 Slint 提供之 RawWindowHandle，仍存活；SetWindowPos 為標準 Win32 API。
    unsafe {
        SetWindowPos(hwnd, target, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    }
}

#[cfg(not(windows))]
fn set_topmost_windows(_ui: &AppWindow, _topmost: bool) {}

/// start-drag：Win32 透過 ReleaseCapture + WM_NCLBUTTONDOWN(HTCAPTION) 觸發系統移動
#[cfg(windows)]
pub(super) fn start_drag(ui: &AppWindow) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::{ReleaseCapture, SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN};

    let slint_wh = ui.window().window_handle();
    let Ok(raw) = slint_wh.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = raw.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as HWND;
    // SAFETY: hwnd 來自 Slint live window；兩個 API 皆為標準 Win32 訊息呼叫。
    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
    }
}

#[cfg(not(windows))]
pub(super) fn start_drag(_ui: &AppWindow) {}
