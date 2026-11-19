use super::persistence::ConfigPersistence;
use crate::context_templates::TemplateId;
use crate::output::OutputMode;
use crate::transcription::opencc::ConversionMode;
use crate::whisper::WhisperParams;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub theme: String,
    pub language_ui: String,
    pub start_minimized: bool,
    pub global_hotkey: String,
    /// 按住說話 (PTT) 快捷鍵
    pub ptt_hotkey: String,
    /// 是否啟用 PTT 模式
    pub ptt_mode: bool,
    pub auto_start: bool,
    /// 按下關閉鈕時縮到系統匣而非結束程式。
    pub close_to_tray: bool,
    /// 最小化時縮到系統匣（隱藏工作列按鈕）。
    pub minimize_to_tray: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            theme: "auto".into(),
            language_ui: "zh-TW".into(),
            start_minimized: false,
            global_hotkey: "Ctrl+Shift+W".into(),
            ptt_hotkey: "Ctrl+Alt+L".into(),
            ptt_mode: true,
            auto_start: false,
            close_to_tray: true,
            minimize_to_tray: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub device_name: Option<String>,
    pub vad_silence_timeout_ms: u64,
    pub vad_threshold: f32,
    pub max_segment_seconds: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            vad_silence_timeout_ms: crate::audio::VAD_DEFAULT_SILENCE_TIMEOUT_MS,
            vad_threshold: crate::audio::VAD_DEFAULT_THRESHOLD,
            max_segment_seconds: crate::audio::MAX_SEGMENT_SECONDS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChineseConfig {
    pub conversion: ConversionMode,
    pub add_punctuation: bool,
    pub remove_fillers: bool,
}

impl Default for ChineseConfig {
    fn default() -> Self {
        Self {
            conversion: ConversionMode::ZhTW,
            add_punctuation: true,
            remove_fillers: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub append_newline: bool,
    pub append_space: bool,
    pub clear_clipboard_after: bool,
    pub inject_delay_ms: u64,
    pub privacy_mode: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            mode: OutputMode::ClipboardAndInject,
            append_newline: true,
            append_space: false,
            clear_clipboard_after: false,
            inject_delay_ms: 50,
            privacy_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub auto_detect: bool,
    pub default_template: TemplateId,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            default_template: TemplateId::Casual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub max_entries: usize,
    pub save_audio: bool,
    pub export_format: String,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            save_audio: false,
            export_format: "txt".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleConfig {
    pub vad: bool,
    pub context_templates: bool,
    pub opencc: bool,
    pub auto_inject: bool,
    pub waveform: bool,
    pub history: bool,
    pub noise_suppress: bool,
    pub auto_punctuation: bool,
    pub speaker_detect: bool,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self::all_enabled()
    }
}

impl ModuleConfig {
    pub fn all_enabled() -> Self {
        Self {
            vad: true,
            context_templates: true,
            opencc: true,
            auto_inject: true,
            waveform: true,
            history: true,
            noise_suppress: false,
            auto_punctuation: true,
            speaker_detect: false,
        }
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        match id {
            "vad" => self.vad,
            "context_templates" => self.context_templates,
            "opencc" => self.opencc,
            "auto_inject" => self.auto_inject,
            "waveform" => self.waveform,
            "history" => self.history,
            "noise_suppress" => self.noise_suppress,
            "auto_punctuation" => self.auto_punctuation,
            "speaker_detect" => self.speaker_detect,
            _ => false,
        }
    }
}

/// 本機 whisper 引擎（預設）。
pub const ENGINE_LOCAL: &str = "local";
/// OpenAI 雲端轉錄（需使用者明確同意，見 [`TranscriptionConfig::openai_consent`]）。
pub const ENGINE_OPENAI: &str = "openai";
/// OpenAI 轉錄模型預設值。
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-live-transcribe";

/// 轉錄引擎。設定檔中以字串保存（`"local"` / `"openai"`），未知值一律視為本機。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranscriptionEngine {
    #[default]
    Local,
    OpenAi,
}

impl TranscriptionEngine {
    /// 字串 → 引擎；無法辨識時回退本機（隱私優先）。
    pub fn from_str_lossy(s: &str) -> Self {
        if s.eq_ignore_ascii_case(ENGINE_OPENAI) {
            Self::OpenAi
        } else {
            Self::Local
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => ENGINE_LOCAL,
            Self::OpenAi => ENGINE_OPENAI,
        }
    }
}

/// `[transcription]`：轉錄引擎選擇。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TranscriptionConfig {
    /// `"local"`（預設）或 `"openai"`。
    pub engine: String,
    /// OpenAI 轉錄模型名稱。
    pub openai_model: String,
    /// 使用者是否已明確同意把語音傳送到 OpenAI。未同意時一律使用本機引擎。
    pub openai_consent: bool,
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            engine: ENGINE_LOCAL.into(),
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            openai_consent: false,
        }
    }
}

/// 預設模型 ID（與 UI 初始值一致）
pub const DEFAULT_MODEL_ID: &str = "small";

/// 模型選擇
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    /// 目前選用的模型 ID（對應 ggml-{id}.bin）
    pub selected_id: String,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            selected_id: DEFAULT_MODEL_ID.into(),
        }
    }
}

/// 唯一的持久化設定結構（config.toml）。
/// UI 設定僅為此結構的記憶體快照，見 [`crate::settings::ui_settings::UiSettings`]。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub model: ModelConfig,
    pub transcription: TranscriptionConfig,
    pub audio: AudioConfig,
    pub whisper: WhisperParams,
    pub chinese: ChineseConfig,
    pub output: OutputConfig,
    pub context: ContextConfig,
    pub history: HistoryConfig,
    pub modules: ModuleConfig,
}

impl AppConfig {
    /// 從預設位置載入；檔案損毀時會備份為 `config.toml.bak` 並回傳預設值。
    /// 需要錯誤訊息時請改用 [`ConfigPersistence::load_or_recover`]。
    pub fn load_or_default() -> anyhow::Result<Self> {
        Ok(ConfigPersistence::default_location()?
            .load_or_recover()
            .config)
    }

    /// 寫入預設位置的 config.toml（唯一的設定檔）。
    pub fn save(&self) -> anyhow::Result<()> {
        ConfigPersistence::default_location()?.save(self)
    }

    /// config.toml 的完整路徑（%APPDATA%/whisper-typeless/config.toml）。
    pub fn config_path() -> anyhow::Result<std::path::PathBuf> {
        let base = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("無法取得設定目錄"))?;
        Ok(base.join("whisper-typeless").join("config.toml"))
    }

    pub fn models_dir() -> anyhow::Result<std::path::PathBuf> {
        let base = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        Ok(base.join("models"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_field_defaults() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.transcription.engine, "local");
        assert_eq!(cfg.transcription.openai_model, "gpt-live-transcribe");
        assert!(!cfg.transcription.openai_consent);
        assert!(cfg.general.close_to_tray);
        assert!(!cfg.general.minimize_to_tray);
    }

    #[test]
    fn old_config_without_new_fields_uses_defaults() {
        let cfg: AppConfig = toml::from_str("[general]\ntheme = \"dark\"\n").unwrap();
        assert_eq!(cfg.general.theme, "dark");
        assert!(cfg.general.close_to_tray);
        assert!(!cfg.general.minimize_to_tray);
        assert_eq!(cfg.transcription.engine, ENGINE_LOCAL);
        assert!(!cfg.transcription.openai_consent);
    }

    #[test]
    fn new_fields_round_trip_through_toml() {
        let mut cfg = AppConfig::default();
        cfg.transcription.engine = ENGINE_OPENAI.into();
        cfg.transcription.openai_model = "other-model".into();
        cfg.transcription.openai_consent = true;
        cfg.general.close_to_tray = false;
        cfg.general.minimize_to_tray = true;
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("[transcription]"));
        let back: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.transcription.engine, "openai");
        assert_eq!(back.transcription.openai_model, "other-model");
        assert!(back.transcription.openai_consent);
        assert!(!back.general.close_to_tray);
        assert!(back.general.minimize_to_tray);
    }

    #[test]
    fn engine_parse_falls_back_to_local() {
        assert_eq!(
            TranscriptionEngine::from_str_lossy("openai"),
            TranscriptionEngine::OpenAi
        );
        assert_eq!(
            TranscriptionEngine::from_str_lossy("local"),
            TranscriptionEngine::Local
        );
        assert_eq!(
            TranscriptionEngine::from_str_lossy("bogus"),
            TranscriptionEngine::Local
        );
        for e in [TranscriptionEngine::Local, TranscriptionEngine::OpenAi] {
            assert_eq!(TranscriptionEngine::from_str_lossy(e.as_str()), e);
        }
    }
}
