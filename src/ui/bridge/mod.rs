pub mod file_mode;
pub mod hotkey;
pub mod model;
pub mod ptt;
pub mod recording;
pub mod templates;
pub mod window;

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use tokio::{runtime::Handle as TokioHandle, sync::RwLock};
use tracing::error;

use crate::app::AppState;
use crate::settings::config::AppConfig;
use crate::settings::ui_settings::UiSettings;
use crate::transcription::opencc::{ConversionMode, OpenCCProcessor};
use crate::whisper::WhisperEngine;

use slint::ComponentHandle;

use crate::{AppWindow, TranscriptSegmentData};

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

            Some(Self { old_handle: raw_old, null_handle: raw_null })
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


pub(crate) struct LoadedWhisperModel {
    pub(crate) model_id: String,
    pub(crate) model_path: PathBuf,
    pub(crate) engine: WhisperEngine,
}

#[derive(Clone)]
pub(crate) struct ModelRuntime {
    pub(crate) cache: Arc<Mutex<Option<LoadedWhisperModel>>>,
    pub(crate) generation: Arc<AtomicU32>,
    pub(crate) download_in_progress: Arc<AtomicBool>,
}

impl ModelRuntime {
    pub(crate) fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU32::new(0)),
            download_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ─── Entry ───────────────────────────────────────────────────────────────────

pub async fn launch(state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    // 抑制 Slint/parley→ICU4X 初始化的 stderr 雜訊
    let _icu_guard = IcuStderrGuard::new();
    let ui = AppWindow::new()?;
    ui.set_app_title(crate::APP_NAME.into());

    // Restore UI settings from settings.json (if it exists) before any callbacks are wired.
    if let Some(saved) = UiSettings::load() {
        ui.set_vad_threshold(saved.vad_threshold);
        ui.set_silence_timeout(saved.silence_timeout_ms);
        ui.set_n_threads(saved.n_threads);
        ui.set_use_gpu(saved.use_gpu);
        ui.set_temperature(saved.temperature);
        ui.set_conversion_mode(saved.conversion_mode.into());
        ui.set_append_newline(saved.append_newline);
        ui.set_hotkey(saved.hotkey.into());
        ui.set_ptt_hotkey(saved.ptt_hotkey.into());
        ui.set_ptt_mode(saved.ptt_mode);
        ui.set_mod_vad(saved.mod_vad);
        ui.set_mod_context(saved.mod_context);
        ui.set_mod_opencc(saved.mod_opencc);
        ui.set_mod_inject(saved.mod_inject);
        ui.set_mod_waveform(saved.mod_waveform);
        ui.set_mod_history(saved.mod_history);
        ui.set_clipboard_enabled(saved.clipboard_enabled);
        ui.set_inject_enabled(saved.inject_enabled);
        if !saved.selected_model_id.is_empty() {
            ui.set_selected_model_id(saved.selected_model_id.into());
        }
    }

    let config = {
        let s = state.read().await;
        let c = s.config.read().await.clone();
        c
    };

    let selected_model_id: Arc<Mutex<String>> =
        Arc::new(Mutex::new(ui.get_selected_model_id().to_string()));
    let recording_handle: Arc<Mutex<Option<recording::RecordingHandle>>> =
        Arc::new(Mutex::new(None));
    let transcript_list: Arc<Mutex<Vec<TranscriptSegmentData>>> = Arc::new(Mutex::new(Vec::new()));
    let model_runtime = ModelRuntime::new();

    model::setup_model_entries(&ui, &config).await?;
    model::setup_model_callbacks(
        &ui,
        &config,
        Arc::clone(&selected_model_id),
        model_runtime.clone(),
    )?;
    if !model::preload_selected_model(
        &ui,
        &config,
        &model_runtime,
        ui.get_selected_model_id().to_string(),
    ) {
        model::spawn_model_select_or_download(
            TokioHandle::current(),
            ui.as_weak(),
            AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ui.get_selected_model_id().to_string(),
            Arc::clone(&selected_model_id),
            model_runtime.clone(),
            config.clone(),
            model::whisper_params_from_ui(&ui, &config),
            true,
        );
    }
    templates::setup_template_callbacks(&ui).await;
    recording::setup_recording_callbacks(
        &ui,
        config.clone(),
        Arc::clone(&selected_model_id),
        Arc::clone(&recording_handle),
        Arc::clone(&transcript_list),
        model_runtime.clone(),
    );
    file_mode::setup_file_mode_callbacks(
        &ui,
        &config,
        Arc::clone(&selected_model_id),
        model_runtime.clone(),
    )?;
    let hotkey_tx = hotkey::setup_global_hotkey(&ui, ui.get_hotkey().to_string());
    ptt::PTT_HOOK_ENABLED.store(ui.get_ptt_mode(), Ordering::Relaxed);
    let ptt_hotkey_tx = ptt::setup_ptt_hotkey(&ui, ui.get_ptt_hotkey().to_string());
    // Set initial (idle) window icon after the window is ready.
    crate::ui::window_icon::update(false);

    // Wire settings-changed → update toggle hotkey + PTT hotkey + PTT enabled flag
    {
        let ui_ref = ui.as_weak();
        let tx = hotkey_tx.clone();
        let ptt_tx = ptt_hotkey_tx.clone();
        ui.on_settings_changed(move || {
            if let Some(ui) = ui_ref.upgrade() {
                let _ = tx.send(ui.get_hotkey().to_string());
                let _ = ptt_tx.send(ui.get_ptt_hotkey().to_string());
                ptt::PTT_HOOK_ENABLED.store(ui.get_ptt_mode(), Ordering::Relaxed);
            }
        });
    }

    // Wire save-settings → serialize UI state → write settings.json
    {
        let ui_ref = ui.as_weak();
        ui.on_save_settings(move || {
            let Some(ui) = ui_ref.upgrade() else { return };

            let settings = UiSettings {
                vad_threshold: ui.get_vad_threshold(),
                silence_timeout_ms: ui.get_silence_timeout(),
                n_threads: ui.get_n_threads(),
                use_gpu: ui.get_use_gpu(),
                temperature: ui.get_temperature(),
                conversion_mode: ui.get_conversion_mode().to_string(),
                append_newline: ui.get_append_newline(),
                hotkey: ui.get_hotkey().to_string(),
                ptt_hotkey: ui.get_ptt_hotkey().to_string(),
                ptt_mode: ui.get_ptt_mode(),
                mod_vad: ui.get_mod_vad(),
                mod_context: ui.get_mod_context(),
                mod_opencc: ui.get_mod_opencc(),
                mod_inject: ui.get_mod_inject(),
                mod_waveform: ui.get_mod_waveform(),
                mod_history: ui.get_mod_history(),
                clipboard_enabled: ui.get_clipboard_enabled(),
                inject_enabled: ui.get_inject_enabled(),
                selected_model_id: ui.get_selected_model_id().to_string(),
            };

            match settings.save() {
                Ok(()) => {
                    ui.set_save_status("✓ 已儲存".into());
                    // Clear feedback after 2 s
                    let u = ui_ref.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = u.upgrade() {
                                if ui.get_save_status() == "✓ 已儲存" {
                                    ui.set_save_status("".into());
                                }
                            }
                        });
                    });
                }
                Err(e) => {
                    error!("設定儲存失敗: {e}");
                    ui.set_save_status("✗ 儲存失敗".into());
                }
            }
        });
    }

    // ── v2.2 新增：浮動視窗 ↔ 完整視窗切換 ─────────────────────
    window::setup_floating_mode_callbacks(&ui);

    ui.run()?;
    Ok(())
}

// ─── 中文轉換輔助 ──────────────────────────────────────────────────────────────

fn conversion_mode_from_str(mode: &str) -> ConversionMode {
    match mode {
        "zh-HK" => ConversionMode::ZhHK,
        "zh-CN" => ConversionMode::ZhCN,
        "raw" => ConversionMode::Raw,
        _ => ConversionMode::ZhTW,
    }
}

pub(super) fn convert_chinese_text(
    text: &str,
    opencc_enabled: bool,
    conversion_mode: &str,
) -> String {
    if !opencc_enabled {
        return text.to_string();
    }

    let mode = conversion_mode_from_str(conversion_mode);
    match OpenCCProcessor::new(mode) {
        Ok(processor) => processor.convert(text),
        Err(e) => {
            error!("OpenCC 轉換初始化失敗: {e}");
            text.to_string()
        }
    }
}
