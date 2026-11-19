//! 錯誤紀錄浮層。

use super::live::scrollbar;
use super::widgets::{text_button, FONT_MD, FONT_XS};
use super::Ctx;
use crate::core::UiCommand;
use crate::ui_acme::msg::{Msg, ScrollId};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;

pub fn error_log(v: &mut View, cx: &mut Ctx<'_>, width: f32, height: f32) {
    let t = cx.t;
    let s = cx.s;
    // 遮罩：點擊關閉
    v.rect(r(0.0, 0.0, width, height), 0.0, t.scrim);
    v.hit(
        r(0.0, 0.0, width, height),
        0.0,
        crate::ui_acme::paint::Action::Click(Msg::Core(UiCommand::SetShowErrorLog(false))),
        acme_core::CursorHint::Default,
        None,
        2,
    );
    let cw = (width - 32.0).clamp(0.0, 480.0);
    let ch = (height - 80.0).clamp((height - 8.0).clamp(0.0, 120.0), 420.0);
    let x0 = (width - cw) / 2.0;
    let y0 = (height - ch) / 2.0;
    let card = r(x0, y0, cw, ch);
    v.bordered(card, 12.0, t.bg2, 1.0, t.border);
    v.hit(
        card,
        12.0,
        crate::ui_acme::paint::Action::Block,
        acme_core::CursorHint::Default,
        None,
        3,
    );

    v.bold_text(
        r(x0 + 12.0, y0 + 8.0, cw - 150.0, 28.0),
        format!("⚠ 錯誤紀錄 ({})", s.error_log.len()),
        FONT_MD,
        t.text,
        Align::Start,
    );
    let close = r(x0 + cw - 12.0 - 50.0, y0 + 10.0, 50.0, 24.0);
    let clear = r(close.origin.x.get() - 4.0 - 60.0, y0 + 10.0, 60.0, 24.0);
    v.layer(5);
    text_button(
        v,
        t,
        clear,
        "清空",
        FONT_XS,
        t.danger,
        t.danger_soft,
        None,
        Some(Msg::Core(UiCommand::ClearErrorLog)),
    );
    text_button(
        v,
        t,
        close,
        "關閉",
        FONT_XS,
        t.text2,
        t.bg3,
        None,
        Some(Msg::Core(UiCommand::SetShowErrorLog(false))),
    );

    let list = r(x0 + 12.0, y0 + 44.0, cw - 24.0, (ch - 56.0).max(0.0));
    v.rect(list, 8.0, t.bg);
    let inner_w = (cw - 24.0 - 22.0).max(20.0);
    let mut heights = Vec::with_capacity(s.error_log.len());
    let mut content_h = 8.0;
    for entry in &s.error_log {
        let hh = cx
            .text
            .measure_wrapped(&format!("• {entry}"), FONT_XS, inner_w);
        heights.push(hh);
        content_h += hh + 4.0;
    }
    if s.error_log.is_empty() {
        content_h += 20.0;
    }
    content_h += 8.0;
    let max = (content_h - list.size.height.get()).max(0.0);
    let offset = cx.ui.scroll(ScrollId::ErrorLog).min(max);
    v.scroll_area(list, ScrollId::ErrorLog, max);
    v.push_clip(list);
    v.push_offset(0.0, -offset);
    let mut y = list.origin.y.get() + 8.0;
    if s.error_log.is_empty() {
        v.text(
            r(list.origin.x.get() + 8.0, y, inner_w, 20.0),
            "沒有錯誤紀錄",
            FONT_XS,
            t.placeholder,
            Align::Start,
        );
    }
    for (entry, hh) in s.error_log.iter().zip(heights) {
        v.wrapped(
            r(list.origin.x.get() + 8.0, y, inner_w, hh),
            format!("• {entry}"),
            FONT_XS,
            t.text2,
        );
        y += hh + 4.0;
    }
    v.pop_offset();
    v.pop_clip();
    scrollbar(v, cx, list, offset, max, content_h);
}
