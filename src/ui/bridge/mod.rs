//! Slint 轉接層：把 `AppWindow` 接到 UI 無關的 [`crate::core`]。
//!
//! - Slint callback → [`UiCommand`] → [`Controller::handle`]
//! - 背景 [`CoreEvent`] → [`Notifier`](crate::core::Notifier) 喚醒 →
//!   `slint::invoke_from_event_loop` → [`Controller::apply`]
//! - 每次處理完把 [`AppViewState`] 與上次推送的版本比對，只設定有變的屬性（[`sync`]）。
//!
//! 使用者在 Slint 中透過雙向綁定直接修改的屬性（設定值、視窗模式…），
//! 會在處理任何指令 / 事件前先讀回核心狀態（[`sync::pull`]）。

mod sync;
mod window;

use std::cell::RefCell;
use std::sync::Arc;
use tokio::sync::RwLock;

use slint::ComponentHandle;

use crate::app::AppState;
use crate::core::services::{initial_state, RealBackend};
use crate::core::{event_channel, AppViewState, Controller, EventQueue, UiCommand, ViewRequest};
use crate::AppWindow;

// ─── 臨時 stderr 抑制（Slint/parley→ICU4X 啟動雜訊）──────────────────────
//
// ICU4X v2.2 的 icu_segmenter 缺少 ja 語系的 segmentation 模型，
// 但 Slint 在 AppWindow::new() 時會觸發 text layout → parley → icu_segmenter，
// 噴出 60+ 行 "ICU4X data error: No segmentation model for language: ja"。
// 這些錯誤走 eprintln!，繞過 tracing/log 過濾器，純屬非功能性雜訊。
// 解法：在 AppWindow::new() 期間暫時 redirect stderr → NUL。

#[cfg(windows)]
/// Guard 物件：建構時將 stderr 重新導向至 NUL，drop 時恢復。
struct IcuStderrGuard {
    old_handle: *mut winapi::ctypes::c_void,
    null_handle: *mut winapi::ctypes::c_void,
}

#[cfg(windows)]
impl IcuStderrGuard {
    fn new() -> Option<Self> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
        use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
        use winapi::um::processenv::{GetStdHandle, SetStdHandle};
        use winapi::um::winbase::STD_ERROR_HANDLE;
        use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_WRITE};

        // SAFETY: 僅在初始化主執行緒上呼叫，無併發競爭。
        unsafe {
            let null_path: Vec<u16> = OsStr::new("NUL").encode_wide().chain(Some(0)).collect();
            let raw_null = CreateFileW(
                null_path.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_WRITE | FILE_SHARE_READ,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );

            if raw_null == INVALID_HANDLE_VALUE {
                return None;
            }

            let raw_old = GetStdHandle(STD_ERROR_HANDLE);
            if raw_old.is_null() || raw_old == INVALID_HANDLE_VALUE {
                CloseHandle(raw_null);
                return None;
            }

            SetStdHandle(STD_ERROR_HANDLE, raw_null);

            Some(Self {
                old_handle: raw_old,
                null_handle: raw_null,
            })
        }
    }
}

#[cfg(windows)]
impl Drop for IcuStderrGuard {
    fn drop(&mut self) {
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processenv::SetStdHandle;
        use winapi::um::winbase::STD_ERROR_HANDLE;

        // SAFETY: 恢復原始 stderr 後關閉 NUL handle。
        unsafe {
            SetStdHandle(STD_ERROR_HANDLE, self.old_handle);
            CloseHandle(self.null_handle);
        }
    }
}

#[cfg(not(windows))]
struct IcuStderrGuard;
#[cfg(not(windows))]
impl IcuStderrGuard {
    fn new() -> Option<Self> {
        None
    }
}

// ─── 轉接器狀態（只存在於 UI 執行緒）─────────────────────────────────────────

struct Adapter {
    controller: Controller,
    queue: EventQueue,
    /// 上次推送到 AppWindow 的狀態（差異比較用）。
    last: AppViewState,
    ui: slint::Weak<AppWindow>,
}

impl Adapter {
    fn sync_from_window(&mut self, ui: &AppWindow) {
        sync::pull(ui, self.controller.state_mut());
        sync::pull(ui, &mut self.last);
    }

    fn sync_to_window(&mut self, ui: &AppWindow) {
        let state = self.controller.state();
        if state.is_recording != self.last.is_recording {
            crate::ui::window_icon::update(state.is_recording);
        }
        sync::push(ui, state, Some(&self.last));
        self.last = state.clone();
    }
}

thread_local! {
    static ADAPTER: RefCell<Option<Adapter>> = const { RefCell::new(None) };
}

/// 在 UI 執行緒處理一個使用者操作。
fn dispatch(cmd: UiCommand) {
    let outcome = ADAPTER.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return Err(cmd); // 重入：稍後再處理
        };
        let Some(adapter) = guard.as_mut() else {
            return Ok(None);
        };
        let Some(ui) = adapter.ui.upgrade() else {
            return Ok(None);
        };
        adapter.sync_from_window(&ui);
        let requests = adapter.controller.handle(cmd);
        adapter.sync_to_window(&ui);
        Ok(Some((ui, requests)))
    });
    match outcome {
        Ok(Some((ui, requests))) => {
            for request in requests {
                perform(&ui, request);
            }
        }
        Ok(None) => {}
        Err(cmd) => {
            let _ = slint::invoke_from_event_loop(move || dispatch(cmd));
        }
    }
}

/// 在 UI 執行緒套用所有待處理的背景事件。
fn drain_events() {
    let busy = ADAPTER.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return true;
        };
        let Some(adapter) = guard.as_mut() else {
            return false;
        };
        let events = adapter.queue.drain();
        if events.is_empty() {
            return false;
        }
        let Some(ui) = adapter.ui.upgrade() else {
            return false;
        };
        adapter.sync_from_window(&ui);
        for event in events {
            adapter.controller.apply(event);
        }
        adapter.sync_to_window(&ui);
        false
    });
    if busy {
        let _ = slint::invoke_from_event_loop(drain_events);
    }
}

fn perform(ui: &AppWindow, request: ViewRequest) {
    match request {
        ViewRequest::ApplyWindowGeometry => window::apply_window_geometry(ui),
        ViewRequest::HideWindow => {
            let _ = ui.window().hide();
        }
        ViewRequest::StartDrag => window::start_drag(ui),
        // WP-B（Slint 備援只做最基本的處理）
        ViewRequest::ShowWindow => {
            let _ = ui.window().show();
        }
        ViewRequest::Quit => {
            let _ = slint::quit_event_loop();
        }
    }
}

// ─── Entry ───────────────────────────────────────────────────────────────────

pub async fn launch(state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    // 抑制 Slint/parley→ICU4X 初始化的 stderr 雜訊
    let _icu_guard = IcuStderrGuard::new();
    let ui = AppWindow::new()?;

    // 設定唯一來源：AppConfig（config.toml）。
    let (config, config_store, startup_warnings) = {
        let s = state.read().await;
        let c = s.config.read().await.clone();
        (c, Arc::clone(&s.config), s.startup_warnings.clone())
    };
    let view = initial_state(&config, &startup_warnings).await;

    // 背景 → UI：喚醒時排入 Slint 事件迴圈
    let (notifier, queue) = event_channel(|| {
        let _ = slint::invoke_from_event_loop(drain_events);
    });

    let mut backend = RealBackend::new(
        config.clone(),
        config_store,
        view.selected_model_id.clone(),
        notifier,
    );
    backend.start_background(&view.hotkey, &view.ptt_hotkey, view.ptt_mode);

    // 在接上任何 callback 前把完整狀態推到 UI
    sync::push(&ui, &view, None);
    let mut controller = Controller::new(view, config, Box::new(backend));
    controller.start();
    sync::push(&ui, controller.state(), None);
    let last = controller.state().clone();
    ADAPTER.with(|cell| {
        *cell.borrow_mut() = Some(Adapter {
            controller,
            queue,
            last,
            ui: ui.as_weak(),
        });
    });

    wire_callbacks(&ui);
    // Set initial (idle) window icon after the window is ready.
    crate::ui::window_icon::update(false);

    let result = ui.run();
    ADAPTER.with(|cell| cell.borrow_mut().take());
    result?;
    Ok(())
}

fn wire_callbacks(ui: &AppWindow) {
    // 視窗
    let w = ui.as_weak();
    ui.on_toggle_floating_mode(move || {
        if let Some(ui) = w.upgrade() {
            dispatch(UiCommand::SetFloatingMode(ui.get_floating_mode()));
        }
    });
    let w = ui.as_weak();
    ui.on_toggle_floating_expanded(move || {
        if let Some(ui) = w.upgrade() {
            dispatch(UiCommand::SetFloatingExpanded(ui.get_floating_expanded()));
        }
    });
    ui.on_request_close(|| dispatch(UiCommand::RequestClose));
    ui.on_start_drag(|| dispatch(UiCommand::StartDrag));
    ui.on_clear_error_log(|| dispatch(UiCommand::ClearErrorLog));

    // 錄音 / 轉錄紀錄 / 模板
    ui.on_toggle_recording(|| dispatch(UiCommand::ToggleRecording));
    ui.on_clear_transcript(|| dispatch(UiCommand::ClearTranscript));
    ui.on_template_selected(|id| dispatch(UiCommand::TemplateSelected(id.to_string())));
    ui.on_template_save(|original, name, prompt| {
        dispatch(UiCommand::TemplateSave {
            original: original.to_string(),
            name: name.to_string(),
            prompt: prompt.to_string(),
        })
    });
    ui.on_template_delete(|name| dispatch(UiCommand::TemplateDelete(name.to_string())));

    // 設定 / 模型
    ui.on_settings_changed(|| dispatch(UiCommand::SettingsChanged));
    ui.on_save_settings(|| dispatch(UiCommand::SaveSettings));
    ui.on_open_model_manager(|| dispatch(UiCommand::OpenModelManager));
    ui.on_model_select_or_download(|id| dispatch(UiCommand::ModelSelectOrDownload(id.to_string())));

    // 檔案模式
    ui.on_open_file_dialog(|| dispatch(UiCommand::OpenFileDialog));
    ui.on_start_file_transcription(|| dispatch(UiCommand::StartFileTranscription));
    ui.on_file_copy_result(|| dispatch(UiCommand::FileCopyResult));
    ui.on_file_export_result(|fmt| dispatch(UiCommand::FileExportResult(fmt.to_string())));
}
