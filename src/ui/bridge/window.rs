use slint::ComponentHandle;
use tracing::info;

use crate::AppWindow;

// ─── v2.2 浮動模式切換 ────────────────────────────────────────────────────────

/// 視窗尺寸（單位 px，logical）
const FULL_W: f32 = 560.0;
const FULL_H: f32 = 680.0;
const FLOAT_W: f32 = 380.0;
const FLOAT_BAR_H: f32 = 52.0;
const FLOAT_EXP_H: f32 = 540.0;

fn apply_window_geometry(ui: &AppWindow) {
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

pub(super) fn setup_floating_mode_callbacks(ui: &AppWindow) {
    // 啟動時不主動 set_size：交給 Slint preferred-* 決定，避免在 run() 前
    // 對尚未建立的 OS window 操作造成的副作用。

    {
        let ui_ref = ui.as_weak();
        ui.on_toggle_floating_mode(move || {
            if let Some(ui) = ui_ref.upgrade() {
                apply_window_geometry(&ui);
                info!(floating = ui.get_floating_mode(), "視窗模式切換");
            }
        });
    }

    {
        let ui_ref = ui.as_weak();
        ui.on_toggle_floating_expanded(move || {
            if let Some(ui) = ui_ref.upgrade() {
                apply_window_geometry(&ui);
            }
        });
    }

    {
        let ui_ref = ui.as_weak();
        ui.on_request_close(move || {
            if let Some(ui) = ui_ref.upgrade() {
                let _ = ui.window().hide();
            }
        });
    }

    // start-drag：Win32 透過 ReleaseCapture + WM_NCLBUTTONDOWN(HTCAPTION) 觸發系統移動
    #[cfg(windows)]
    {
        let ui_ref = ui.as_weak();
        ui.on_start_drag(move || {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            use winapi::shared::windef::HWND;
            use winapi::um::winuser::{ReleaseCapture, SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN};

            let Some(ui) = ui_ref.upgrade() else { return };
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
        });
    }

    {
        let ui_ref = ui.as_weak();
        ui.on_clear_error_log(move || {
            // 清空緩衝；listener 會把空清單推回 UI
            crate::ui::error_log::global().clear();
            if let Some(ui) = ui_ref.upgrade() {
                let empty: Vec<slint::SharedString> = Vec::new();
                ui.set_error_log(slint::ModelRc::new(slint::VecModel::from(empty)));
                ui.set_show_error_log(false);
            }
        });
    }

    attach_error_log_sink(ui);
}

/// 把全域錯誤紀錄（tracing WARN/ERROR）同步到 UI 的 `error-log` 屬性 / 計數徽章。
///
/// 連續大量事件只排一次 event-loop 更新（pending 旗標合併）。
fn attach_error_log_sink(ui: &AppWindow) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let ui_ref = ui.as_weak();
    let pending = Arc::new(AtomicBool::new(false));
    let refresh = move || {
        if pending.swap(true, Ordering::AcqRel) {
            return; // 已有一次更新排隊中
        }
        let u = ui_ref.clone();
        let p = Arc::clone(&pending);
        let _ = slint::invoke_from_event_loop(move || {
            p.store(false, Ordering::Release);
            if let Some(ui) = u.upgrade() {
                let entries: Vec<slint::SharedString> = crate::ui::error_log::global()
                    .snapshot_newest_first()
                    .into_iter()
                    .map(Into::into)
                    .collect();
                ui.set_error_log(slint::ModelRc::new(slint::VecModel::from(entries)));
            }
        });
    };

    // 啟動前已發生的錯誤也要顯示
    refresh();
    crate::ui::error_log::global().set_listener(refresh);
}
