//! 畫面訊息：命中區域被點擊後送出的 [`Msg`]，以及設定、文字框、捲動區的識別碼。

use crate::core::UiCommand;
use crate::settings::ui_settings::UiSettings;

/// 文字輸入框。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldId {
    Hotkey,
    PttHotkey,
    TemplateName,
    TemplatePrompt,
    /// OpenAI API 金鑰（遮蔽顯示；儲存後清空，不同步自狀態）。
    OpenAiKey,
}

/// 可捲動區域。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScrollId {
    Transcript,
    Settings,
    FileResult,
    ErrorLog,
}

impl ScrollId {
    pub const ALL: [ScrollId; 4] = [
        ScrollId::Transcript,
        ScrollId::Settings,
        ScrollId::FileResult,
        ScrollId::ErrorLog,
    ];

    pub fn index(self) -> usize {
        match self {
            ScrollId::Transcript => 0,
            ScrollId::Settings => 1,
            ScrollId::FileResult => 2,
            ScrollId::ErrorLog => 3,
        }
    }
}

/// 設定的分頁（由左到右）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsTab {
    #[default]
    General,
    Engine,
    Models,
    Output,
    Templates,
    Advanced,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 6] = [
        SettingsTab::General,
        SettingsTab::Engine,
        SettingsTab::Models,
        SettingsTab::Output,
        SettingsTab::Templates,
        SettingsTab::Advanced,
    ];

    /// 分頁標籤（zh-TW）。
    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::General => "一般",
            SettingsTab::Engine => "辨識",
            SettingsTab::Models => "模型",
            SettingsTab::Output => "輸出",
            SettingsTab::Templates => "模板",
            SettingsTab::Advanced => "進階",
        }
    }

    /// 在 [`SettingsTab::ALL`] 中的位置（0 起算；`Ctrl+1..6` 用）。
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    /// 穩定的字串 id（分段控制項用）。
    pub fn id(self) -> &'static str {
        match self {
            SettingsTab::General => "general",
            SettingsTab::Engine => "engine",
            SettingsTab::Models => "models",
            SettingsTab::Output => "output",
            SettingsTab::Templates => "templates",
            SettingsTab::Advanced => "advanced",
        }
    }

    pub fn from_id(id: &str) -> Option<SettingsTab> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }
}

/// 滑桿控制的設定值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderKind {
    VadThreshold,
    Temperature,
}

/// 功能模組開關。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModKey {
    Vad,
    Context,
    OpenCc,
    Inject,
    Waveform,
    History,
}

/// 對設定快照的一次修改（透過 `UiCommand::UpdateSettings` 交給核心）。
#[derive(Clone, Debug, PartialEq)]
pub enum SettingEdit {
    VadThreshold(f32),
    Temperature(f32),
    SilenceTimeout(f32),
    Threads(i32),
    UseGpu(bool),
    PttMode(bool),
    ConversionMode(String),
    Module(ModKey, bool),
    Hotkey(String),
    PttHotkey(String),
    /// 轉錄引擎 `"local"` / `"openai"`。
    Engine(String),
    /// 同意把語音傳送到 OpenAI。
    OpenAiConsent(bool),
    OpenAiModel(String),
    /// 結尾自動換行（`append_newline`）。
    AppendNewline(bool),
    /// 關閉視窗時縮到系統匣。
    CloseToTray(bool),
    /// 最小化時縮到系統匣。
    MinimizeToTray(bool),
    /// 開機（啟動）後直接縮到系統匣。
    StartMinimized(bool),
}

impl SettingEdit {
    /// 套用到設定快照。
    pub fn apply(&self, s: &mut UiSettings) {
        match self {
            SettingEdit::VadThreshold(v) => s.vad_threshold = *v,
            SettingEdit::Temperature(v) => s.temperature = *v,
            SettingEdit::SilenceTimeout(v) => s.silence_timeout_ms = *v,
            SettingEdit::Threads(v) => s.n_threads = *v,
            SettingEdit::UseGpu(v) => s.use_gpu = *v,
            SettingEdit::PttMode(v) => s.ptt_mode = *v,
            SettingEdit::ConversionMode(v) => s.conversion_mode = v.clone(),
            SettingEdit::Module(key, on) => {
                let slot = match key {
                    ModKey::Vad => &mut s.mod_vad,
                    ModKey::Context => &mut s.mod_context,
                    ModKey::OpenCc => &mut s.mod_opencc,
                    ModKey::Inject => &mut s.mod_inject,
                    ModKey::Waveform => &mut s.mod_waveform,
                    ModKey::History => &mut s.mod_history,
                };
                *slot = *on;
            }
            SettingEdit::Hotkey(v) => s.hotkey = v.clone(),
            SettingEdit::PttHotkey(v) => s.ptt_hotkey = v.clone(),
            SettingEdit::Engine(v) => s.engine = v.clone(),
            SettingEdit::OpenAiConsent(v) => s.openai_consent = *v,
            SettingEdit::OpenAiModel(v) => s.openai_model = v.clone(),
            SettingEdit::AppendNewline(v) => s.append_newline = *v,
            SettingEdit::CloseToTray(v) => s.close_to_tray = *v,
            SettingEdit::MinimizeToTray(v) => s.minimize_to_tray = *v,
            SettingEdit::StartMinimized(v) => s.start_minimized = *v,
        }
    }

    /// 修改後是否要重新套用熱鍵（`UiCommand::SettingsChanged`）。
    pub fn needs_settings_changed(&self) -> bool {
        !matches!(
            self,
            SettingEdit::VadThreshold(_)
                | SettingEdit::Temperature(_)
                | SettingEdit::Engine(_)
                | SettingEdit::OpenAiConsent(_)
                | SettingEdit::OpenAiModel(_)
                | SettingEdit::AppendNewline(_)
                | SettingEdit::CloseToTray(_)
                | SettingEdit::MinimizeToTray(_)
                | SettingEdit::StartMinimized(_)
        )
    }
}

/// 使用者意圖。多數直接對應 [`UiCommand`]；其餘由畫面層自行處理。
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// 直接交給核心的指令。
    Core(UiCommand),
    /// 浮動 ↔ 完整視窗。
    ToggleFloating,
    /// 浮動視窗展開 ↔ 收合。
    ToggleExpanded,
    /// 開關設定面板（浮動收合時自動展開）。
    ToggleSettings,
    /// 開啟設定並切到指定分頁（浮動收合時自動展開）。
    OpenSettings(SettingsTab),
    /// 設定已開啟時切換分頁（捲動歸零）。
    SettingsTab(SettingsTab),
    /// 開關錯誤紀錄（浮動收合時先展開，否則清單會被 52px 高度裁掉）。
    ToggleErrorLog,
    /// 浮動視窗置頂 ↔ 取消置頂。
    TogglePin,
    /// 最小化視窗。
    Minimize,
    /// 錄音鍵按下 / 放開（PTT 按住說話）。
    RecordPress,
    RecordRelease,
    /// 修改設定。
    Setting(SettingEdit),
    /// 模板編輯：選取第 n 個 / 新增 / 儲存 / 刪除。
    TemplateEditSelect(usize),
    TemplateEditNew,
    TemplateEditSave,
    TemplateEditDelete,
    /// 把金鑰欄位的內容存入金鑰存放區並清空欄位。
    OpenAiKeySave,
}

impl From<UiCommand> for Msg {
    fn from(cmd: UiCommand) -> Self {
        Msg::Core(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_edits_apply_to_snapshot() {
        let mut s = UiSettings::default();
        SettingEdit::VadThreshold(0.3).apply(&mut s);
        SettingEdit::Module(ModKey::Waveform, false).apply(&mut s);
        SettingEdit::Hotkey("Ctrl+Alt+H".into()).apply(&mut s);
        SettingEdit::ConversionMode("zh-HK".into()).apply(&mut s);
        SettingEdit::Threads(8).apply(&mut s);
        assert_eq!(s.vad_threshold, 0.3);
        assert!(!s.mod_waveform);
        assert_eq!(s.hotkey, "Ctrl+Alt+H");
        assert_eq!(s.conversion_mode, "zh-HK");
        assert_eq!(s.n_threads, 8);
        assert!(!SettingEdit::Temperature(0.1).needs_settings_changed());
        assert!(SettingEdit::PttMode(false).needs_settings_changed());
    }

    #[test]
    fn cloud_setting_edits_apply() {
        let mut s = UiSettings::default();
        SettingEdit::Engine("openai".into()).apply(&mut s);
        SettingEdit::OpenAiConsent(true).apply(&mut s);
        SettingEdit::OpenAiModel("gpt-transcribe".into()).apply(&mut s);
        assert_eq!(s.engine, "openai");
        assert!(s.openai_consent);
        assert_eq!(s.openai_model, "gpt-transcribe");
        assert!(!SettingEdit::OpenAiConsent(true).needs_settings_changed());
    }

    #[test]
    fn window_and_output_edits_apply() {
        let mut s = UiSettings::default();
        SettingEdit::AppendNewline(false).apply(&mut s);
        SettingEdit::CloseToTray(false).apply(&mut s);
        SettingEdit::MinimizeToTray(true).apply(&mut s);
        SettingEdit::StartMinimized(true).apply(&mut s);
        assert!(!s.append_newline && !s.close_to_tray);
        assert!(s.minimize_to_tray && s.start_minimized);
        assert!(!SettingEdit::CloseToTray(true).needs_settings_changed());
    }

    #[test]
    fn settings_tabs_are_ordered_and_labelled() {
        assert_eq!(SettingsTab::default(), SettingsTab::General);
        for (i, tab) in SettingsTab::ALL.into_iter().enumerate() {
            assert_eq!(tab.index(), i);
            assert_eq!(SettingsTab::from_id(tab.id()), Some(tab));
            assert_eq!(tab.label().chars().count(), 2);
        }
        assert_eq!(SettingsTab::from_id("nope"), None);
    }

    #[test]
    fn scroll_ids_index_uniquely() {
        let mut seen = [false; 4];
        for id in ScrollId::ALL {
            assert!(!seen[id.index()]);
            seen[id.index()] = true;
        }
    }
}
