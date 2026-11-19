//! 與 UI 框架無關的畫面狀態（[`AppViewState`]）。
//!
//! 每個欄位對應 Slint `AppWindow` 上一個 bridge 會設定或讀取的屬性；
//! 預設值與 Slint 主視窗（ui/appwindow）的屬性預設值一致。

use crate::app::RecordingState;
use crate::settings::ui_settings::UiSettings;

/// 轉錄紀錄中的一段文字（對應 Slint `TranscriptSegmentData`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranscriptSegment {
    pub text: String,
    pub timestamp: String,
}

/// 模型清單中的一列（對應 Slint `ModelEntry`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelEntryView {
    pub id: String,
    pub display_name: String,
    pub is_downloaded: bool,
    pub size_mb: i32,
    pub quality: i32,
    pub download_url: String,
}

/// 情境模板編輯清單中的一列（對應 Slint `TemplateEditEntry`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TemplateEntryView {
    pub name: String,
    pub prompt: String,
    pub is_builtin: bool,
}

/// 主視窗完整狀態的純資料鏡像。
#[derive(Debug, Clone, PartialEq)]
pub struct AppViewState {
    // ── 視窗 ──
    pub app_title: String,
    pub floating_mode: bool,
    pub floating_expanded: bool,
    pub show_error_log: bool,
    pub error_log: Vec<String>,
    pub settings_open: bool,
    pub dark_mode: bool,

    // ── 即時錄音 ──
    pub is_recording: bool,
    pub is_processing: bool,
    pub current_template: String,
    pub current_partial: String,
    pub transcript_segments: Vec<TranscriptSegment>,
    pub audio_level: f32,
    pub waveform_bars: Vec<f32>,
    pub status_text: String,
    pub char_count: i32,
    pub device_label: String,

    // ── 模型 ──
    pub model_name: String,
    pub model_entries: Vec<ModelEntryView>,
    pub selected_model_id: String,
    pub downloading_id: String,
    pub download_progress: f32,

    // ── 模組開關 ──
    pub mod_vad: bool,
    pub mod_context: bool,
    pub mod_opencc: bool,
    pub mod_inject: bool,
    pub mod_waveform: bool,
    pub mod_history: bool,
    pub clipboard_enabled: bool,
    pub inject_enabled: bool,

    // ── 設定 ──
    pub vad_threshold: f32,
    pub silence_timeout: f32,
    pub n_threads: i32,
    pub use_gpu: bool,
    pub temperature: f32,
    pub conversion_mode: String,
    pub append_newline: bool,
    pub privacy_mode: bool,
    pub hotkey: String,
    pub ptt_hotkey: String,
    pub ptt_mode: bool,
    pub save_status: String,

    // ── 轉錄引擎（WP-A）──
    /// `"local"` / `"openai"`。
    pub engine: String,
    pub openai_model: String,
    pub openai_consent: bool,
    /// 是否有可用的 OpenAI 金鑰（只存在狀態中，不寫入設定檔）。
    pub openai_key_set: bool,

    // ── 視窗 / 系統匣（WP-B）──
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub start_minimized: bool,
    /// 視窗目前縮在系統匣（只存在狀態中）。
    pub window_hidden: bool,
    /// 使用者要求結束程式（只存在狀態中；畫面層看到後關閉事件迴圈）。
    pub quit_requested: bool,

    // ── 檔案模式 ──
    pub file_mode: bool,
    pub file_name: String,
    pub file_progress: f32,
    pub file_processing: bool,
    pub file_has_result: bool,
    pub file_result: String,
    pub file_output_format: String,

    // ── 情境模板 ──
    pub template_entries: Vec<TemplateEntryView>,
}

impl Default for AppViewState {
    /// 與 Slint 主視窗（ui/appwindow）的屬性預設值相同。
    fn default() -> Self {
        Self {
            app_title: "whisper-typeless".into(),
            floating_mode: false,
            floating_expanded: false,
            show_error_log: false,
            error_log: Vec::new(),
            settings_open: false,
            dark_mode: false,

            is_recording: false,
            is_processing: false,
            current_template: "auto".into(),
            current_partial: String::new(),
            transcript_segments: Vec::new(),
            audio_level: 0.0,
            waveform_bars: Vec::new(),
            status_text: "就緒".into(),
            char_count: 0,
            device_label: "CPU".into(),

            model_name: "small".into(),
            model_entries: Vec::new(),
            selected_model_id: "small".into(),
            downloading_id: String::new(),
            download_progress: 0.0,

            mod_vad: true,
            mod_context: true,
            mod_opencc: true,
            mod_inject: true,
            mod_waveform: true,
            mod_history: true,
            clipboard_enabled: true,
            inject_enabled: true,

            vad_threshold: 0.6,
            silence_timeout: 1500.0,
            n_threads: 4,
            use_gpu: true,
            temperature: 0.0,
            conversion_mode: "zh-TW".into(),
            append_newline: true,
            privacy_mode: false,
            hotkey: "Ctrl+Shift+W".into(),
            ptt_hotkey: "Ctrl+Alt+L".into(),
            ptt_mode: true,
            save_status: String::new(),

            engine: "local".into(),
            openai_model: "gpt-live-transcribe".into(),
            openai_consent: false,
            openai_key_set: false,

            close_to_tray: true,
            minimize_to_tray: false,
            start_minimized: false,
            window_hidden: false,
            quit_requested: false,

            file_mode: false,
            file_name: String::new(),
            file_progress: 0.0,
            file_processing: false,
            file_has_result: false,
            file_result: String::new(),
            file_output_format: "txt".into(),

            template_entries: Vec::new(),
        }
    }
}

impl AppViewState {
    /// 由設定快照建立狀態（其餘欄位取預設值）。
    pub fn from_settings(s: &UiSettings) -> Self {
        let mut state = Self::default();
        state.apply_ui_settings(s);
        state
    }

    /// 將設定快照寫入狀態（等同舊版 `apply_ui_settings`）。
    pub fn apply_ui_settings(&mut self, s: &UiSettings) {
        self.vad_threshold = s.vad_threshold;
        self.silence_timeout = s.silence_timeout_ms;
        self.n_threads = s.n_threads;
        self.use_gpu = s.use_gpu;
        self.temperature = s.temperature;
        self.conversion_mode = s.conversion_mode.clone();
        self.append_newline = s.append_newline;
        self.hotkey = s.hotkey.clone();
        self.ptt_hotkey = s.ptt_hotkey.clone();
        self.ptt_mode = s.ptt_mode;
        self.mod_vad = s.mod_vad;
        self.mod_context = s.mod_context;
        self.mod_opencc = s.mod_opencc;
        self.mod_inject = s.mod_inject;
        self.mod_waveform = s.mod_waveform;
        self.mod_history = s.mod_history;
        self.clipboard_enabled = s.clipboard_enabled;
        self.inject_enabled = s.inject_enabled;
        self.privacy_mode = s.privacy_mode;
        if !s.selected_model_id.is_empty() {
            self.selected_model_id = s.selected_model_id.clone();
        }
        self.engine = s.engine.clone();
        self.openai_model = s.openai_model.clone();
        self.openai_consent = s.openai_consent;
        self.close_to_tray = s.close_to_tray;
        self.minimize_to_tray = s.minimize_to_tray;
        self.start_minimized = s.start_minimized;
        self.dark_mode = s.dark_mode;
    }

    /// 從狀態讀出設定快照（等同舊版 `ui_settings_from_window`）。
    pub fn ui_settings(&self) -> UiSettings {
        UiSettings {
            vad_threshold: self.vad_threshold,
            silence_timeout_ms: self.silence_timeout,
            n_threads: self.n_threads,
            use_gpu: self.use_gpu,
            temperature: self.temperature,
            conversion_mode: self.conversion_mode.clone(),
            hotkey: self.hotkey.clone(),
            ptt_hotkey: self.ptt_hotkey.clone(),
            ptt_mode: self.ptt_mode,
            mod_vad: self.mod_vad,
            mod_context: self.mod_context,
            mod_opencc: self.mod_opencc,
            mod_inject: self.mod_inject,
            mod_waveform: self.mod_waveform,
            mod_history: self.mod_history,
            append_newline: self.append_newline,
            clipboard_enabled: self.clipboard_enabled,
            inject_enabled: self.inject_enabled,
            privacy_mode: self.privacy_mode,
            selected_model_id: self.selected_model_id.clone(),
            engine: self.engine.clone(),
            openai_model: self.openai_model.clone(),
            openai_consent: self.openai_consent,
            close_to_tray: self.close_to_tray,
            minimize_to_tray: self.minimize_to_tray,
            start_minimized: self.start_minimized,
            dark_mode: self.dark_mode,
        }
    }

    /// 啟動警告（設定檔損毀等）→ 錯誤紀錄 + 狀態列。
    pub fn show_startup_warnings(&mut self, warnings: &[String]) {
        if warnings.is_empty() {
            return;
        }
        self.error_log = warnings.to_vec();
        self.status_text = "⚠ 設定載入有問題，詳見錯誤紀錄".into();
    }

    /// 由 `is_recording` / `is_processing` 推得的錄音狀態。
    pub fn recording_state(&self) -> RecordingState {
        if self.is_recording {
            RecordingState::Recording
        } else if self.is_processing {
            RecordingState::Processing
        } else {
            RecordingState::Idle
        }
    }

    /// 轉錄紀錄總字數。
    pub fn transcript_char_count(&self) -> i32 {
        self.transcript_segments
            .iter()
            .map(|s| s.text.chars().count() as i32)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_state() {
        let s = UiSettings {
            privacy_mode: true,
            hotkey: "Ctrl+Alt+H".into(),
            n_threads: 7,
            selected_model_id: "base".into(),
            engine: "openai".into(),
            openai_model: "m".into(),
            openai_consent: true,
            close_to_tray: false,
            minimize_to_tray: true,
            start_minimized: true,
            dark_mode: true,
            ..UiSettings::default()
        };
        let state = AppViewState::from_settings(&s);
        assert!(state.dark_mode);
        assert_eq!(state.ui_settings(), s);
    }

    #[test]
    fn view_state_defaults_match_settings_defaults() {
        let state = AppViewState::default();
        let s = UiSettings::default();
        assert_eq!(state.engine, s.engine);
        assert_eq!(state.openai_model, s.openai_model);
        assert_eq!(state.openai_consent, s.openai_consent);
        assert_eq!(state.close_to_tray, s.close_to_tray);
        assert_eq!(state.minimize_to_tray, s.minimize_to_tray);
        assert_eq!(state.start_minimized, s.start_minimized);
        assert!(!state.openai_key_set && !state.window_hidden && !state.quit_requested);
    }

    #[test]
    fn empty_selected_model_keeps_default() {
        let s = UiSettings {
            selected_model_id: String::new(),
            ..UiSettings::default()
        };
        let state = AppViewState::from_settings(&s);
        assert_eq!(state.selected_model_id, "small");
    }

    #[test]
    fn startup_warnings_populate_error_log() {
        let mut state = AppViewState::default();
        state.show_startup_warnings(&[]);
        assert_eq!(state.status_text, "就緒");
        state.show_startup_warnings(&["壞掉".to_string()]);
        assert_eq!(state.error_log, vec!["壞掉".to_string()]);
        assert!(state.status_text.contains("設定載入有問題"));
    }

    #[test]
    fn recording_state_derivation() {
        let mut state = AppViewState::default();
        assert_eq!(state.recording_state(), RecordingState::Idle);
        state.is_processing = true;
        assert_eq!(state.recording_state(), RecordingState::Processing);
        state.is_recording = true;
        assert_eq!(state.recording_state(), RecordingState::Recording);
    }
}
