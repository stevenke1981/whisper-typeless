use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use tokio::{runtime::Handle as TokioHandle, sync::RwLock};
use tracing::{error, info};

pub mod hotkey;
pub mod ptt;
pub mod recording;
pub mod templates;

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



use crate::app::AppState;
use crate::file_transcription::{decode_audio_file, format_transcript, ExportFormat};
use crate::models::registry::ModelInfo;
use crate::models::{ModelDownloader, ModelManager};
use crate::settings::config::AppConfig;
use crate::settings::ui_settings::UiSettings;
use crate::transcription::opencc::{ConversionMode, OpenCCProcessor};
use crate::whisper::{WhisperEngine, WhisperParams};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, ModelEntry, TranscriptSegmentData};



struct LoadedWhisperModel {
    model_id: String,
    model_path: PathBuf,
    engine: WhisperEngine,
}

#[derive(Clone)]
pub(crate) struct ModelRuntime {
    cache: Arc<Mutex<Option<LoadedWhisperModel>>>,
    generation: Arc<AtomicU32>,
    download_in_progress: Arc<AtomicBool>,
}

impl ModelRuntime {
    fn new() -> Self {
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

    setup_model_entries(&ui, &config).await?;
    setup_model_callbacks(
        &ui,
        &config,
        Arc::clone(&selected_model_id),
        model_runtime.clone(),
    )?;
    if !preload_selected_model(
        &ui,
        &config,
        &model_runtime,
        ui.get_selected_model_id().to_string(),
    ) {
        spawn_model_select_or_download(
            TokioHandle::current(),
            ui.as_weak(),
            AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ui.get_selected_model_id().to_string(),
            Arc::clone(&selected_model_id),
            model_runtime.clone(),
            config.clone(),
            whisper_params_from_ui(&ui, &config),
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
    setup_file_mode_callbacks(
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
    setup_floating_mode_callbacks(&ui);

    ui.run()?;
    Ok(())
}

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

fn setup_floating_mode_callbacks(ui: &AppWindow) {
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
            if let Some(ui) = ui_ref.upgrade() {
                let empty: Vec<slint::SharedString> = Vec::new();
                ui.set_error_log(slint::ModelRc::new(slint::VecModel::from(empty)));
                ui.set_show_error_log(false);
            }
        });
    }
}

// ─── 模型列表 ─────────────────────────────────────────────────────────────────

fn model_entries_from(models: &[ModelInfo], _selected_id: &str) -> Vec<ModelEntry> {
    models
        .iter()
        .map(|m| ModelEntry {
            id: m.id.as_str().into(),
            display_name: m.display_name.as_str().into(),
            is_downloaded: m.is_downloaded,
            size_mb: m.size.disk_mb as i32,
            quality: m.quality_stars as i32,
            download_url: m.download_url.as_str().into(),
        })
        .collect()
}

async fn setup_model_entries(ui: &AppWindow, _config: &AppConfig) -> anyhow::Result<()> {
    let models_dir = AppConfig::models_dir()?;
    let manager = ModelManager::new(models_dir);
    let models = manager.list_available().await;

    let selected = ui.get_selected_model_id().to_string();
    let entries = model_entries_from(&models, &selected);
    ui.set_model_entries(ModelRc::new(VecModel::from(entries)));

    if let Some(info) = models.iter().find(|m| m.id == selected) {
        ui.set_model_name(info.display_name.as_str().into());
    }
    Ok(())
}

fn whisper_params_from_ui(ui: &AppWindow, config: &AppConfig) -> WhisperParams {
    let mut params = config.whisper.clone();
    params.use_gpu = ui.get_use_gpu();
    params.n_threads = ui.get_n_threads();
    params.temperature = ui.get_temperature();
    params
}

fn preload_selected_model(
    ui: &AppWindow,
    config: &AppConfig,
    runtime: &ModelRuntime,
    model_id: String,
) -> bool {
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let model_path = models_dir.join(format!("ggml-{model_id}.bin"));
    if !model_path.exists() {
        info!("略過模型預載，檔案不存在: {}", model_path.display());
        return false;
    }
    spawn_model_preload(
        runtime.clone(),
        model_id,
        model_path,
        whisper_params_from_ui(ui, config),
        ui.as_weak(),
    );
    true
}

fn spawn_model_preload(
    runtime: ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    whisper_params: WhisperParams,
    ui_weak: slint::Weak<AppWindow>,
) {
    let generation = runtime.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let label = model_id.clone();

    {
        let u = ui_weak.clone();
        let status = format!("預載模型 {label}...");
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                if !ui.get_is_recording() {
                    ui.set_status_text(status.into());
                }
            }
        });
    }

    std::thread::spawn(move || {
        info!("背景預載模型: {} ({})", model_id, model_path.display());
        match WhisperEngine::load(&model_path, &whisper_params) {
            Ok(engine) => {
                if runtime.generation.load(Ordering::SeqCst) != generation {
                    info!("丟棄過期模型預載結果: {}", model_id);
                    return;
                }

                let device = engine.device_label.clone();
                {
                    let mut cache = runtime.cache.lock().unwrap();
                    *cache = Some(LoadedWhisperModel {
                        model_id: model_id.clone(),
                        model_path: model_path.clone(),
                        engine,
                    });
                }

                let u = ui_weak.clone();
                let status = format!("模型已預載: {model_id}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_device_label(device.into());
                        if !ui.get_is_recording() {
                            ui.set_status_text(status.into());
                        }
                    }
                });
            }
            Err(e) => {
                error!("模型預載失敗: {e}");
                let u = ui_weak.clone();
                let msg = format!("模型預載失敗: {e}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        if !ui.get_is_recording() {
                            ui.set_status_text(msg.into());
                        }
                    }
                });
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_model_select_or_download(
    tokio_handle: TokioHandle,
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    model_id: String,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    _config: AppConfig,
    whisper_params: WhisperParams,
    confirm_if_missing: bool,
) {
    tokio_handle.spawn(async move {
        let manager = ModelManager::new(models_dir.clone());
        let models = manager.list_available().await;

        let Some(info) = models.iter().find(|m| m.id == model_id).cloned() else {
            let msg = format!("未知模型: {model_id}");
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_status_text(msg.into());
                }
            });
            return;
        };

        if info.is_downloaded {
            select_downloaded_model(
                ui_weak,
                models_dir,
                model_id,
                selected_model_id,
                runtime,
                whisper_params,
                models,
                "模型已切換",
            );
            return;
        }

        if confirm_if_missing {
            let display_name = info.display_name.clone();
            let size_mb = info.size.disk_mb;
            let result = rfd::AsyncMessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("模型尚未下載")
                .set_description(format!(
                    "語音辨識模型 {display_name} 尚未下載。\n\n大小約 {size_mb} MB，是否現在自動下載？\n下載完成後會自動選用並預載。"
                ))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                .await;

            if result != rfd::MessageDialogResult::Yes {
                let msg = format!("已取消下載模型 {display_name}");
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_status_text(msg.into());
                    }
                });
                return;
            }
        }

        download_model_and_select(
            ui_weak,
            models_dir,
            info,
            selected_model_id,
            runtime,
            whisper_params,
        )
        .await;
    });
}

#[allow(clippy::too_many_arguments)]
fn select_downloaded_model(
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    model_id: String,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    whisper_params: WhisperParams,
    models: Vec<ModelInfo>,
    status: &'static str,
) {
    *selected_model_id.lock().unwrap() = model_id.clone();

    let display = models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| model_id.clone());
    let entries = model_entries_from(&models, &model_id);
    let model_path = models_dir.join(format!("ggml-{model_id}.bin"));
    let ui_for_preload = ui_weak.clone();
    let id_for_ui = model_id.clone();

    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_downloading_id("".into());
            ui.set_download_progress(0.0);
            ui.set_selected_model_id(id_for_ui.into());
            ui.set_model_name(display.into());
            ui.set_status_text(status.into());
            ui.set_model_entries(ModelRc::new(VecModel::from(entries)));
        }
    });

    spawn_model_preload(
        runtime,
        model_id,
        model_path,
        whisper_params,
        ui_for_preload,
    );
}

#[allow(clippy::too_many_arguments)]
async fn download_model_and_select(
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    info: ModelInfo,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    whisper_params: WhisperParams,
) {
    let model_id = info.id.clone();
    let size_mb = info.size.disk_mb;

    if runtime
        .download_in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        let u = ui_weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_status_text("已有模型正在下載，請稍候...".into());
            }
        });
        return;
    }

    {
        let u = ui_weak.clone();
        let id_show = model_id.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_downloading_id(id_show.into());
                ui.set_download_progress(0.0);
                ui.set_status_text(format!("下載模型 ({size_mb} MB)...").into());
            }
        });
    }

    let downloader = ModelDownloader::new(models_dir.clone());
    let (tx, mut rx) =
        tokio::sync::mpsc::channel::<crate::models::downloader::DownloadProgress>(64);

    let ui_prog = ui_weak.clone();
    tokio::spawn(async move {
        while let Some(prog) = rx.recv().await {
            let ratio = prog
                .total_bytes
                .map(|t| prog.downloaded_bytes as f32 / t as f32)
                .unwrap_or(0.5_f32);
            let u = ui_prog.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_download_progress(ratio);
                }
            });
        }
    });

    match downloader.download(&info, tx).await {
        Ok(_) => {
            info!("模型 {model_id} 下載完成");
            let manager = ModelManager::new(models_dir.clone());
            let updated = manager.list_available().await;
            select_downloaded_model(
                ui_weak,
                models_dir,
                model_id,
                selected_model_id,
                runtime.clone(),
                whisper_params,
                updated,
                "模型下載完成，已選用",
            );
        }
        Err(e) => {
            error!("模型下載失敗: {e}");
            let msg = format!("下載失敗: {e}");
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_downloading_id("".into());
                    ui.set_download_progress(0.0);
                    ui.set_status_text(msg.into());
                }
            });
        }
    }
    runtime.download_in_progress.store(false, Ordering::SeqCst);
}

// ─── 模型選擇 / 下載 ──────────────────────────────────────────────────────────

fn setup_model_callbacks(
    ui: &AppWindow,
    config: &AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    model_runtime: ModelRuntime,
) -> anyhow::Result<()> {
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let ui_weak = ui.as_weak();
    let model_id_arc = Arc::clone(&selected_model_id);
    let config = config.clone();
    let tokio_handle = TokioHandle::current();

    ui.on_model_select_or_download(move |model_id_shared| {
        let params = ui_weak
            .upgrade()
            .map(|ui| whisper_params_from_ui(&ui, &config))
            .unwrap_or_else(|| config.whisper.clone());
        spawn_model_select_or_download(
            tokio_handle.clone(),
            ui_weak.clone(),
            models_dir.clone(),
            model_id_shared.to_string(),
            Arc::clone(&model_id_arc),
            model_runtime.clone(),
            config.clone(),
            params,
            false,
        );
    });

    // 開啟模型資料夾
    let mdir2 = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    ui.on_open_model_manager(move || {
        let dir = mdir2.clone();
        std::thread::spawn(move || {
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        });
    });

    Ok(())
}


// ─── 檔案模式 Callbacks ───────────────────────────────────────────────────────

fn setup_file_mode_callbacks(
    ui: &AppWindow,
    config: &AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    model_runtime: ModelRuntime,
) -> anyhow::Result<()> {
    let whisper_params = config.whisper.clone();
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let tokio_handle = TokioHandle::current();

    // 開啟檔案對話框
    let ui_handle = ui.as_weak();
    ui.on_open_file_dialog(move || {
        let ui_handle = ui_handle.clone();
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .add_filter(
                    "音訊/影片",
                    &[
                        "mp3", "mp4", "wav", "flac", "ogg", "m4a", "aac", "mkv", "mov", "avi",
                        "webm",
                    ],
                )
                .pick_file();

            if let Some(path) = result {
                let full_path = path.to_string_lossy().into_owned();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_file_name(full_path.into());
                        ui.set_file_has_result(false);
                        ui.set_file_result("".into());
                        ui.set_file_progress(0.0);
                    }
                });
            }
        });
    });

    // 開始轉錄
    let ui_handle = ui.as_weak();
    let params = whisper_params.clone();
    let mdir = models_dir.clone();
    let model_id_arc = Arc::clone(&selected_model_id);
    let runtime = model_runtime.clone();
    let cfg = config.clone();
    let tokio_handle2 = tokio_handle.clone();
    ui.on_start_file_transcription(move || {
        let ui_weak = ui_handle.clone();

        let (file_path, model_id, fmt_str, opencc_enabled, conversion_mode) = {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let full: String = ui.get_file_name().into();
            if full.is_empty() {
                return;
            }
            let model: String = ui.get_selected_model_id().into();
            let fmt: String = ui.get_file_output_format().into();
            (
                PathBuf::from(full),
                model,
                fmt,
                ui.get_mod_opencc(),
                ui.get_conversion_mode().to_string(),
            )
        };

        if !file_path.exists() {
            return;
        }

        let ui_weak2 = ui_weak.clone();
        let params2 = params.clone();
        let mdir2 = mdir.clone();
        let model_id_arc2 = Arc::clone(&model_id_arc);
        let runtime2 = runtime.clone();
        let cfg2 = cfg.clone();
        let tokio_handle3 = tokio_handle2.clone();

        std::thread::spawn(move || {
            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_processing(true);
                        ui.set_file_progress(0.05);
                    }
                });
            }

            let decoded = match decode_audio_file(&file_path) {
                Ok(d) => d,
                Err(e) => {
                    let msg = format!("解碼失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_progress(0.3);
                    }
                });
            }

            let id_lower = model_id.to_lowercase();
            let model_path = mdir2.join(format!("ggml-{}.bin", id_lower));

            if !model_path.exists() {
                let msg = format!(
                    "模型尚未下載: {}\n已開啟下載確認，下載完成後請再開始轉錄。",
                    model_path.display()
                );
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_processing(false);
                        ui.set_file_result(msg.into());
                        ui.set_file_has_result(true);
                    }
                });
                spawn_model_select_or_download(
                    tokio_handle3,
                    ui_weak2,
                    mdir2,
                    id_lower,
                    model_id_arc2,
                    runtime2,
                    cfg2,
                    params2.clone(),
                    true,
                );
                return;
            }

            let engine = match WhisperEngine::load(&model_path, &params2) {
                Ok(e) => e,
                Err(e) => {
                    let msg = format!("模型載入失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_progress(0.5);
                    }
                });
            }

            let audio_seg = crate::audio::AudioSegment {
                samples: decoded.samples,
                duration_ms: decoded.duration_ms,
            };

            let result = match engine.transcribe(&audio_seg, &params2) {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("轉錄失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            let fmt = ExportFormat::from_extension(&fmt_str);
            let text = convert_chinese_text(
                &format_transcript(&result.segments, fmt),
                opencc_enabled,
                &conversion_mode,
            );

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak2.upgrade() {
                    ui.set_file_progress(1.0);
                    ui.set_file_processing(false);
                    ui.set_file_result(text.into());
                    ui.set_file_has_result(true);
                }
            });
        });
    });

    // 複製結果
    let ui_handle = ui.as_weak();
    ui.on_file_copy_result(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let text: String = ui.get_file_result().into();
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text);
            }
        }
    });

    // 匯出結果
    let ui_handle = ui.as_weak();
    ui.on_file_export_result(move |fmt_shared| {
        let fmt_str: String = fmt_shared.into();
        let fmt = ExportFormat::from_extension(&fmt_str);

        if let Some(ui) = ui_handle.upgrade() {
            let text: String = ui.get_file_result().into();
            let file_name: String = ui.get_file_name().into();

            let stem = PathBuf::from(&file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("transcript")
                .to_string();

            std::thread::spawn(move || {
                let save_path = rfd::FileDialog::new()
                    .set_file_name(format!("{}.{}", stem, fmt.extension()))
                    .add_filter(fmt.extension().to_uppercase().as_str(), &[fmt.extension()])
                    .save_file();

                if let Some(path) = save_path {
                    if let Err(e) = std::fs::write(&path, &text) {
                        error!("匯出失敗: {e}");
                    } else {
                        info!("已匯出至: {}", path.display());
                    }
                }
            });
        }
    });

    Ok(())
}

fn conversion_mode_from_str(mode: &str) -> ConversionMode {
    match mode {
        "zh-HK" => ConversionMode::ZhHK,
        "zh-CN" => ConversionMode::ZhCN,
        "raw" => ConversionMode::Raw,
        _ => ConversionMode::ZhTW,
    }
}

fn convert_chinese_text(text: &str, opencc_enabled: bool, conversion_mode: &str) -> String {
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
