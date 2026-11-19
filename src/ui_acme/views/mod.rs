//! 檢視：`AppViewState`（＋畫面本地狀態）→ [`View`] 描述樹的純函式。
//!
//! - 浮動模式：緊湊列 380×52，展開為 380×540（快速操作、模板列、波形、轉錄 / 檔案模式）。
//! - 完整模式：標題列、即時 / 檔案分頁、模板列、波形、轉錄、控制列、狀態列。
//! - 浮層：錯誤紀錄、設定面板（兩種模式共用）。

pub mod file;
pub mod live;
pub mod onboarding;
pub mod overlay;
pub mod settings;
pub mod widgets;

use super::local::UiLocal;
use super::paint::{r, View};
use super::text::TextEngine;
use super::theme::Theme;
use crate::core::AppViewState;

/// 視窗尺寸（邏輯像素），與 Slint 版一致。
pub const FULL_W: f32 = 560.0;
pub const FULL_H: f32 = 620.0;
pub const BAR_W: f32 = 380.0;
pub const BAR_H: f32 = 52.0;
pub const EXPANDED_H: f32 = 540.0;

/// 情境模板列（與 Slint `AppWindow.templates` 相同）。
pub const TEMPLATES: [(&str, &str, &str); 6] = [
    ("auto", "🔮", "自動"),
    ("meeting", "🏢", "會議"),
    ("casual", "💬", "口語"),
    ("technical", "💻", "技術"),
    ("medical", "🏥", "醫療"),
    ("legal", "⚖", "法律"),
];

/// 模板 id → 顯示名稱。
pub fn template_label(id: &str) -> &str {
    TEMPLATES
        .iter()
        .find(|(tid, _, _)| *tid == id)
        .map(|(_, _, name)| *name)
        .unwrap_or(id)
}

/// 目前狀態下的視窗邏輯尺寸。
pub fn window_size(s: &AppViewState) -> (f32, f32) {
    if !s.floating_mode {
        (FULL_W, FULL_H)
    } else if s.floating_expanded {
        (BAR_W, EXPANDED_H)
    } else {
        (BAR_W, BAR_H)
    }
}

/// 主畫面狀態卡要呈現的階段（`docs/ui-redesign.md` 2.5）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// 選用的本機模型尚未下載（雲端也不可用）。
    NeedsModel,
    /// 正在下載模型。
    Downloading,
    Recording,
    Processing,
    Ready,
}

/// 目前的階段。錄音 / 辨識優先（下載其他模型時仍看得到錄音狀態），其次是缺模型、下載中。
pub fn phase(s: &AppViewState) -> Phase {
    if s.is_recording {
        Phase::Recording
    } else if s.is_processing {
        Phase::Processing
    } else if needs_model(s) {
        Phase::NeedsModel
    } else if !s.downloading_id.is_empty() {
        Phase::Downloading
    } else {
        Phase::Ready
    }
}

/// 雲端轉錄實際會被使用：選了 OpenAI、已同意、有金鑰，且不在隱私模式。
pub fn cloud_ready(s: &AppViewState) -> bool {
    s.engine == "openai" && s.openai_consent && s.openai_key_set && !s.privacy_mode
}

/// 需要先下載模型：不走雲端、選用的模型在清單中但未下載，且目前沒有下載中的模型。
pub fn needs_model(s: &AppViewState) -> bool {
    !cloud_ready(s)
        && s.downloading_id.is_empty()
        && s.model_entries
            .iter()
            .any(|m| m.id == s.selected_model_id && !m.is_downloaded)
}

/// 建構檢視時需要的一切。
pub struct Ctx<'a> {
    pub s: &'a AppViewState,
    pub ui: &'a UiLocal,
    pub t: &'static Theme,
    pub text: &'a mut TextEngine,
    pub scale: f32,
}

/// 根檢視。
pub fn build(cx: &mut Ctx<'_>, width: f32, height: f32) -> View {
    let mut v = View::new();
    if cx.s.floating_mode {
        live::floating(&mut v, cx, width, height);
    } else {
        live::full(&mut v, cx, width, height);
    }
    if cx.s.show_error_log {
        v.layer(4);
        overlay::error_log(&mut v, cx, width, height);
    }
    if cx.s.settings_open {
        v.layer(6);
        settings::panel(&mut v, cx, r(0.0, 0.0, width, height));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ModelEntryView;

    fn with_models(selected: &str) -> AppViewState {
        AppViewState {
            selected_model_id: selected.into(),
            model_entries: vec![
                ModelEntryView {
                    id: "small".into(),
                    is_downloaded: true,
                    ..ModelEntryView::default()
                },
                ModelEntryView {
                    id: "large-v3".into(),
                    is_downloaded: false,
                    ..ModelEntryView::default()
                },
            ],
            ..AppViewState::default()
        }
    }

    #[test]
    fn cloud_ready_needs_consent_key_and_no_privacy() {
        let mut s = AppViewState {
            engine: "openai".into(),
            openai_consent: true,
            openai_key_set: true,
            ..AppViewState::default()
        };
        assert!(cloud_ready(&s));
        s.privacy_mode = true;
        assert!(!cloud_ready(&s));
        s.privacy_mode = false;
        s.openai_key_set = false;
        assert!(!cloud_ready(&s));
        s.openai_key_set = true;
        s.engine = "local".into();
        assert!(!cloud_ready(&s));
    }

    #[test]
    fn phase_follows_state_priority() {
        let mut s = with_models("small");
        assert_eq!(phase(&s), Phase::Ready);
        assert!(!needs_model(&AppViewState::default()), "空清單不算缺模型");

        s = with_models("large-v3");
        assert!(needs_model(&s));
        assert_eq!(phase(&s), Phase::NeedsModel);
        s.engine = "openai".into();
        s.openai_consent = true;
        s.openai_key_set = true;
        assert_eq!(phase(&s), Phase::Ready, "雲端可用時不需要本機模型");

        s = with_models("large-v3");
        s.downloading_id = "large-v3".into();
        assert!(!needs_model(&s));
        assert_eq!(phase(&s), Phase::Downloading);
        s.is_processing = true;
        assert_eq!(phase(&s), Phase::Processing);
        s.is_recording = true;
        assert_eq!(phase(&s), Phase::Recording);
    }
}
