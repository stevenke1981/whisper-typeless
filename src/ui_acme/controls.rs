//! 控制項的純邏輯：滑桿 / 步進器數學、文字輸入框（acme-textinput）的鍵盤與輸入法處理。

use acme_platform::{Clipboard, PlatformKey};
use acme_text::{FontSystem, TextStyle};
use acme_textinput::{
    byte_offset_to_x, handle_key, handle_keyboard_shortcut, handle_text, TextInputState,
};

use super::msg::FieldId;

// ─── 滑桿 / 步進器 ───────────────────────────────────────────────────────────

/// 數值在 `min..=max` 中的比例（0.0–1.0）。
pub fn slider_fraction(value: f32, min: f32, max: f32) -> f32 {
    if max.partial_cmp(&min) != Some(std::cmp::Ordering::Greater) || !value.is_finite() {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

/// 依步長對齊並夾在範圍內。
pub fn snap(value: f32, min: f32, max: f32, step: f32) -> f32 {
    let v = f64::from(if value.is_finite() { value } else { min });
    let (lo, st) = (f64::from(min), f64::from(step));
    let v = if st > 0.0 {
        lo + ((v - lo) / st).round() * st
    } else {
        v
    };
    // 以 f64 計算並取到小數四位，消除 0.1 + 0.05×8 之類的浮點誤差。
    let v = ((v * 10_000.0).round() / 10_000.0) as f32;
    v.clamp(min.min(max), max.max(min))
}

/// 指標 x（視窗座標）→ 滑桿數值。
pub fn slider_value(x: f32, track_x: f32, track_w: f32, min: f32, max: f32, step: f32) -> f32 {
    let t = if track_w > 0.0 {
        ((x - track_x) / track_w).clamp(0.0, 1.0)
    } else {
        0.0
    };
    snap(min + t * (max - min), min, max, step)
}

/// 步進器：往上（`+1`）或往下（`-1`）走 `steps` 格。
pub fn step_value(value: f32, steps: i32, step: f32, min: f32, max: f32) -> f32 {
    snap(value + steps as f32 * step, min, max, step)
}

// ─── 文字輸入框 ──────────────────────────────────────────────────────────────

/// 所有文字輸入框的狀態（acme-textinput）。
#[derive(Clone, Debug, Default)]
pub struct Fields {
    pub hotkey: TextInputState,
    pub ptt_hotkey: TextInputState,
    pub template_name: TextInputState,
    pub template_prompt: TextInputState,
    pub openai_key: TextInputState,
}

impl Fields {
    pub fn new() -> Self {
        let mut f = Self::default();
        f.hotkey.placeholder = "Ctrl+Shift+W".into();
        f.ptt_hotkey.placeholder = "Ctrl+Alt+L".into();
        f.template_name.placeholder = "模板名稱".into();
        f.template_prompt.placeholder = "提示詞（給 whisper 的情境提示）".into();
        f.openai_key.placeholder = "貼上 sk-… 金鑰".into();
        f.openai_key.password = true;
        f
    }

    pub fn get(&self, id: FieldId) -> &TextInputState {
        match id {
            FieldId::Hotkey => &self.hotkey,
            FieldId::PttHotkey => &self.ptt_hotkey,
            FieldId::TemplateName => &self.template_name,
            FieldId::TemplatePrompt => &self.template_prompt,
            FieldId::OpenAiKey => &self.openai_key,
        }
    }

    pub fn get_mut(&mut self, id: FieldId) -> &mut TextInputState {
        match id {
            FieldId::Hotkey => &mut self.hotkey,
            FieldId::PttHotkey => &mut self.ptt_hotkey,
            FieldId::TemplateName => &mut self.template_name,
            FieldId::TemplatePrompt => &mut self.template_prompt,
            FieldId::OpenAiKey => &mut self.openai_key,
        }
    }
}

/// 整個換掉文字（外部同步用）：游標移到結尾、清除選取 / 組字 / 復原紀錄。
pub fn set_text(state: &mut TextInputState, text: &str) {
    state.text = text.to_string();
    state.cancel_preedit();
    state.move_cursor_to_offset(state.text.len());
    state.clear_history();
    state.scroll_offset = 0.0;
}

/// 按鍵交給文字框後的結果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKey {
    /// 文字框不處理（交給視窗快捷鍵）。
    Ignored,
    /// 已處理；`edited` 表示文字內容有變。
    Handled { edited: bool },
    /// Enter：送出（失去焦點並套用）。
    Submit,
    /// Esc / Tab：失去焦點。
    Blur,
}

/// 文字框持有焦點時的按鍵處理（acme-textinput `handle_key` / `handle_text` / 剪貼簿快捷鍵）。
#[allow(clippy::too_many_arguments)]
pub fn field_key(
    state: &mut TextInputState,
    key: PlatformKey,
    ctrl: bool,
    alt: bool,
    shift: bool,
    character: Option<&str>,
    text: Option<&str>,
    clipboard: Option<&Clipboard>,
) -> FieldKey {
    let before = state.text.clone();
    let edited = |state: &TextInputState| FieldKey::Handled {
        edited: state.text != before,
    };
    match key {
        PlatformKey::Enter => return FieldKey::Submit,
        PlatformKey::Tab => return FieldKey::Blur,
        PlatformKey::Escape => {
            if !state.preedit.is_empty() {
                state.cancel_preedit();
                return FieldKey::Handled { edited: false };
            }
            return FieldKey::Blur;
        }
        PlatformKey::ArrowLeft
        | PlatformKey::ArrowRight
        | PlatformKey::Home
        | PlatformKey::End
        | PlatformKey::Backspace
        | PlatformKey::Delete => {
            handle_key(state, &key, true, clipboard, ctrl, shift);
            return edited(state);
        }
        _ => {}
    }
    if ctrl {
        let letter = character.or(text).map(|c| c.to_lowercase());
        let shortcut = match letter.as_deref() {
            Some("z") if shift => Some("y"),
            Some(l @ ("a" | "c" | "v" | "x" | "z" | "y")) => Some(l),
            _ => None,
        };
        if let Some(shortcut) = shortcut {
            // 遮蔽欄位（金鑰）不允許複製 / 剪下到剪貼簿
            if state.password && matches!(shortcut, "c" | "x") {
                return FieldKey::Handled { edited: false };
            }
            handle_keyboard_shortcut(state, shortcut, clipboard);
            return edited(state);
        }
        return FieldKey::Ignored;
    }
    if alt {
        return FieldKey::Ignored;
    }
    match text {
        Some(t) if !t.is_empty() && !t.chars().any(char::is_control) => {
            handle_text(state, t);
            edited(state)
        }
        // 文字框持有焦點時，其他按鍵（方向上下等）也不交給視窗快捷鍵。
        _ => FieldKey::Handled { edited: false },
    }
}

/// 輸入法組字中（空字串 = 組字結束）。
pub fn ime_preedit(state: &mut TextInputState, text: &str, cursor: Option<(usize, usize)>) {
    if text.is_empty() {
        state.cancel_preedit();
    } else {
        state.set_preedit(text, cursor);
    }
}

/// 輸入法確定文字；回傳文字是否有變。
pub fn ime_commit(state: &mut TextInputState, text: &str) -> bool {
    state.cancel_preedit();
    handle_text(state, text)
}

/// 指標 x（相對於文字起點）→ 最接近的字素邊界位元組位移。
pub fn offset_at_x(
    text: &str,
    x: f32,
    fonts: &mut FontSystem,
    style: &TextStyle,
    scale: f32,
) -> usize {
    use unicode_boundaries::grapheme_starts;
    if text.is_empty() || x <= 0.0 {
        return 0;
    }
    let mut best = (0usize, f32::MAX);
    for offset in grapheme_starts(text) {
        let px = byte_offset_to_x(text, offset, fonts, style, scale);
        let d = (px - x).abs();
        if d < best.1 {
            best = (offset, d);
        }
        if px > x {
            break;
        }
    }
    best.0
}

mod unicode_boundaries {
    /// 字元邊界（含結尾）。以字元近似字素邊界：zh-TW 與熱鍵字串足夠。
    pub fn grapheme_starts(text: &str) -> impl Iterator<Item = usize> + '_ {
        text.char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_math_maps_and_snaps() {
        assert!((slider_fraction(0.5, 0.1, 0.9) - 0.5).abs() < 1e-6);
        assert_eq!(slider_fraction(5.0, 0.1, 0.9), 1.0);
        assert_eq!(slider_fraction(0.5, 1.0, 1.0), 0.0);
        // 軌道 100..200，範圍 0.1..0.9、步長 0.05
        assert_eq!(slider_value(100.0, 100.0, 100.0, 0.1, 0.9, 0.05), 0.1);
        assert_eq!(slider_value(200.0, 100.0, 100.0, 0.1, 0.9, 0.05), 0.9);
        assert_eq!(slider_value(150.0, 100.0, 100.0, 0.1, 0.9, 0.05), 0.5);
        assert_eq!(slider_value(152.0, 100.0, 100.0, 0.1, 0.9, 0.05), 0.5);
        assert_eq!(slider_value(10.0, 100.0, 100.0, 0.1, 0.9, 0.05), 0.1);
        assert_eq!(slider_value(999.0, 100.0, 100.0, 0.0, 1.0, 0.05), 1.0);
        assert_eq!(slider_value(130.0, 100.0, 100.0, 0.0, 1.0, 0.0), 0.3);
        assert_eq!(slider_value(130.0, 100.0, 0.0, 0.0, 1.0, 0.1), 0.0);
    }

    #[test]
    fn stepper_math_clamps_and_avoids_drift() {
        assert_eq!(step_value(1500.0, 1, 100.0, 300.0, 5000.0), 1600.0);
        assert_eq!(step_value(300.0, -1, 100.0, 300.0, 5000.0), 300.0);
        assert_eq!(step_value(5000.0, 3, 100.0, 300.0, 5000.0), 5000.0);
        assert_eq!(step_value(4.0, -1, 1.0, 1.0, 32.0), 3.0);
        let mut v = 0.1;
        for _ in 0..3 {
            v = step_value(v, 1, 0.1, 0.0, 1.0);
        }
        assert_eq!(v, 0.4);
        assert_eq!(snap(f32::NAN, 0.0, 1.0, 0.1), 0.0);
    }

    fn focused(text: &str) -> TextInputState {
        let mut s = TextInputState::new();
        set_text(&mut s, text);
        s.focused = true;
        s
    }

    fn press(s: &mut TextInputState, key: PlatformKey, text: Option<&str>) -> FieldKey {
        field_key(s, key, false, false, false, text, text, None)
    }

    #[test]
    fn typing_editing_and_submit() {
        let mut s = focused("Ctrl+Alt+");
        assert_eq!(
            press(&mut s, PlatformKey::Other, Some("L")),
            FieldKey::Handled { edited: true }
        );
        assert_eq!(s.text, "Ctrl+Alt+L");
        // Space 是文字，不是錄音快捷鍵
        assert_eq!(
            press(&mut s, PlatformKey::Space, Some(" ")),
            FieldKey::Handled { edited: true }
        );
        assert_eq!(
            press(&mut s, PlatformKey::Backspace, None),
            FieldKey::Handled { edited: true }
        );
        assert_eq!(s.text, "Ctrl+Alt+L");
        assert_eq!(
            press(&mut s, PlatformKey::Home, None),
            FieldKey::Handled { edited: false }
        );
        assert_eq!(s.cursor(), 0);
        assert_eq!(press(&mut s, PlatformKey::Enter, None), FieldKey::Submit);
        assert_eq!(press(&mut s, PlatformKey::Tab, None), FieldKey::Blur);
        // 控制字元不插入
        assert_eq!(
            press(&mut s, PlatformKey::Other, Some("\u{8}")),
            FieldKey::Handled { edited: false }
        );
        // Ctrl+A 全選後輸入取代
        assert_eq!(
            field_key(
                &mut s,
                PlatformKey::Other,
                true,
                false,
                false,
                Some("a"),
                None,
                None
            ),
            FieldKey::Handled { edited: false }
        );
        press(&mut s, PlatformKey::Other, Some("F9"));
        assert_eq!(s.text, "F9");
        // Ctrl+Z 復原
        field_key(
            &mut s,
            PlatformKey::Other,
            true,
            false,
            false,
            Some("z"),
            None,
            None,
        );
        assert_eq!(s.text, "Ctrl+Alt+L");
        // Ctrl+其他、Alt+字母 交還給視窗
        assert_eq!(
            field_key(
                &mut s,
                PlatformKey::Other,
                true,
                false,
                false,
                Some("q"),
                None,
                None
            ),
            FieldKey::Ignored
        );
        assert_eq!(
            field_key(
                &mut s,
                PlatformKey::Other,
                false,
                true,
                false,
                Some("e"),
                Some("e"),
                None
            ),
            FieldKey::Ignored
        );
    }

    #[test]
    fn ime_preedit_commit_and_cancel() {
        let mut s = focused("會議");
        ime_preedit(&mut s, "ㄐㄧˋ", Some((0, 3)));
        assert_eq!(s.preedit, "ㄐㄧˋ");
        assert_eq!(s.text, "會議", "preedit must not touch committed text");
        assert!(ime_commit(&mut s, "記錄"));
        assert_eq!(s.text, "會議記錄");
        assert!(s.preedit.is_empty());
        assert_eq!(s.cursor(), "會議記錄".len());

        // 組字中按 Esc：只取消組字，不失去焦點
        ime_preedit(&mut s, "ㄅ", None);
        assert_eq!(
            press(&mut s, PlatformKey::Escape, None),
            FieldKey::Handled { edited: false }
        );
        assert!(s.preedit.is_empty());
        assert_eq!(s.text, "會議記錄");
        assert_eq!(press(&mut s, PlatformKey::Escape, None), FieldKey::Blur);

        // 空的 preedit = 組字結束
        ime_preedit(&mut s, "ㄅ", None);
        ime_preedit(&mut s, "", None);
        assert!(s.preedit.is_empty());
        assert!(!ime_commit(&mut s, ""));
    }

    #[test]
    fn set_text_resets_cursor_and_history() {
        let mut s = focused("abc");
        handle_text(&mut s, "d");
        set_text(&mut s, "新模板");
        assert_eq!(s.cursor(), "新模板".len());
        assert!(!s.undo());
    }

    #[test]
    fn offset_at_x_snaps_to_char_boundaries() {
        let mut fonts = FontSystem::new();
        let style = super::super::text::TextEngine::style(13.0, false);
        assert_eq!(offset_at_x("中文", -5.0, &mut fonts, &style, 1.0), 0);
        assert_eq!(offset_at_x("", 50.0, &mut fonts, &style, 1.0), 0);
        let far = offset_at_x("中文", 500.0, &mut fonts, &style, 1.0);
        assert!("中文".is_char_boundary(far));
        if fonts.diagnostics().system_faces > 0 {
            assert_eq!(far, "中文".len());
        }
    }

    #[test]
    fn fields_lookup_by_id() {
        let mut f = Fields::new();
        set_text(f.get_mut(FieldId::PttHotkey), "Ctrl+Alt+L");
        assert_eq!(f.get(FieldId::PttHotkey).text, "Ctrl+Alt+L");
        assert!(f.get(FieldId::Hotkey).text.is_empty());
        assert!(!f.get(FieldId::TemplateName).placeholder.is_empty());
    }

    #[test]
    fn password_field_blocks_copy_and_cut() {
        let f = Fields::new();
        assert!(f.get(FieldId::OpenAiKey).password);
        assert!(!f.get(FieldId::Hotkey).password);
        let mut s = focused("sk-secret");
        s.password = true;
        let ctrl = |s: &mut TextInputState, c: &str| {
            field_key(
                s,
                PlatformKey::Other,
                true,
                false,
                false,
                Some(c),
                None,
                None,
            )
        };
        ctrl(&mut s, "a");
        assert_eq!(ctrl(&mut s, "x"), FieldKey::Handled { edited: false });
        assert_eq!(ctrl(&mut s, "c"), FieldKey::Handled { edited: false });
        assert_eq!(s.text, "sk-secret");
    }
}
