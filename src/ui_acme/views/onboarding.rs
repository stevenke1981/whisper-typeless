//! 首次使用：主畫面狀態卡的 `NeedsModel` / `Downloading` 版本（WP-3 擁有；契約提交為可用的最小實作）。
//!
//! WP-2 在 [`super::phase`] 為 [`Phase::NeedsModel`] / [`Phase::Downloading`] 時呼叫 [`card`]；
//! 其他階段不畫任何東西。

use acme_core::{Logical, Rect};

use super::widgets::{
    btn, card as tone_card, progress, Btn, CTRL_SM, FONT_LG, FONT_SM, FONT_XL, FONT_XS, RADIUS_LG,
    SP2, SP3, SP4,
};
use super::{phase, Ctx, Phase};
use crate::core::UiCommand;
use crate::ui_acme::msg::{Msg, SettingsTab};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

/// 缺模型卡的標題。
const TITLE_NEEDS_MODEL: &str = "還差一步：下載語音模型";
/// 比這個寬度窄（例如 380 寬的浮動面板）時改用上下排列的窄版。
const NARROW_W: f32 = 440.0;

/// 缺模型 / 下載中的狀態卡（填滿 `rect`，圓角 14）。寬度小於 440 時改用窄版排列。
pub fn card(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let s = cx.s;
    let t = cx.t;
    let (x0, y0) = (rect.origin.x.get(), rect.origin.y.get());
    let (w0, h0) = (rect.size.width.get(), rect.size.height.get());
    match phase(s) {
        Phase::NeedsModel => {
            tone_card(v, t, rect, RADIUS_LG, Tone::Warning);
            let model = s.model_entries.iter().find(|m| m.id == s.selected_model_id);
            let body = match model {
                Some(m) => format!(
                    "下載 {}（{} MB）即可開始，只需一次，語音不離開電腦。",
                    m.display_name, m.size_mb
                ),
                None => "下載語音模型即可開始，只需一次，語音不離開電腦。".to_string(),
            };
            let download = Some(Msg::Core(UiCommand::ModelSelectOrDownload(
                s.selected_model_id.clone(),
            )));
            let others = Some(Msg::OpenSettings(SettingsTab::Models));
            let btn_w = 104.0;
            if w0 < NARROW_W {
                // 窄版（浮動面板）：標題、單行說明、按鈕排成一列。
                let tw = w0 - 2.0 * SP4;
                v.bold_text(
                    r(x0 + SP4, y0 + 8.0, tw, 24.0),
                    TITLE_NEEDS_MODEL,
                    FONT_LG,
                    t.text,
                    Align::Start,
                );
                let line = cx.text.elide(&body, FONT_XS, false, tw);
                v.text(
                    r(x0 + SP4, y0 + 32.0, tw, 18.0),
                    line,
                    FONT_XS,
                    t.text2,
                    Align::Start,
                );
                let by = y0 + h0 - SP3 - CTRL_SM;
                btn(
                    v,
                    cx,
                    r(x0 + SP4, by, btn_w, CTRL_SM),
                    "下載模型",
                    None,
                    Btn::Primary,
                    download,
                );
                btn(
                    v,
                    cx,
                    r(x0 + SP4 + btn_w + SP2, by, btn_w, CTRL_SM),
                    "選其他模型",
                    None,
                    Btn::Ghost,
                    others,
                );
                return;
            }
            // 寬版按鈕窄一點、間距 8，讓一句說明在 528 寬的卡片裡放得下。
            let btn_w = 88.0;
            let text_w = (w0 - 2.0 * SP4 - btn_w - SP2).max(80.0);
            v.bold_text(
                r(x0 + SP4, y0 + 12.0, text_w, 28.0),
                TITLE_NEEDS_MODEL,
                FONT_XL,
                t.text,
                Align::Start,
            );
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
                download,
            );
            btn(
                v,
                cx,
                r(bx, y0 + 48.0, btn_w, 28.0),
                "選其他模型",
                None,
                Btn::Ghost,
                others,
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
    fn narrow_needs_model_card_keeps_buttons_inside() {
        let s = needs_model_state();
        let ui = UiLocal::default();
        let mut text = TextEngine::new();
        let mut cx = Ctx {
            s: &s,
            ui: &ui,
            t: Theme::for_mode(false),
            text: &mut text,
            scale: 1.0,
        };
        let mut v = View::new();
        let rect = r(16.0, 52.0, 348.0, 88.0);
        card(&mut v, &mut cx, rect);
        let want = Action::Click(Msg::Core(UiCommand::ModelSelectOrDownload("base".into())));
        assert!(v.find_hit(|a| *a == want).is_some());
        for item in &v.items {
            if let Item::Hit { rect: h, .. } = item {
                assert!(h.origin.x.get() + h.size.width.get() <= 16.0 + 348.0);
                assert!(h.origin.y.get() + h.size.height.get() <= 52.0 + 88.0);
            }
        }
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
