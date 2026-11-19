//! 首次使用：主畫面狀態卡的 `NeedsModel` / `Downloading` 版本（WP-3 擁有；契約提交為可用的最小實作）。
//!
//! WP-2 在 [`super::phase`] 為 [`Phase::NeedsModel`] / [`Phase::Downloading`] 時呼叫 [`card`]；
//! 其他階段不畫任何東西。

use acme_core::{Logical, Rect};

use super::widgets::{
    btn, card as tone_card, progress, Btn, FONT_SM, FONT_XL, FONT_XS, RADIUS_LG, SP3, SP4,
};
use super::{phase, Ctx, Phase};
use crate::core::UiCommand;
use crate::ui_acme::msg::{Msg, SettingsTab};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

/// 缺模型 / 下載中的狀態卡（填滿 `rect`，圓角 14）。
pub fn card(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let s = cx.s;
    let t = cx.t;
    let (x0, y0) = (rect.origin.x.get(), rect.origin.y.get());
    let (w0, h0) = (rect.size.width.get(), rect.size.height.get());
    match phase(s) {
        Phase::NeedsModel => {
            tone_card(v, t, rect, RADIUS_LG, Tone::Warning);
            let btn_w = 104.0;
            let text_w = (w0 - 2.0 * SP4 - btn_w - SP3).max(80.0);
            v.bold_text(
                r(x0 + SP4, y0 + 12.0, text_w, 28.0),
                "還差一步：下載語音模型",
                FONT_XL,
                t.text,
                Align::Start,
            );
            let model = s.model_entries.iter().find(|m| m.id == s.selected_model_id);
            let body = match model {
                Some(m) => format!(
                    "下載 {}（{} MB）後即可開始，只需一次；語音只在本機處理。",
                    m.display_name, m.size_mb
                ),
                None => "下載語音模型後即可開始，只需一次；語音只在本機處理。".to_string(),
            };
            let body_h = (h0 - 44.0 - 8.0).max(FONT_SM);
            v.wrapped(
                r(x0 + SP4, y0 + 44.0, text_w, body_h),
                body,
                FONT_SM,
                t.text2,
            );
            let bx = x0 + w0 - SP4 - btn_w;
            btn(
                v,
                cx,
                r(bx, y0 + 12.0, btn_w, 32.0),
                "下載模型",
                None,
                Btn::Primary,
                Some(Msg::Core(UiCommand::ModelSelectOrDownload(
                    s.selected_model_id.clone(),
                ))),
            );
            btn(
                v,
                cx,
                r(bx, y0 + 48.0, btn_w, 28.0),
                "選其他模型",
                None,
                Btn::Ghost,
                Some(Msg::OpenSettings(SettingsTab::Models)),
            );
        }
        Phase::Downloading => {
            tone_card(v, t, rect, RADIUS_LG, Tone::Accent);
            let pct = (s.download_progress.clamp(0.0, 1.0) * 100.0).round() as i32;
            let tw = w0 - 2.0 * SP4;
            v.bold_text(
                r(x0 + SP4, y0 + 12.0, tw, 28.0),
                format!("正在下載模型 {pct}%"),
                FONT_XL,
                t.text,
                Align::Start,
            );
            progress(
                v,
                t,
                r(x0 + SP4, y0 + 48.0, tw, 6.0),
                s.download_progress,
                t.accent,
            );
            v.text(
                r(x0 + SP4, y0 + 58.0, tw, 20.0),
                "下載完成後會自動載入",
                FONT_XS,
                t.text2,
                Align::Start,
            );
        }
        Phase::Ready | Phase::Recording | Phase::Processing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AppViewState, ModelEntryView};
    use crate::ui_acme::local::UiLocal;
    use crate::ui_acme::paint::{Action, Item};
    use crate::ui_acme::text::TextEngine;
    use crate::ui_acme::theme::Theme;

    fn build(s: &AppViewState) -> View {
        let ui = UiLocal::default();
        let mut text = TextEngine::new();
        let mut cx = Ctx {
            s,
            ui: &ui,
            t: Theme::for_mode(false),
            text: &mut text,
            scale: 1.0,
        };
        let mut v = View::new();
        card(&mut v, &mut cx, r(16.0, 52.0, 528.0, 88.0));
        v
    }

    fn needs_model_state() -> AppViewState {
        AppViewState {
            selected_model_id: "base".into(),
            model_entries: vec![ModelEntryView {
                id: "base".into(),
                display_name: "Base".into(),
                size_mb: 142,
                ..ModelEntryView::default()
            }],
            ..AppViewState::default()
        }
    }

    #[test]
    fn needs_model_card_offers_download_of_selected_model() {
        let v = build(&needs_model_state());
        let want = Action::Click(Msg::Core(UiCommand::ModelSelectOrDownload("base".into())));
        assert!(v.find_hit(|a| *a == want).is_some());
        let other = Action::Click(Msg::OpenSettings(SettingsTab::Models));
        assert!(v.find_hit(|a| *a == other).is_some());
    }

    #[test]
    fn downloading_card_shows_percent_and_ready_draws_nothing() {
        let mut s = needs_model_state();
        s.downloading_id = "base".into();
        s.download_progress = 0.42;
        let v = build(&s);
        assert!(v
            .items
            .iter()
            .any(|i| matches!(i, Item::Text { content, .. } if content == "正在下載模型 42%")));
        assert_eq!(v.hit_count(), 0);
        assert!(build(&AppViewState::default()).items.is_empty());
    }
}
