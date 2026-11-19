//! 設定面板（兩種模式共用，覆蓋整個視窗；`docs/ui-redesign.md` 2.7）。
//!
//! 版面：標題列 0–44（「設定」、自動儲存狀態、關閉）→ 分頁 44–88（一般 / 辨識 / 模型 /
//! 輸出 / 模板 / 進階）→ 88 以下為目前分頁的可捲動內容。所有設定即改即存，沒有儲存鈕。
//! 每個分頁一個子模組，簽名一致：`(描述樹, 情境, 左緣 x, 起始 y, 內容寬) → 結束的 y`。

mod advanced;
mod engine;
mod general;
mod models;
mod output;
mod templates;
#[cfg(test)]
mod tests;

use acme_core::{CursorHint, Logical, Rect};

use super::live::scrollbar;
use super::widgets::{
    icon_btn, segmented_fill, setting_row, status_pill, toggle, Group, CTRL_MD, FONT_LG, FONT_SM,
    FONT_XS, RADIUS_SM, SP2, SP3, SP4, TOGGLE_W,
};
use super::{overlay, Ctx};
use crate::core::UiCommand;
use crate::ui_acme::icons::Icon;
use crate::ui_acme::msg::{Msg, ScrollId, SettingEdit, SettingsTab};
use crate::ui_acme::paint::{r, Action, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

/// 中文輸出（OpenCC 轉換模式）。`raw` = 不轉換。
pub const CONVERSION_MODES: [(&str, &str); 4] = [
    ("zh-TW", "台灣繁體"),
    ("zh-HK", "香港繁體"),
    ("zh-CN", "簡體"),
    ("raw", "不轉換"),
];

pub const VAD_RANGE: (f32, f32, f32) = (0.1, 0.9, 0.05);
pub const TEMPERATURE_RANGE: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const SILENCE_RANGE: (f32, f32, f32) = (300.0, 5000.0, 100.0);
pub const THREADS_RANGE: (i32, i32) = (1, 32);

/// 轉錄引擎選項（`[transcription] engine`）。
pub const ENGINES: [(&str, &str); 2] = [("local", "本機"), ("openai", "OpenAI 雲端")];
/// OpenAI 轉錄模型。
pub const OPENAI_MODELS: [(&str, &str); 2] = [
    ("gpt-live-transcribe", "gpt-live-transcribe"),
    ("gpt-transcribe", "gpt-transcribe"),
];

/// 核心在儲存成功後放進 `save_status` 的字串。
const SAVED_STATUS: &str = "✓ 已儲存";

/// 標題列高度。
const TITLE_H: f32 = 44.0;
/// 分頁列底緣（內容從這裡開始）。
const HEADER_H: f32 = 88.0;
/// 內容最後的留白。
const BOTTOM_PAD: f32 = SP2;

pub fn panel(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let w = rect.size.width.get();
    let h = rect.size.height.get();
    v.rect(rect, 0.0, t.bg);
    v.hit(rect, 0.0, Action::Block, CursorHint::Default, None, 0);
    header(v, cx, w);

    let body = r(0.0, HEADER_H, w, (h - HEADER_H).max(0.0));
    let x0 = SP4;
    let cw = (w - 2.0 * SP4).max(100.0);
    // 第一輪量高度（丟到暫存描述樹），第二輪才真的畫並套用捲動位移。
    let content_h = {
        let mut scratch = View::new();
        content(&mut scratch, cx, x0, HEADER_H, cw) + BOTTOM_PAD - HEADER_H
    };
    let max = (content_h - body.size.height.get()).max(0.0);
    let offset = cx.ui.scroll(ScrollId::Settings).min(max);
    v.scroll_area(body, ScrollId::Settings, max);
    v.push_clip(body);
    v.push_offset(0.0, -offset);
    content(v, cx, x0, HEADER_H, cw);
    v.pop_offset();
    v.pop_clip();
    if offset > 0.5 {
        v.rect(r(0.0, HEADER_H - 1.0, w, 1.0), 0.0, t.border);
    }
    scrollbar(v, cx, body, offset, max, content_h);

    // 設定面板蓋在錯誤紀錄之上；從「進階 › 錯誤紀錄」打開時要畫在最上層才看得到。
    if cx.s.show_error_log {
        v.layer(8);
        overlay::error_log(v, cx, w, h);
    }
}

/// 標題列（可拖曳）+ 自動儲存狀態 + 關閉 + 分頁列。
fn header(v: &mut View, cx: &mut Ctx<'_>, w: f32) {
    let t = cx.t;
    v.drag_area(r(0.0, 0.0, w, TITLE_H));
    v.bold_text(
        r(SP4, 0.0, 120.0, TITLE_H),
        "設定",
        FONT_LG,
        t.text,
        Align::Start,
    );
    let close_x = w - SP4 - 28.0;
    icon_btn(
        v,
        t,
        r(close_x, (TITLE_H - 28.0) / 2.0, 28.0, 28.0),
        Icon::Close,
        false,
        UiCommand::SetSettingsOpen(false),
    );
    if let Some((label, tone, msg)) = save_pill(&cx.s.save_status) {
        let pw = pill_width(cx, label);
        status_pill(
            v,
            cx,
            close_x - SP2 - pw,
            (TITLE_H - 24.0) / 2.0,
            label,
            tone,
            msg,
        );
    }

    let options: Vec<(&str, &str)> = SettingsTab::ALL
        .iter()
        .map(|tab| (tab.id(), tab.label()))
        .collect();
    segmented_fill(
        v,
        cx,
        r(SP4, TITLE_H + 4.0, w - 2.0 * SP4, CTRL_MD),
        &options,
        cx.ui.settings_tab.id(),
        true,
        |id| Msg::SettingsTab(SettingsTab::from_id(id).unwrap_or_default()),
    );
}

/// 標題列的儲存狀態膠囊：成功 →「已自動儲存」；失敗 →「儲存失敗，請查看錯誤紀錄」（點擊開錯誤紀錄）。
fn save_pill(status: &str) -> Option<(&'static str, Tone, Option<Msg>)> {
    if status.is_empty() {
        None
    } else if status == SAVED_STATUS {
        Some(("已自動儲存", Tone::Success, None))
    } else {
        Some((
            "儲存失敗，請查看錯誤紀錄",
            Tone::Danger,
            Some(Msg::ToggleErrorLog),
        ))
    }
}

/// 目前分頁的內容，回傳結束的 y。
fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    match cx.ui.settings_tab {
        SettingsTab::General => general::content(v, cx, x0, y0, cw),
        SettingsTab::Engine => engine::content(v, cx, x0, y0, cw),
        SettingsTab::Models => models::content(v, cx, x0, y0, cw),
        SettingsTab::Output => output::content(v, cx, x0, y0, cw),
        SettingsTab::Templates => templates::content(v, cx, x0, y0, cw),
        SettingsTab::Advanced => advanced::content(v, cx, x0, y0, cw),
    }
}

// ─── 分頁共用的小元件 ─────────────────────────────────────────────────────────

fn edit(e: SettingEdit) -> Msg {
    Msg::Setting(e)
}

/// [`status_pill`] 的寬度（先量再畫，才能靠右對齊）。
fn pill_width(cx: &mut Ctx<'_>, label: &str) -> f32 {
    32.0 + cx.text.measure_width(label, FONT_XS, true)
}

/// 控制區右緣。
fn right(ctrl: Rect<Logical>) -> f32 {
    ctrl.origin.x.get() + ctrl.size.width.get()
}

/// 標題 + 說明 + 靠右的開關；`msg = None` 時整列停用。回傳列高。
#[allow(clippy::too_many_arguments)]
fn switch_row(
    v: &mut View,
    cx: &mut Ctx<'_>,
    g: &Group,
    y: f32,
    title: &str,
    hint: Option<&str>,
    on: bool,
    msg: Option<Msg>,
    divider: bool,
) -> f32 {
    let (h, ctrl) = setting_row(v, cx, g, y, title, hint, msg.is_some(), divider);
    toggle(
        v,
        cx.t,
        r(
            right(ctrl) - TOGGLE_W,
            ctrl.origin.y.get(),
            TOGGLE_W,
            CTRL_MD,
        ),
        on,
        msg,
    );
    h
}

/// 比預設 160 寬的控制區（分段選項較多時用）：從標題文字後面延伸到列的右緣，最寬 `max_w`。
fn wide_control(
    cx: &mut Ctx<'_>,
    g: &Group,
    ctrl: Rect<Logical>,
    title: &str,
    max_w: f32,
) -> Rect<Logical> {
    let title_w = cx.text.measure_width(title, FONT_SM, false);
    let left = g.x + SP3 + title_w + SP4;
    let right_x = right(ctrl);
    let width = (right_x - left).min(max_w).max(ctrl.size.width.get());
    r(right_x - width, ctrl.origin.y.get(), width, CTRL_MD)
}

/// 停用的文字框外觀：`bg3` 底、`placeholder` 字，沒有命中區域。
fn disabled_field(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>, text: &str) {
    let t = cx.t;
    v.rect(rect, RADIUS_SM, t.bg3);
    let inner = r(
        rect.origin.x.get() + 8.0,
        rect.origin.y.get(),
        (rect.size.width.get() - 16.0).max(0.0),
        rect.size.height.get(),
    );
    let shown = cx.text.elide(text, FONT_SM, false, inner.size.width.get());
    v.text(inner, shown, FONT_SM, t.placeholder, Align::Start);
}

/// 群組外、自動換行的說明文字（11px `text2`）。回傳結束的 y（含下方 8px 間距）。
fn note(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y: f32, cw: f32, text: &str) -> f32 {
    let h = cx.text.measure_wrapped(text, FONT_XS, cw).max(FONT_XS);
    v.wrapped(r(x0, y, cw, h), text, FONT_XS, cx.t.text2);
    y + h + SP2
}
