use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// All settings that are visible and editable through the UI.
/// Saved as `settings.json` in the working directory (project root when using `cargo run`,
/// or the directory the executable is launched from when deployed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    // Audio
    pub vad_threshold: f32,
    pub silence_timeout_ms: f32,

    // Whisper engine
    pub n_threads: i32,
    pub use_gpu: bool,
    pub temperature: f32,

    // Chinese processing
    pub conversion_mode: String,

    // Hotkeys
    pub hotkey: String,
    pub ptt_hotkey: String,
    pub ptt_mode: bool,

    // Module toggles
    pub mod_vad: bool,
    pub mod_context: bool,
    pub mod_opencc: bool,
    pub mod_inject: bool,
    pub mod_waveform: bool,
    pub mod_history: bool,

    // Output
    pub append_newline: bool,
    pub clipboard_enabled: bool,
    pub inject_enabled: bool,

    // Model
    pub selected_model_id: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            vad_threshold: 0.6,
            silence_timeout_ms: 1500.0,
            n_threads: 4,
            use_gpu: true,
            temperature: 0.0,
            conversion_mode: "zh-TW".into(),
            hotkey: "Ctrl+Shift+W".into(),
            ptt_hotkey: "Ctrl+Alt+L".into(),
            ptt_mode: true,
            mod_vad: true,
            mod_context: true,
            mod_opencc: true,
            mod_inject: true,
            mod_waveform: true,
            mod_history: true,
            append_newline: true,
            clipboard_enabled: true,
            inject_enabled: true,
            selected_model_id: "medium".into(),
        }
    }
}

impl UiSettings {
    /// Path to `settings.json` in the current working directory
    /// (the project root when launched via `cargo run`, or the directory the exe is launched from).
    pub fn settings_path() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("settings.json")
    }

    /// Load from `settings.json`. Returns `None` if the file does not exist or cannot be parsed.
    pub fn load() -> Option<Self> {
        let path = Self::settings_path();
        let content = std::fs::read_to_string(&path).ok()?;
        let s: Self = serde_json::from_str(&content)
            .map_err(|e| tracing::warn!("settings.json 解析失敗: {e}"))
            .ok()?;
        tracing::info!("已載入設定: {:?}", path);
        Some(s)
    }

    /// Write to `settings.json`, pretty-printed.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::settings_path();
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        tracing::info!("設定已儲存: {:?}", path);
        Ok(())
    }
}
