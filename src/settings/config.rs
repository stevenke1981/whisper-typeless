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
