//! 錯誤紀錄浮層（`docs/ui-redesign.md` 2.8）：置中卡片，每筆「時間 + 訊息」一列。

use super::live::scrollbar;
use super::widgets::{btn, card, Btn, FONT_LG, FONT_SM, FONT_XS, RADIUS_LG, SP2, SP3, SP4};
use super::Ctx;
use crate::core::UiCommand;
use crate::ui_acme::msg::{Msg, ScrollId};
use crate::ui_acme::paint::{r, Action, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

/// 時間欄寬。
const TIME_W: f32 = 64.0;
/// 標題列高。
const HEAD_H: f32 = 56.0;

/// 把「HH:MM:SS 訊息」拆成（時間, 訊息）；開頭不是時間就整串當訊息。
pub(super) fn split_entry(entry: &str) -> (&str, &str) {
    match entry.split_once(' ') {
        Some((head, rest))
            if !head.is_empty()
                && head.contains(':')
                && head.chars().all(|c| c.is_ascii_digit() || c == ':') =>
        {
            (head, rest.trim_start())
        }
        _ => ("", entry),
    }
}

pub fn error_log(v: &mut View, cx: &mut Ctx<'_>, width: f32, height: f32) {
    let t = cx.t;
    let s = cx.s;
    let close_msg = Msg::Core(UiCommand::SetShowErrorLog(false));
    // 遮罩：點擊關閉
    v.rect(r(0.0, 0.0, width, height), 0.0, t.scrim);
    v.hit(
        r(0.0, 0.0, width, height),
        0.0,
        Action::Click(close_msg.clone()),
        acme_core::CursorHint::Default,
        None,
        2,
    );

    let cw = (width - 2.0 * SP4).clamp(0.0, 480.0);
    let msg_w = (cw - 2.0 * SP4 - TIME_W - 6.0 - SP2).max(40.0);
    let mut heights = Vec::with_capacity(s.error_log.len());
    for entry in &s.error_log {
        let (_, msg) = split_entry(entry);
        heights.push(
            cx.text
                .measure_wrapped(msg, FONT_SM, msg_w)
                .max(FONT_SM * 1.4)
                + 2.0 * SP2,
        );
    }
    let content_h = heights.iter().sum::<f32>();
    let body_h = if s.error_log.is_empty() {
        96.0
    } else {
        content_h + SP2
    };
    let ch = (HEAD_H + body_h)
        .min(420.0)
        .min((height - 2.0 * SP2).max(0.0));
    let x0 = (width - cw) / 2.0;
    let y0 = (height - ch) / 2.0;
    let rect = r(x0, y0, cw, ch);
    card(v, t, rect, RADIUS_LG, Tone::Neutral);
    v.hit(
        rect,
        RADIUS_LG,
        Action::Block,
        acme_core::CursorHint::Default,
        None,
        3,
    );

    v.layer(5);
    v.bold_text(
        r(x0 + SP4, y0, (cw - 160.0).max(0.0), HEAD_H),
        format!("錯誤紀錄（{}）", s.error_log.len()),
        FONT_LG,
        t.text,
        Align::Start,
    );
    let close = r(x0 + cw - SP4 - 56.0, y0 + (HEAD_H - 28.0) / 2.0, 56.0, 28.0);
    let clear = r(
        close.origin.x.get() - SP2 - 56.0,
        close.origin.y.get(),
        56.0,
        28.0,
    );
    btn(
        v,
        cx,
        clear,
        "清空",
        None,
        Btn::Danger,
        (!s.error_log.is_empty()).then_some(Msg::Core(UiCommand::ClearErrorLog)),
    );
    btn(v, cx, close, "關閉", None, Btn::Secondary, Some(close_msg));
    v.rect(r(x0, y0 + HEAD_H - 1.0, cw, 1.0), 0.0, t.border);

    let list = r(x0, y0 + HEAD_H, cw, (ch - HEAD_H).max(0.0));
    if s.error_log.is_empty() {
        v.text(list, "目前沒有錯誤", FONT_SM, t.text2, Align::Center);
        return;
    }
    let total = content_h + SP2;
    let max = (total - list.size.height.get()).max(0.0);
    let offset = cx.ui.scroll(ScrollId::ErrorLog).min(max);
    v.scroll_area(list, ScrollId::ErrorLog, max);
    v.push_clip(list);
    v.push_offset(0.0, -offset);
    let mut y = y0 + HEAD_H;
    for (i, (entry, hh)) in s.error_log.iter().zip(&heights).enumerate() {
        let (time, msg) = split_entry(entry);
        if i > 0 {
            v.rect(r(x0 + SP4, y, cw - 2.0 * SP4, 1.0), 0.0, t.border);
        }
        v.text(
            r(x0 + SP4, y + SP2, TIME_W, FONT_SM * 1.4),
            time,
            FONT_XS,
            t.text2,
            Align::Start,
        );
        v.wrapped(
            r(
                x0 + SP4 + TIME_W + SP3.min(SP2),
                y + SP2,
                msg_w,
                hh - 2.0 * SP2,
            ),
            msg,
            FONT_SM,
            t.text,
        );
        y += hh;
    }
    v.pop_offset();
    v.pop_clip();
    scrollbar(v, cx, list, offset, max, total);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::AppViewState;
    use crate::ui_acme::local::UiLocal;
    use crate::ui_acme::paint::Item;
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
        error_log(&mut v, &mut cx, 560.0, 620.0);
        v
    }

    fn has_text(v: &View, needle: &str) -> bool {
        v.items.iter().any(|i| match i {
            Item::Text { content, .. } | Item::Wrapped { content, .. } => content == needle,
            _ => false,
        })
    }

    #[test]
    fn split_entry_separates_time_from_message() {
        assert_eq!(split_entry("10:01:12 下載失敗"), ("10:01:12", "下載失敗"));
        assert_eq!(split_entry("沒有時間 的訊息"), ("", "沒有時間 的訊息"));
        assert_eq!(split_entry("單行"), ("", "單行"));
    }

    #[test]
    fn lists_entries_with_count_title_and_actions() {
        let s = AppViewState {
            error_log: vec!["10:01:12 下載失敗".into(), "10:02:00 找不到麥克風".into()],
            ..AppViewState::default()
        };
        let v = build(&s);
        assert!(has_text(&v, "錯誤紀錄（2）"));
        assert!(has_text(&v, "10:01:12"));
        assert!(has_text(&v, "找不到麥克風"));
        let clear = Action::Click(Msg::Core(UiCommand::ClearErrorLog));
        assert!(v.find_hit(|a| *a == clear).is_some());
    }

    #[test]
    fn empty_log_says_so_and_cannot_clear() {
        let v = build(&AppViewState::default());
        assert!(has_text(&v, "目前沒有錯誤"));
        let clear = Action::Click(Msg::Core(UiCommand::ClearErrorLog));
        assert!(v.find_hit(|a| *a == clear).is_none());
    }
}
