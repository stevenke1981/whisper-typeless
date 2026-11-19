//! UI 無關的服務層：錄音管線、模型、檔案模式、模板、全域熱鍵 / PTT，
//! 以及把它們組成 [`Backend`] 的 [`RealBackend`]。

pub mod file_mode;
pub mod hotkey;
pub mod model;
pub mod ptt;
pub mod recording;
pub mod templates;

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use tokio::{runtime::Handle as TokioHandle, sync::RwLock};
use tracing::error;

use crate::core::backend::{Backend, FileTranscriptionRequest, RecordingRequest, RecordingStart};
use crate::core::event::CoreEvent;
use crate::core::state::AppViewState;
use crate::core::Notifier;
use crate::settings::config::AppConfig;
use crate::settings::secret::Secret;
use crate::settings::ui_settings::{conversion_mode_from_str, UiSettings};
use crate::transcription::opencc::OpenCCProcessor;
use crate::whisper::{WhisperEngine, WhisperParams};

// ─── 模型快取 ─────────────────────────────────────────────────────────────────

pub(crate) struct LoadedWhisperModel {
    pub(crate) model_id: String,
    pub(crate) model_path: PathBuf,
    pub(crate) engine: WhisperEngine,
}

/// 已載入模型快取 + 預載世代 + 下載中旗標（跨執行緒共用）。
#[derive(Clone)]
pub struct ModelRuntime {
    pub(crate) cache: Arc<Mutex<Option<LoadedWhisperModel>>>,
    pub(crate) generation: Arc<AtomicU32>,
    pub(crate) download_in_progress: Arc<AtomicBool>,
}

impl ModelRuntime {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU32::new(0)),
            download_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for ModelRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// 模型相關背景工作的共用環境。
#[derive(Clone)]
pub(crate) struct ModelContext {
    pub(crate) tokio: TokioHandle,
    pub(crate) notifier: Notifier,
    pub(crate) models_dir: PathBuf,
    /// 錄音實際使用的模型（選用 / 下載完成才更新）。
    pub(crate) selected_model_id: Arc<Mutex<String>>,
    pub(crate) runtime: ModelRuntime,
}

// ─── 中文轉換輔助 ──────────────────────────────────────────────────────────────

pub(crate) fn convert_chinese_text(
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

fn models_dir() -> PathBuf {
    AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."))
}

// ─── 啟動 ────────────────────────────────────────────────────────────────────

/// 建立啟動時的畫面狀態：設定、啟動警告、模型清單、模板清單。
pub async fn initial_state(config: &AppConfig, startup_warnings: &[String]) -> AppViewState {
    let mut state = AppViewState::from_settings(&UiSettings::from_config(config));
    state.app_title = crate::APP_NAME.into();
    state.show_startup_warnings(startup_warnings);

    match AppConfig::models_dir() {
        Ok(dir) => {
            let (entries, name) = model::list_model_entries(dir, &state.selected_model_id).await;
            state.model_entries = entries;
            if let Some(name) = name {
                state.model_name = name;
            }
        }
        Err(e) => error!("無法取得模型資料夾: {e}"),
    }

    let templates = templates::load_templates_from_disk().await;
    state.template_entries = templates::templates_to_entries(&templates);
    state
}

// ─── RealBackend ─────────────────────────────────────────────────────────────

/// 正式版 [`Backend`]：真的錄音、載入模型、讀寫檔案。
pub struct RealBackend {
    config: AppConfig,
    config_store: Arc<RwLock<AppConfig>>,
    models: ModelContext,
    recording: Option<recording::RecordingHandle>,
    hotkey_tx: Option<std::sync::mpsc::Sender<String>>,
    ptt_tx: Option<std::sync::mpsc::Sender<String>>,
}

impl RealBackend {
    /// `config`：啟動設定快照；`config_store`：可寫入的設定（存檔用）；
    /// `selected_model_id`：啟動時選用的模型。需在 tokio runtime 內呼叫。
    pub fn new(
        config: AppConfig,
        config_store: Arc<RwLock<AppConfig>>,
        selected_model_id: String,
        notifier: Notifier,
    ) -> Self {
        Self {
            config,
            config_store,
            models: ModelContext {
                tokio: TokioHandle::current(),
                notifier,
                models_dir: models_dir(),
                selected_model_id: Arc::new(Mutex::new(selected_model_id)),
                runtime: ModelRuntime::new(),
            },
            recording: None,
            hotkey_tx: None,
            ptt_tx: None,
        }
    }

    /// 啟動全域熱鍵與 PTT hook，並把全域錯誤紀錄接到事件通道。
    pub fn start_background(&mut self, hotkey: &str, ptt_hotkey: &str, ptt_enabled: bool) {
        let notifier = self.models.notifier.clone();
        self.hotkey_tx = Some(hotkey::spawn_global_hotkey(
            hotkey.to_string(),
            notifier.clone(),
        ));
        ptt::PTT_HOOK_ENABLED.store(ptt_enabled, Ordering::Relaxed);
        self.ptt_tx = Some(ptt::spawn_ptt_hotkey(
            ptt_hotkey.to_string(),
            notifier.clone(),
        ));

        // 全域錯誤紀錄（tracing WARN/ERROR）→ 畫面；啟動前已發生的錯誤也要顯示
        notifier.send(CoreEvent::ErrorLogChanged);
        let n = notifier;
        crate::ui::error_log::global().set_listener(move || n.send(CoreEvent::ErrorLogChanged));
    }
}

impl Backend for RealBackend {
    fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    fn start_recording(&mut self, req: RecordingRequest) -> RecordingStart {
        let mut model_id = self.models.selected_model_id.lock().unwrap().clone();
        let mut path = model::model_path(&self.models.models_dir, &model_id);
        // 選用的模型遺失時，自動改用已下載的最小模型，而不是跳出下載提示。
        if req.engine == crate::settings::config::TranscriptionEngine::Local && !path.exists() {
            if let Some(fallback) = model::fallback_model_id(&self.models.models_dir, &model_id) {
                self.models.notifier.send(CoreEvent::Status(format!(
                    "模型 {model_id} 尚未下載，改用已下載的 {fallback}"
                )));
                *self.models.selected_model_id.lock().unwrap() = fallback.clone();
                model_id = fallback;
                path = model::model_path(&self.models.models_dir, &model_id);
            }
        }
        match recording::start_recording(
            &self.models.tokio,
            &self.config,
            model_id.clone(),
            path,
            req,
            self.models.runtime.clone(),
            self.models.notifier.clone(),
        ) {
            Ok(handle) => {
                self.recording = Some(handle);
                RecordingStart::Started
            }
            Err(recording::StartError::ModelMissing) => RecordingStart::ModelMissing { model_id },
            Err(recording::StartError::Failed(msg)) => RecordingStart::Failed(msg),
        }
    }

    fn stop_recording(&mut self) {
        if let Some(handle) = self.recording.take() {
            handle.stop();
        }
    }

    fn select_or_download_model(
        &mut self,
        model_id: String,
        params: WhisperParams,
        confirm_if_missing: bool,
    ) {
        self.models
            .spawn_select_or_download(model_id, params, confirm_if_missing);
    }

    fn preload_model(&mut self, model_id: String, params: WhisperParams) -> bool {
        let path = model::model_path(&self.models.models_dir, &model_id);
        if !path.exists() {
            tracing::info!("略過模型預載，檔案不存在: {}", path.display());
            return false;
        }
        model::spawn_model_preload(
            self.models.runtime.clone(),
            model_id,
            path,
            params,
            self.models.notifier.clone(),
        );
        true
    }

    fn save_settings(&mut self, settings: &UiSettings) -> Result<(), String> {
        let result = match self.config_store.try_write() {
            Ok(mut cfg) => {
                settings.apply_to(&mut cfg);
                cfg.save().map_err(|e| e.to_string())
            }
            Err(_) => Err("設定正被其他工作使用中，請稍後再試".to_string()),
        };
        if result.is_ok() {
            // 2 秒後清除「✓ 已儲存」
            let n = self.models.notifier.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                n.send(CoreEvent::SaveStatusExpired);
            });
        }
        result
    }

    fn apply_hotkeys(&mut self, hotkey: &str, ptt_hotkey: &str, ptt_enabled: bool) {
        if let Some(tx) = &self.hotkey_tx {
            let _ = tx.send(hotkey.to_string());
        }
        if let Some(tx) = &self.ptt_tx {
            let _ = tx.send(ptt_hotkey.to_string());
        }
        ptt::PTT_HOOK_ENABLED.store(ptt_enabled, Ordering::Relaxed);
    }

    fn open_model_folder(&mut self) {
        let dir = self.models.models_dir.clone();
        std::thread::spawn(move || {
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            #[cfg(not(target_os = "windows"))]
            let _ = dir;
        });
    }

    fn pick_audio_file(&mut self) {
        file_mode::spawn_pick_file(self.models.notifier.clone());
    }

    fn start_file_transcription(&mut self, req: FileTranscriptionRequest) {
        file_mode::spawn_file_transcription(req, self.models.clone());
    }

    fn copy_text(&mut self, text: String) {
        file_mode::copy_text(text);
    }

    fn export_text(&mut self, text: String, source_file: String, format: String) {
        file_mode::spawn_export(text, source_file, format);
    }

    fn save_template(&mut self, original: String, name: String, prompt: String) {
        templates::spawn_save_template(
            &self.models.tokio,
            self.models.notifier.clone(),
            original,
            name,
            prompt,
        );
    }

    fn delete_template(&mut self, name: String) {
        templates::spawn_delete_template(&self.models.tokio, self.models.notifier.clone(), name);
    }

    fn clear_error_log(&mut self) {
        // 清空緩衝；listener 會再送一次 ErrorLogChanged
        crate::ui::error_log::global().clear();
    }

    fn error_log_snapshot(&self) -> Vec<String> {
        crate::ui::error_log::global().snapshot_newest_first()
    }

    // WP-A：改為作業系統憑證存放區。
    fn store_api_key(&mut self, _key: Option<Secret>) -> Result<(), String> {
        Err("尚未實作".into())
    }

    fn has_api_key(&self) -> bool {
        false
    }
}
