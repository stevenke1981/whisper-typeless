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
        }
    }

    /// 修改後是否要重新套用熱鍵（`UiCommand::SettingsChanged`）。
    pub fn needs_settings_changed(&self) -> bool {
        !matches!(
            self,
            SettingEdit::VadThreshold(_) | SettingEdit::Temperature(_)
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
    fn scroll_ids_index_uniquely() {
        let mut seen = [false; 4];
        for id in ScrollId::ALL {
            assert!(!seen[id.index()]);
            seen[id.index()] = true;
        }
    }
}
