//! 鍵盤快捷鍵 → [`UiCommand`]（純函式，與 Slint 主視窗（ui/appwindow）的 FocusScope 規則一致）。
//!
//! | 按鍵 | 條件 | 指令 |
//! |------|------|------|
//! | Esc | 設定面板開啟 | `SetSettingsOpen(false)` |
//! | Esc | 錯誤紀錄開啟 | `SetShowErrorLog(false)` |
//! | Esc | 浮動且展開 | `SetFloatingExpanded(false)` |
//! | Space | 無修飾鍵、文字框無焦點、設定面板關閉 | `ToggleRecording` |
//! | E / e | 無修飾鍵、文字框無焦點、浮動模式 | `SetFloatingExpanded(!expanded)` |
//!
//! 這些只是「視窗內」快捷鍵；全域熱鍵與 PTT 由背景 hook 處理，永遠不會搶焦點。

use super::command::UiCommand;
use super::state::AppViewState;

/// 與 UI 框架無關的按鍵。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Escape,
    Space,
    Char(char),
    /// 其他不處理的按鍵。
    Other,
}

/// 修飾鍵狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
    pub shift: bool,
}

/// 一次按鍵。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyPress {
    /// 無修飾鍵的按鍵。
    pub fn plain(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::default(),
        }
    }
}

/// 依目前狀態把按鍵轉成指令；`None` 表示畫面應照常處理此按鍵（例如輸入文字）。
///
/// `text_focused`：文字輸入框持有焦點時為 true，此時 Space / E 必須留給輸入框；
/// Esc 仍可關閉浮層。
pub fn key_to_command(
    state: &AppViewState,
    press: &KeyPress,
    text_focused: bool,
) -> Option<UiCommand> {
    if press.key == Key::Escape {
        if state.settings_open {
            return Some(UiCommand::SetSettingsOpen(false));
        }
        if state.show_error_log {
            return Some(UiCommand::SetShowErrorLog(false));
        }
        if state.floating_mode && state.floating_expanded {
            return Some(UiCommand::SetFloatingExpanded(false));
        }
        return None;
    }

    let m = press.modifiers;
    if text_focused || m.control || m.alt || m.meta {
        return None;
    }
    match press.key {
        Key::Space | Key::Char(' ') if !state.settings_open => Some(UiCommand::ToggleRecording),
        Key::Char('e') | Key::Char('E') if state.floating_mode => {
            Some(UiCommand::SetFloatingExpanded(!state.floating_expanded))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key) -> KeyPress {
        KeyPress::plain(key)
    }

    #[test]
    fn escape_closes_overlays_in_priority_order() {
        let mut s = AppViewState {
            settings_open: true,
            show_error_log: true,
            floating_mode: true,
            floating_expanded: true,
            ..AppViewState::default()
        };
        let esc = press(Key::Escape);
        assert_eq!(
            key_to_command(&s, &esc, false),
            Some(UiCommand::SetSettingsOpen(false))
        );
        s.settings_open = false;
        assert_eq!(
            key_to_command(&s, &esc, false),
            Some(UiCommand::SetShowErrorLog(false))
        );
        s.show_error_log = false;
        assert_eq!(
            key_to_command(&s, &esc, true),
            Some(UiCommand::SetFloatingExpanded(false))
        );
        s.floating_expanded = false;
        assert_eq!(key_to_command(&s, &esc, false), None);
    }

    #[test]
    fn space_toggles_recording_unless_settings_or_text_focus() {
        let mut s = AppViewState::default();
        assert_eq!(
            key_to_command(&s, &press(Key::Space), false),
            Some(UiCommand::ToggleRecording)
        );
        assert_eq!(
            key_to_command(&s, &press(Key::Char(' ')), false),
            Some(UiCommand::ToggleRecording)
        );
        assert_eq!(key_to_command(&s, &press(Key::Space), true), None);
        let ctrl_space = KeyPress {
            key: Key::Space,
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        };
        assert_eq!(key_to_command(&s, &ctrl_space, false), None);
        s.settings_open = true;
        assert_eq!(key_to_command(&s, &press(Key::Space), false), None);
    }

    #[test]
    fn e_toggles_expansion_only_in_floating_mode() {
        let mut s = AppViewState::default();
        assert_eq!(key_to_command(&s, &press(Key::Char('e')), false), None);
        s.floating_mode = true;
        assert_eq!(
            key_to_command(&s, &press(Key::Char('E')), false),
            Some(UiCommand::SetFloatingExpanded(true))
        );
        s.floating_expanded = true;
        assert_eq!(
            key_to_command(&s, &press(Key::Char('e')), false),
            Some(UiCommand::SetFloatingExpanded(false))
        );
        assert_eq!(key_to_command(&s, &press(Key::Char('e')), true), None);
        let alt_e = KeyPress {
            key: Key::Char('e'),
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        };
        assert_eq!(key_to_command(&s, &alt_e, false), None);
    }

    #[test]
    fn other_keys_pass_through() {
        let s = AppViewState::default();
        assert_eq!(key_to_command(&s, &press(Key::Char('x')), false), None);
        assert_eq!(key_to_command(&s, &press(Key::Other), false), None);
    }
}
