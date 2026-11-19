//! 控制器的副作用介面（[`Backend`]）。
//!
//! 正式版由 [`services::RealBackend`](super::services::RealBackend) 實作（音訊、whisper、
//! 檔案、熱鍵）；測試用假實作只記錄呼叫，不碰硬體。所有長時間工作都在背景執行，
//! 結果以 [`CoreEvent`](super::CoreEvent) 透過 [`Notifier`](super::Notifier) 回報。

use crate::settings::config::TranscriptionEngine;
use crate::settings::secret::Secret;
use crate::settings::ui_settings::UiSettings;
use crate::whisper::WhisperParams;

/// 開始錄音所需的參數（取自目前畫面狀態）。
#[derive(Debug, Clone)]
pub struct RecordingRequest {
    pub privacy_mode: bool,
    pub vad_threshold: f32,
    pub vad_timeout_ms: u64,
    pub clipboard_enabled: bool,
    pub inject_enabled: bool,
    pub opencc_enabled: bool,
    pub conversion_mode: String,
    pub whisper_params: WhisperParams,
    /// 轉錄引擎。只有使用者已同意（`openai_consent`）時才會是 `OpenAi`。
    pub engine: TranscriptionEngine,
    /// OpenAI 轉錄模型名稱（`engine` 為 `OpenAi` 時使用）。
    pub openai_model: String,
}

/// [`Backend::start_recording`] 的結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingStart {
    /// 已開始錄音。
    Started,
    /// 選用的模型尚未下載。
    ModelMissing { model_id: String },
    /// 無法開始（音訊裝置錯誤等；訊息顯示於狀態列）。
    Failed(String),
}

/// 檔案轉錄參數。
#[derive(Debug, Clone)]
pub struct FileTranscriptionRequest {
    pub file_path: String,
    pub model_id: String,
    pub output_format: String,
    pub opencc_enabled: bool,
    pub conversion_mode: String,
    pub whisper_params: WhisperParams,
}

/// 控制器需要的所有副作用。
pub trait Backend {
    /// 目前是否持有錄音串流。
    fn is_recording(&self) -> bool;
    /// 開始錄音（模型由後端目前選用的模型決定）。
    fn start_recording(&mut self, req: RecordingRequest) -> RecordingStart;
    /// 停止錄音；剩餘音訊會在背景轉錄並以事件回報。
    fn stop_recording(&mut self);
    /// 選用模型；未下載時（`confirm_if_missing` 為 true 會先詢問）下載後選用並預載。
    fn select_or_download_model(
        &mut self,
        model_id: String,
        params: WhisperParams,
        confirm_if_missing: bool,
    );
    /// 背景預載模型；模型檔不存在時回傳 false。
    fn preload_model(&mut self, model_id: String, params: WhisperParams) -> bool;
    /// 寫入設定檔。成功時後端需在 2 秒後送出 `CoreEvent::SaveStatusExpired`。
    fn save_settings(&mut self, settings: &UiSettings) -> Result<(), String>;
    /// 重新套用切換熱鍵、PTT 熱鍵與 PTT 開關。
    fn apply_hotkeys(&mut self, hotkey: &str, ptt_hotkey: &str, ptt_enabled: bool);
    /// 開啟模型資料夾。
    fn open_model_folder(&mut self);
    /// 開啟檔案選擇對話框；結果以 `CoreEvent::FilePicked` 回報。
    fn pick_audio_file(&mut self);
    /// 開始檔案轉錄。
    fn start_file_transcription(&mut self, req: FileTranscriptionRequest);
    /// 複製文字到剪貼簿。
    fn copy_text(&mut self, text: String);
    /// 匯出文字（另存新檔對話框）。
    fn export_text(&mut self, text: String, source_file: String, format: String);
    /// 新增 / 修改模板。
    fn save_template(&mut self, original: String, name: String, prompt: String);
    /// 刪除模板。
    fn delete_template(&mut self, name: String);
    /// 清空全域錯誤紀錄。
    fn clear_error_log(&mut self);
    /// 全域錯誤紀錄快照（新到舊）。
    fn error_log_snapshot(&self) -> Vec<String>;
    /// 儲存（`Some`）或刪除（`None`）OpenAI 金鑰（DPAPI 加密檔，不寫入 config.toml）。
    fn store_api_key(&mut self, key: Option<Secret>) -> Result<(), String>;
    /// 是否有可用的 OpenAI 金鑰（已儲存的金鑰或環境變數 `OPENAI_API_KEY`）。
    fn has_api_key(&self) -> bool;
}
