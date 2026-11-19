//! 模板：情境模板清單 + 新增 + 編輯卡（名稱、提示詞、儲存、刪除）。

use super::super::widgets::{
    btn, group_begin, group_end, status_pill, text_field, Btn, CTRL_MD, FONT_SM, FONT_XS,
    GROUP_GAP, SP1, SP2, SP3,
};
use super::super::Ctx;
use super::pill_width;
use crate::ui_acme::icons::Icon;
use crate::ui_acme::msg::{FieldId, Msg};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

/// 模板列高。
const TEMPLATE_ROW_H: f32 = 40.0;

pub(super) fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let t = cx.t;
    let s = cx.s;
    let editing = cx.ui.template_edit.clone();
    let selected = editing.as_ref().and_then(|e| e.index);

    // ── 情境模板 ──
    let g = group_begin(v, cx, x0, y0, cw, "情境模板", Tone::Neutral);
    let mut y = g.y;
    for (i, entry) in s.template_entries.iter().enumerate() {
        let sel = selected == Some(i);
        let row = r(g.x + 1.0, y + 1.0, g.w - 2.0, TEMPLATE_ROW_H - 2.0);
        if sel {
            v.rect(row, super::super::widgets::RADIUS_SM, t.accent_soft);
        } else if i > 0 {
            v.rect(r(g.x + SP3, y, g.w - SP3, 1.0), 0.0, t.border);
        }
        let pill = if entry.is_builtin {
            pill_width(cx, "內建")
        } else {
            0.0
        };
        let name_w = (g.w - 3.0 * SP3 - pill).max(40.0);
        let name = cx.text.elide(&entry.name, FONT_SM, sel, name_w);
        v.styled_text(
            r(g.x + SP3, y, name_w, TEMPLATE_ROW_H),
            name,
            FONT_SM,
            sel,
            if sel { t.accent } else { t.text },
            Align::Start,
        );
        if entry.is_builtin {
            status_pill(
                v,
                cx,
                g.x + g.w - SP3 - pill,
                y + (TEMPLATE_ROW_H - 24.0) / 2.0,
                "內建",
                Tone::Neutral,
                None,
            );
        }
        v.button(
            row,
            super::super::widgets::RADIUS_SM,
            Msg::TemplateEditSelect(i),
            t.hover,
        );
        y += TEMPLATE_ROW_H;
    }
    if s.template_entries.is_empty() {
        v.text(
            r(g.x + SP3, y, g.w - 2.0 * SP3, TEMPLATE_ROW_H),
            "目前沒有模板",
            FONT_SM,
            t.text2,
            Align::Start,
        );
        y += TEMPLATE_ROW_H;
    }
    let mut y = group_end(v, g, y) - GROUP_GAP + SP2;
    btn(
        v,
        cx,
        r(x0, y, 112.0, CTRL_MD),
        "新增模板",
        Some(Icon::Plus),
        Btn::Secondary,
        Some(Msg::TemplateEditNew),
    );
    y += CTRL_MD;

    // ── 編輯卡 ──
    let Some(edit) = editing else {
        return y;
    };
    y += GROUP_GAP;
    let title = if edit.index.is_none() && edit.original.is_empty() {
        "新增模板"
    } else {
        "編輯模板"
    };
    let g = group_begin(v, cx, x0, y, cw, title, Tone::Neutral);
    let inner_x = g.x + SP3;
    let inner_w = g.w - 2.0 * SP3;
    let mut y = g.y + SP3;
    v.text(
        r(inner_x, y, inner_w, 18.0),
        "名稱",
        FONT_SM,
        t.text,
        Align::Start,
    );
    y += 18.0 + SP1;
    text_field(
        v,
        cx,
        r(inner_x, y, inner_w, CTRL_MD),
        FieldId::TemplateName,
    );
    y += CTRL_MD;
    if edit.builtin {
        y += SP1;
        v.text(
            r(inner_x, y, inner_w, 16.0),
            "內建模板的名稱不能修改",
            FONT_XS,
            t.text2,
            Align::Start,
        );
        y += 16.0;
    }
    y += SP3;
    v.text(
        r(inner_x, y, inner_w, 18.0),
        "提示詞",
        FONT_SM,
        t.text,
        Align::Start,
    );
    y += 18.0;
    let hint = "告訴辨識引擎這段話的情境與常見用詞";
    let hint_h = cx.text.measure_wrapped(hint, FONT_XS, inner_w).max(FONT_XS);
    v.wrapped(r(inner_x, y, inner_w, hint_h), hint, FONT_XS, t.text2);
    y += hint_h + SP1;
    text_field(
        v,
        cx,
        r(inner_x, y, inner_w, CTRL_MD),
        FieldId::TemplatePrompt,
    );
    y += CTRL_MD + SP1;
    // 單行欄位看不到全文：在下方顯示字數，讓長提示詞也有回饋。
    let count = cx
        .ui
        .fields
        .get(FieldId::TemplatePrompt)
        .text
        .chars()
        .count();
    v.text(
        r(inner_x, y, inner_w, 16.0),
        format!("{count} 字"),
        FONT_XS,
        t.text2,
        Align::End,
    );
    y += 16.0 + SP3;
    btn(
        v,
        cx,
        r(inner_x, y, 72.0, CTRL_MD),
        "儲存",
        None,
        Btn::Primary,
        Some(Msg::TemplateEditSave),
    );
    if !edit.builtin {
        btn(
            v,
            cx,
            r(inner_x + 72.0 + SP2, y, 72.0, CTRL_MD),
            "刪除",
            None,
            Btn::Danger,
            Some(Msg::TemplateEditDelete),
        );
    }
    y += CTRL_MD + SP3;
    group_end(v, g, y) - GROUP_GAP
}
