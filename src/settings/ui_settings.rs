//! UI 設定快照。
//!
//! `UiSettings` 不再自行持久化：它只是 [`AppConfig`] 中「UI 可編輯欄位」的
//! 記憶體快照，透過 [`UiSettings::from_config`] / [`UiSettings::apply_to`] 互轉。
//! 唯一的設定檔是 config.toml。舊版 `settings.json` 只在啟動時匯入一次
//! （見 [`crate::settings::migration`]），因此保留 `Deserialize`。

use super::config::AppConfig;
use crate::output::OutputMode;
use crate::transcription::opencc::ConversionMode;
use serde::Deserialize;

/// UI 上可見、可編輯的設定。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
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
    pub privacy_mode: bool,

    // Model
    pub selected_model_id: String,

    // Transcription engine（[transcription]）
    /// `"local"` / `"openai"`。
    pub engine: String,
    pub openai_model: String,
    pub openai_consent: bool,

    // Window / tray（[general]）
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub start_minimized: bool,

    // Appearance（[general] theme）
    /// 深色模式；`general.theme == "dark"` 時為真。
    pub dark_mode: bool,
}

impl Default for UiSettings {
    /// 預設值一律取自 `AppConfig::default()`，確保 UI 與設定檔一致。
    fn default() -> Self {
        Self::from_config(&AppConfig::default())
    }
}

impl UiSettings {
    /// 由 AppConfig 建立 UI 快照（純函式）。
    pub fn from_config(config: &AppConfig) -> Self {
        let (clipboard_enabled, inject_enabled) = output_flags(&config.output.mode);
        Self {
            vad_threshold: config.audio.vad_threshold,
            silence_timeout_ms: config.audio.vad_silence_timeout_ms as f32,
            n_threads: config.whisper.n_threads,
            use_gpu: config.whisper.use_gpu,
            temperature: config.whisper.temperature,
            conversion_mode: conversion_mode_to_str(&config.chinese.conversion).into(),
            hotkey: config.general.global_hotkey.clone(),
            ptt_hotkey: config.general.ptt_hotkey.clone(),
            ptt_mode: config.general.ptt_mode,
            mod_vad: config.modules.vad,
            mod_context: config.modules.context_templates,
            mod_opencc: config.modules.opencc,
            mod_inject: config.modules.auto_inject,
            mod_waveform: config.modules.waveform,
            mod_history: config.modules.history,
            append_newline: config.output.append_newline,
            clipboard_enabled,
            inject_enabled,
            privacy_mode: config.output.privacy_mode,
            selected_model_id: config.model.selected_id.clone(),
            engine: config.transcription.engine.clone(),
            openai_model: config.transcription.openai_model.clone(),
            openai_consent: config.transcription.openai_consent,
            close_to_tray: config.general.close_to_tray,
            minimize_to_tray: config.general.minimize_to_tray,
            start_minimized: config.general.start_minimized,
            dark_mode: config.general.theme == "dark",
        }
    }

    /// 將 UI 快照寫回 AppConfig（純函式；不屬於 UI 的欄位保持不變）。
    pub fn apply_to(&self, config: &mut AppConfig) {
        config.audio.vad_threshold = self.vad_threshold;
        config.audio.vad_silence_timeout_ms = silence_ms_to_u64(self.silence_timeout_ms);
        config.whisper.n_threads = self.n_threads;
        config.whisper.use_gpu = self.use_gpu;
        config.whisper.temperature = self.temperature;
        config.chinese.conversion = conversion_mode_from_str(&self.conversion_mode);
        config.general.global_hotkey = self.hotkey.clone();
        config.general.ptt_hotkey = self.ptt_hotkey.clone();
        config.general.ptt_mode = self.ptt_mode;
        config.modules.vad = self.mod_vad;
        config.modules.context_templates = self.mod_context;
        config.modules.opencc = self.mod_opencc;
        config.modules.auto_inject = self.mod_inject;
        config.modules.waveform = self.mod_waveform;
        config.modules.history = self.mod_history;
        config.output.append_newline = self.append_newline;
        // 只有旗標實際改變時才改寫 mode，避免 FileAppend 等非 UI 模式被抹除
        if output_flags(&config.output.mode) != (self.clipboard_enabled, self.inject_enabled) {
            config.output.mode =
                output_mode_from_flags(self.clipboard_enabled, self.inject_enabled);
        }
        config.output.privacy_mode = self.privacy_mode;
        if !self.selected_model_id.is_empty() {
            config.model.selected_id = self.selected_model_id.clone();
        }
        config.transcription.engine = self.engine.clone();
        config.transcription.openai_model = self.openai_model.clone();
        config.transcription.openai_consent = self.openai_consent;
        config.general.close_to_tray = self.close_to_tray;
        config.general.minimize_to_tray = self.minimize_to_tray;
        config.general.start_minimized = self.start_minimized;
        // 深色 → "dark"；淺色只改寫原本的 "dark"，其餘（例如 "auto"）保留。
        if self.dark_mode {
            config.general.theme = "dark".into();
        } else if config.general.theme == "dark" {
            config.general.theme = "light".into();
        }
    }
}

fn silence_ms_to_u64(ms: f32) -> u64 {
    if ms.is_finite() && ms > 0.0 {
        ms.round() as u64
    } else {
        0
    }
}

/// OutputMode → (剪貼簿, 注入)
pub fn output_flags(mode: &OutputMode) -> (bool, bool) {
    match mode {
        OutputMode::ClipboardAndInject => (true, true),
        OutputMode::ClipboardOnly => (true, false),
        OutputMode::InjectToFocused => (false, true),
        OutputMode::FileAppend(_) | OutputMode::None => (false, false),
    }
}

/// (剪貼簿, 注入) → OutputMode
pub fn output_mode_from_flags(clipboard: bool, inject: bool) -> OutputMode {
    match (clipboard, inject) {
        (true, true) => OutputMode::ClipboardAndInject,
        (true, false) => OutputMode::ClipboardOnly,
        (false, true) => OutputMode::InjectToFocused,
        (false, false) => OutputMode::None,
    }
}

/// UI 字串（"zh-TW" / "zh-HK" / "zh-CN" / "raw"）→ ConversionMode；未知值回退 zh-TW。
pub fn conversion_mode_from_str(mode: &str) -> ConversionMode {
    match mode {
        "zh-HK" => ConversionMode::ZhHK,
        "zh-CN" => ConversionMode::ZhCN,
        "raw" => ConversionMode::Raw,
        _ => ConversionMode::ZhTW,
    }
}

/// ConversionMode → UI 字串
pub fn conversion_mode_to_str(mode: &ConversionMode) -> &'static str {
    match mode {
        ConversionMode::ZhTW => "zh-TW",
        ConversionMode::ZhHK => "zh-HK",
        ConversionMode::ZhCN => "zh-CN",
        ConversionMode::Raw => "raw",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn non_default_settings() -> UiSettings {
        UiSettings {
            vad_threshold: 0.35,
            silence_timeout_ms: 2200.0,
            n_threads: 8,
            use_gpu: false,
            temperature: 0.4,
            conversion_mode: "zh-HK".into(),
            hotkey: "Ctrl+Alt+R".into(),
            ptt_hotkey: "F9".into(),
            ptt_mode: false,
            mod_vad: false,
            mod_context: false,
            mod_opencc: false,
            mod_inject: false,
            mod_waveform: false,
            mod_history: false,
            append_newline: false,
            clipboard_enabled: true,
            inject_enabled: false,
            privacy_mode: true,
            selected_model_id: "large-v3-turbo".into(),
            engine: "openai".into(),
            openai_model: "custom-transcribe".into(),
            openai_consent: true,
            close_to_tray: false,
            minimize_to_tray: true,
            start_minimized: true,
            dark_mode: true,
        }
    }

    #[test]
    fn ui_to_config_to_ui_is_lossless() {
        let original = non_default_settings();
        let mut cfg = AppConfig::default();
        original.apply_to(&mut cfg);
        assert_eq!(UiSettings::from_config(&cfg), original);
    }

    #[test]
    fn round_trip_survives_toml_serialization() {
        let original = non_default_settings();
        let mut cfg = AppConfig::default();
        original.apply_to(&mut cfg);
        let text = toml::to_string_pretty(&cfg).unwrap();
        let reloaded: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(UiSettings::from_config(&reloaded), original);
    }

    #[test]
    fn config_to_ui_to_config_is_lossless() {
        let mut cfg = AppConfig::default();
        cfg.output.mode = OutputMode::InjectToFocused;
        cfg.chinese.conversion = ConversionMode::Raw;
        cfg.audio.vad_silence_timeout_ms = 900;
        cfg.history.max_entries = 7; // 非 UI 欄位
        let before = toml::to_string(&cfg).unwrap();
        let mut after = cfg.clone();
        UiSettings::from_config(&cfg).apply_to(&mut after);
        assert_eq!(toml::to_string(&after).unwrap(), before);
    }

    #[test]
    fn every_output_flag_combination_round_trips() {
        for clip in [false, true] {
            for inject in [false, true] {
                let s = UiSettings {
                    clipboard_enabled: clip,
                    inject_enabled: inject,
                    ..UiSettings::default()
                };
                let mut cfg = AppConfig::default();
                s.apply_to(&mut cfg);
                assert_eq!(UiSettings::from_config(&cfg), s);
            }
        }
    }

    #[test]
    fn file_append_mode_preserved_when_flags_unchanged() {
        let mut cfg = AppConfig::default();
        cfg.output.mode = OutputMode::FileAppend("out.txt".into());
        let snapshot = UiSettings::from_config(&cfg);
        snapshot.apply_to(&mut cfg);
        assert!(matches!(cfg.output.mode, OutputMode::FileAppend(_)));
    }

    #[test]
    fn every_conversion_mode_round_trips() {
        for m in [
            ConversionMode::ZhTW,
            ConversionMode::ZhHK,
            ConversionMode::ZhCN,
            ConversionMode::Raw,
        ] {
            assert_eq!(conversion_mode_from_str(conversion_mode_to_str(&m)), m);
        }
        assert_eq!(conversion_mode_from_str("bogus"), ConversionMode::ZhTW);
    }

    #[test]
    fn default_thresholds_consistent() {
        let ui = UiSettings::default();
        let cfg = AppConfig::default();
        assert_eq!(ui.vad_threshold, crate::audio::VAD_DEFAULT_THRESHOLD);
        assert_eq!(cfg.audio.vad_threshold, crate::audio::VAD_DEFAULT_THRESHOLD);
        assert_eq!(
            ui.silence_timeout_ms as u64,
            crate::audio::VAD_DEFAULT_SILENCE_TIMEOUT_MS
        );
        assert_eq!(
            cfg.audio.vad_silence_timeout_ms,
            crate::audio::VAD_DEFAULT_SILENCE_TIMEOUT_MS
        );
        assert_eq!(ui.conversion_mode, "zh-TW");
        assert!(!ui.privacy_mode);
    }

    /// Slint 端的初始值只在設定套用前短暫顯示，但仍須與 Rust 預設一致。
    #[test]
    fn slint_defaults_match_config_defaults() {
        let ui = UiSettings::default();
        for src in [
            include_str!("../../ui/appwindow.slint"),
            include_str!("../../ui/components/settings_panel.slint"),
        ] {
            let vad = format!("property <float> vad-threshold: {};", ui.vad_threshold);
            assert!(src.contains(&vad), "缺少 {vad}");
            let silence = format!(
                "property <float> silence-timeout: {};",
                ui.silence_timeout_ms
            );
            assert!(src.contains(&silence), "缺少 {silence}");
            assert!(src.contains("property <bool> privacy-mode: false;"));
        }
        let app = include_str!("../../ui/appwindow.slint");
        let model = format!(
            "property <string> selected-model-id: \"{}\";",
            ui.selected_model_id
        );
        assert!(app.contains(&model), "缺少 {model}");
    }

    #[test]
    fn legacy_json_without_new_fields_parses() {
        let json = r#"{"vad_threshold":0.5,"silence_timeout_ms":1500.0,"n_threads":6,
            "use_gpu":false,"temperature":0.0,"conversion_mode":"zh-CN",
            "hotkey":"Ctrl+Shift+W","ptt_hotkey":"Ctrl+Alt+L","ptt_mode":true,
            "mod_vad":true,"mod_context":true,"mod_opencc":true,"mod_inject":true,
            "mod_waveform":true,"mod_history":true,"append_newline":true,
            "clipboard_enabled":true,"inject_enabled":true,"selected_model_id":"medium"}"#;
        let s: UiSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.vad_threshold, 0.5);
        assert_eq!(s.conversion_mode, "zh-CN");
        assert!(!s.privacy_mode);
        assert_eq!(s.engine, "local");
        assert!(!s.openai_consent);
        assert!(s.close_to_tray);
    }

    #[test]
    fn dark_mode_round_trips_and_keeps_auto() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.general.theme, "auto");
        let light = UiSettings::from_config(&cfg);
        assert!(!light.dark_mode);
        light.apply_to(&mut cfg);
        assert_eq!(cfg.general.theme, "auto", "淺色時不改寫 auto");

        let dark = UiSettings {
            dark_mode: true,
            ..light.clone()
        };
        dark.apply_to(&mut cfg);
        assert_eq!(cfg.general.theme, "dark");
        let text = toml::to_string_pretty(&cfg).unwrap();
        let reloaded: AppConfig = toml::from_str(&text).unwrap();
        assert!(UiSettings::from_config(&reloaded).dark_mode);

        light.apply_to(&mut cfg);
        assert_eq!(cfg.general.theme, "light");
        assert!(!UiSettings::from_config(&cfg).dark_mode);
    }

    #[test]
    fn new_field_defaults_match_config() {
        let ui = UiSettings::default();
        assert_eq!(ui.engine, "local");
        assert_eq!(ui.openai_model, "gpt-live-transcribe");
        assert!(!ui.openai_consent);
        assert!(ui.close_to_tray);
        assert!(!ui.minimize_to_tray);
        assert!(!ui.start_minimized);
    }
}
