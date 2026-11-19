//! 即時模式：浮動緊湊列、完整視窗外框、模板列、波形、轉錄紀錄、快速操作、控制列、狀態列。

use acme_core::{Logical, Rect};

use super::widgets::{
    chip, icon_button, record_button, waveform, FONT_MD, FONT_SM, FONT_XS, RADIUS_MD, RADIUS_SM,
};
use super::{file, template_label, Ctx, BAR_H, TEMPLATES};
use crate::core::{AppViewState, UiCommand};
use crate::ui_acme::msg::{Msg, ScrollId};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::{Align, TextEngine};

fn rx(rect: Rect<Logical>) -> (f32, f32, f32, f32) {
    (
        rect.origin.x.get(),
        rect.origin.y.get(),
        rect.size.width.get(),
        rect.size.height.get(),
    )
}

// ─── 浮動模式 ────────────────────────────────────────────────────────────────

/// 浮動視窗：緊湊列 + 展開內容。
pub fn floating(v: &mut View, cx: &mut Ctx<'_>, width: f32, height: f32) {
    let t = cx.t;
    v.bordered(r(0.0, 0.0, width, height), 14.0, t.glass_bg, 1.0, t.border);
    compact_bar(v, cx, width);
    if cx.s.floating_expanded && height > BAR_H + 1.0 {
        let mut y = BAR_H;
        quick_actions(v, cx, r(0.0, y, width, 38.0));
        y += 38.0;
        let body = r(0.0, y, width, (height - y).max(0.0));
        if cx.s.file_mode {
            file::view(v, cx, body);
        } else {
            live_body(v, cx, body);
        }
    }
}

/// 380×52 緊湊列：拖曳把手、錄音鍵、迷你波形、即時文字、工具列。
pub fn compact_bar(v: &mut View, cx: &mut Ctx<'_>, width: f32) {
    let t = cx.t;
    let s = cx.s;
    // 整條都可以拖曳；按鈕壓在上面
    v.drag_area(r(0.0, 0.0, width, BAR_H));
    v.rect(r(8.0, 14.0, 6.0, 24.0), 3.0, t.glass_hairline);
    v.rect(r(8.5, 14.5, 5.0, 23.0), 2.5, t.border);

    record_button(v, cx, r(22.0, 8.0, 36.0, 36.0));

    let wave_on = s.mod_waveform;
    let wave_w = if wave_on { 72.0 } else { 0.0 };
    if wave_on {
        waveform(
            v,
            t,
            r(66.0, 0.0, wave_w, BAR_H),
            &s.waveform_bars,
            18,
            2.0,
            2.0,
            3.0,
            28.0,
            s.is_recording,
        );
    }

    // 工具列（由右往左）
    let tool = 26.0;
    let gap = 2.0;
    let tools: [(&str, Msg, bool); 5] = [
        ("✕", Msg::Core(UiCommand::RequestClose), false),
        ("⛶", Msg::ToggleFloating, false),
        (
            if s.floating_expanded { "˄" } else { "˅" },
            Msg::ToggleExpanded,
            false,
        ),
        ("⚙", Msg::ToggleSettings, s.settings_open),
        ("📌", Msg::TogglePin, cx.ui.pinned),
    ];
    let mut tx = width - 8.0 - tool;
    for (label, msg, active) in tools {
        let rect = r(tx, (BAR_H - tool) / 2.0, tool, tool);
        icon_button(
            v,
            t,
            rect,
            label,
            FONT_SM,
            if active { t.on_accent } else { t.text2 },
            active.then_some(t.accent),
            msg,
        );
        tx -= tool + gap;
    }
    let text_x = 66.0 + wave_w + 8.0;
    let text_w = (tx + tool + gap - 8.0 - text_x).max(0.0);
    let partial = !s.current_partial.is_empty();
    let content = if partial {
        &s.current_partial
    } else {
        &s.status_text
    };
    let shown = cx.text.elide(content, FONT_SM, partial, text_w);
    v.styled_text(
        r(text_x, 0.0, text_w, BAR_H),
        shown,
        FONT_SM,
        partial,
        if partial { t.text } else { t.text2 },
        Align::Start,
    );
}

/// 展開後的快速操作列：模板、模型、剪貼簿 / 注入開關、清除、錯誤徽章。
pub fn quick_actions(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let (x0, y0, w, h) = rx(rect);
    v.rect(r(x0, y0, w, 1.0), 0.0, t.glass_hairline);
    v.rect(r(x0, y0 + h - 1.0, w, 1.0), 0.0, t.glass_hairline);
    let ch = 26.0;
    let cy = y0 + (h - ch) / 2.0;
    let mut x = x0 + 10.0;
    x += chip(
        v,
        cx,
        x,
        cy,
        ch,
        "✦",
        template_label(&s.current_template),
        true,
        t.accent,
        UiCommand::TemplateSelected(next_template(&s.current_template).to_string()),
    ) + 6.0;
    let model = s.model_name.clone();
    x += chip(
        v,
        cx,
        x,
        cy,
        ch,
        "⌬",
        &model,
        false,
        t.accent,
        UiCommand::OpenModelManager,
    ) + 6.0;
    x += chip(
        v,
        cx,
        x,
        cy,
        ch,
        "📋",
        "剪貼簿",
        s.clipboard_enabled,
        t.success,
        UiCommand::SetClipboardEnabled(!s.clipboard_enabled),
    ) + 6.0;
    x += chip(
        v,
        cx,
        x,
        cy,
        ch,
        "⌨",
        "注入",
        s.inject_enabled,
        t.accent,
        UiCommand::SetInjectEnabled(!s.inject_enabled),
    ) + 6.0;
    chip(
        v,
        cx,
        x,
        cy,
        ch,
        "✕",
        "清除",
        false,
        t.accent,
        UiCommand::ClearTranscript,
    );
    error_badge(v, cx, r(x0 + w - 10.0 - 34.0, cy, 34.0, ch));
}

/// 模板 chip 點擊：輪到下一個模板。
pub fn next_template(current: &str) -> &'static str {
    let i = TEMPLATES
        .iter()
        .position(|(id, _, _)| *id == current)
        .map_or(0, |i| (i + 1) % TEMPLATES.len());
    TEMPLATES[i].0
}

/// 錯誤紀錄徽章（有錯誤時顯示數量）。
pub fn error_badge(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let count = cx.s.error_log.len();
    let radius = rect.size.height.get() / 2.0;
    if count > 0 {
        v.rect(rect, radius, t.danger_soft);
    }
    let label = if count > 0 {
        format!("⚠ {count}")
    } else {
        "⚠".to_string()
    };
    v.text(
        rect,
        label,
        FONT_XS,
        if count > 0 { t.danger } else { t.text2 },
        Align::Center,
    );
    v.button(
        rect,
        radius,
        UiCommand::SetShowErrorLog(!cx.s.show_error_log),
        t.hover,
    );
}

// ─── 完整模式 ────────────────────────────────────────────────────────────────

/// 完整視窗：標題列、分頁、內容、控制列、狀態列。
pub fn full(v: &mut View, cx: &mut Ctx<'_>, width: f32, height: f32) {
    let t = cx.t;
    let s = cx.s;
    v.rect(r(0.0, 0.0, width, height), 0.0, t.bg);
    title_bar(v, cx, width);
    tabs(v, cx, r(0.0, 52.0, width, 38.0));
    let status_h = 24.0;
    let top = 90.0;
    let bottom = (height - status_h).max(top);
    if s.file_mode {
        file::view(v, cx, r(0.0, top, width, bottom - top));
    } else {
        let control_h = 64.0;
        let body_bottom = (bottom - control_h).max(top);
        live_body(v, cx, r(0.0, top, width, body_bottom - top));
        control_bar(v, cx, r(0.0, body_bottom, width, control_h));
    }
    status_bar(v, cx, r(0.0, bottom, width, status_h));
}

/// 標題列（自繪：無 OS 外框）。
fn title_bar(v: &mut View, cx: &mut Ctx<'_>, width: f32) {
    let t = cx.t;
    let s = cx.s;
    v.rect(r(0.0, 0.0, width, 52.0), 0.0, t.bg2);
    v.rect(r(0.0, 51.0, width, 1.0), 0.0, t.border);
    v.drag_area(r(0.0, 0.0, width, 52.0));
    v.text(r(16.0, 0.0, 24.0, 52.0), "🎙", 18.0, t.text, Align::Start);
    v.bold_text(
        r(44.0, 0.0, 200.0, 52.0),
        "whisper-typeless",
        FONT_MD,
        t.text,
        Align::Start,
    );
    let b = 32.0;
    let by = 10.0;
    let mut x = width - 16.0 - b;
    // 關閉、最小化
    icon_button(
        v,
        t,
        r(x, by, b, b),
        "✕",
        14.0,
        t.text2,
        Some(t.bg3),
        UiCommand::RequestClose,
    );
    x -= b + 6.0;
    icon_button(
        v,
        t,
        r(x, by, b, b),
        "—",
        14.0,
        t.text2,
        Some(t.bg3),
        Msg::Minimize,
    );
    x -= b + 12.0;
    // 設定
    icon_button(
        v,
        t,
        r(x, by, b, b),
        "⚙",
        14.0,
        if s.settings_open {
            t.on_accent
        } else {
            t.text2
        },
        Some(if s.settings_open { t.accent } else { t.bg3 }),
        Msg::ToggleSettings,
    );
    x -= b + 6.0;
    // 錯誤紀錄
    let count = s.error_log.len();
    let rect = r(x, by, b, b);
    v.rect(
        rect,
        RADIUS_SM,
        if count > 0 { t.danger_soft } else { t.bg3 },
    );
    v.text(
        rect,
        if count > 0 {
            format!("⚠ {count}")
        } else {
            "⚠".into()
        },
        FONT_XS,
        if count > 0 { t.danger } else { t.text2 },
        Align::Center,
    );
    v.button(
        rect,
        RADIUS_SM,
        UiCommand::SetShowErrorLog(!s.show_error_log),
        t.hover,
    );
    x -= b + 6.0;
    // 主題
    icon_button(
        v,
        t,
        r(x, by, b, b),
        if s.dark_mode { "☀" } else { "🌙" },
        14.0,
        t.text2,
        Some(t.bg3),
        UiCommand::SetDarkMode(!s.dark_mode),
    );
    x -= b + 6.0;
    // 切到浮動
    icon_button(
        v,
        t,
        r(x, by, b, b),
        "⛶",
        14.0,
        t.text2,
        Some(t.bg3),
        Msg::ToggleFloating,
    );
}

/// 即時 / 檔案 分頁。
fn tabs(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let (x0, y0, w, h) = rx(rect);
    v.rect(rect, 0.0, t.bg2);
    v.rect(r(x0, y0 + h - 1.0, w, 1.0), 0.0, t.border);
    for (i, (label, file_mode)) in [("🎙  即時", false), ("📄  檔案", true)]
        .into_iter()
        .enumerate()
    {
        let tab = r(x0 + 16.0 + i as f32 * 84.0, y0, 80.0, h);
        let active = cx.s.file_mode == file_mode;
        if active {
            v.rect(
                r(tab.origin.x.get(), y0 + h - 2.0, 80.0, 2.0),
                0.0,
                t.accent,
            );
        }
        v.styled_text(
            tab,
            label,
            FONT_SM,
            active,
            if active { t.accent } else { t.text2 },
            Align::Center,
        );
        if !active {
            v.button(tab, 0.0, UiCommand::SetFileMode(file_mode), t.hover);
        }
    }
}

/// 即時內容：模板列（可關）、波形（可關）、轉錄紀錄。
pub fn live_body(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let (x0, mut y, w, h) = rx(rect);
    let bottom = y + h;
    if cx.s.mod_context {
        context_bar(v, cx, r(x0, y, w, 44.0));
        y += 44.0;
    }
    if cx.s.mod_waveform {
        let t = cx.t;
        let s = cx.s;
        let wr = r(x0, y, w, 56.0);
        v.rect(r(x0 + 8.0, y + 27.5, w - 16.0, 1.0), 0.0, t.border);
        waveform(
            v,
            t,
            wr,
            &s.waveform_bars,
            20,
            3.0,
            3.0,
            4.0,
            48.0,
            s.is_recording,
        );
        y += 56.0;
    }
    transcript(v, cx, r(x0, y, w, (bottom - y).max(0.0)));
}

/// 情境模板列。
pub fn context_bar(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let (x0, y0, w, h) = rx(rect);
    v.rect(rect, 0.0, t.bg2);
    v.rect(r(x0, y0 + h - 1.0, w, 1.0), 0.0, t.border);
    let n = TEMPLATES.len() as f32;
    let cw = ((w - 24.0 - (n - 1.0) * 4.0) / n).min(70.0);
    let total = n * cw + (n - 1.0) * 4.0;
    let mut x = x0 + (w - total) / 2.0;
    for (id, icon, name) in TEMPLATES {
        let selected = cx.s.current_template == id;
        let chip = r(x, y0 + (h - 32.0) / 2.0, cw, 32.0);
        if selected {
            v.rect(chip, 16.0, t.accent);
        }
        v.text(
            chip,
            format!("{icon} {name}"),
            FONT_XS,
            if selected { t.on_accent } else { t.text2 },
            Align::Center,
        );
        v.button(
            chip,
            16.0,
            UiCommand::TemplateSelected(id.to_string()),
            t.hover,
        );
        x += cw + 4.0;
    }
}

/// 轉錄紀錄卡片的高度。
fn card_height(cx: &mut Ctx<'_>, header: bool, text: &str, inner_w: f32) -> f32 {
    let header_h = if header { FONT_XS * 1.3 + 4.0 } else { 0.0 };
    20.0 + header_h + cx.text.measure_wrapped(text, FONT_MD, inner_w)
}

/// 可捲動的轉錄紀錄（zh-TW 自動換行，新段落自動捲到底）。
pub fn transcript(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let (x0, y0, w, h) = rx(rect);
    if h <= 0.0 {
        return;
    }
    let pad = 16.0;
    let card_w = (w - 2.0 * pad - 6.0).max(40.0);
    let inner_w = card_w - 20.0;

    // 第一輪：量高度
    let mut heights = Vec::with_capacity(s.transcript_segments.len());
    for seg in &s.transcript_segments {
        let hh = card_height(cx, !seg.timestamp.is_empty(), &seg.text, inner_w);
        heights.push(hh);
    }
    let partial_h = if s.current_partial.is_empty() {
        0.0
    } else {
        card_height(cx, true, &s.current_partial, inner_w)
    };
    let empty = s.transcript_segments.is_empty() && s.current_partial.is_empty();
    let mut content_h = pad;
    for hh in &heights {
        content_h += hh + 8.0;
    }
    if partial_h > 0.0 {
        content_h += partial_h + 8.0;
    }
    if empty {
        content_h += 120.0;
    }
    content_h += pad;
    let max = (content_h - h).max(0.0);
    let offset = if cx.ui.follow_transcript {
        max
    } else {
        cx.ui.scroll(ScrollId::Transcript).min(max)
    };
    v.scroll_area(rect, ScrollId::Transcript, max);

    v.push_clip(rect);
    v.push_offset(0.0, -offset);
    let mut y = y0 + pad;
    let x = x0 + pad;
    for (seg, hh) in s.transcript_segments.iter().zip(&heights) {
        let visible = y + hh >= y0 + offset && y <= y0 + offset + h;
        if visible {
            v.bordered(r(x, y, card_w, *hh), RADIUS_MD, t.bg2, 1.0, t.border);
            let mut ty = y + 10.0;
            if !seg.timestamp.is_empty() {
                v.text(
                    r(x + 10.0, ty, inner_w, FONT_XS * 1.3),
                    seg.timestamp.clone(),
                    FONT_XS,
                    t.placeholder,
                    Align::Start,
                );
                ty += FONT_XS * 1.3 + 4.0;
            }
            v.wrapped(
                r(x + 10.0, ty, inner_w, hh - (ty - y) - 10.0),
                seg.text.clone(),
                FONT_MD,
                t.text,
            );
        }
        y += hh + 8.0;
    }
    if partial_h > 0.0 {
        v.bordered(r(x, y, card_w, partial_h), RADIUS_MD, t.bg3, 1.0, t.border);
        v.text(
            r(x + 10.0, y + 10.0, inner_w, FONT_XS * 1.3),
            if s.is_processing {
                "辨識中..."
            } else {
                "等待聲音..."
            },
            FONT_XS,
            t.accent,
            Align::Start,
        );
        let ty = y + 10.0 + FONT_XS * 1.3 + 4.0;
        v.wrapped(
            r(x + 10.0, ty, inner_w, partial_h - (ty - y) - 10.0),
            s.current_partial.clone(),
            FONT_MD,
            t.text2,
        );
    }
    if empty {
        v.text(r(x0, y + 10.0, w, 56.0), "🎙", 40.0, t.text2, Align::Center);
        v.text(
            r(x0, y + 70.0, w, 24.0),
            "按下錄音按鈕開始語音輸入",
            FONT_MD,
            t.placeholder,
            Align::Center,
        );
    }
    v.pop_offset();
    v.pop_clip();
    scrollbar(v, cx, rect, offset, max, content_h);
}

/// 細捲軸（有可捲動內容時）。
pub fn scrollbar(
    v: &mut View,
    cx: &mut Ctx<'_>,
    rect: Rect<Logical>,
    offset: f32,
    max: f32,
    content_h: f32,
) {
    if max <= 0.0 || content_h <= 0.0 {
        return;
    }
    let (x0, y0, w, h) = rx(rect);
    let thumb_h = (h * h / content_h).max(24.0).min(h);
    let ty = y0 + (h - thumb_h) * (offset / max).clamp(0.0, 1.0);
    v.rect(
        r(x0 + w - 6.0, ty + 2.0, 4.0, thumb_h - 4.0),
        2.0,
        cx.t.border_strong,
    );
}

/// 完整模式的控制列：錄音、清除、剪貼簿、注入。
fn control_bar(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let (x0, y0, w, h) = rx(rect);
    v.rect(rect, 0.0, t.bg2);
    v.rect(r(x0, y0, w, 1.0), 0.0, t.border);
    record_button(v, cx, r(x0 + 16.0, y0 + (h - 44.0) / 2.0, 44.0, 44.0));
    let clear = r(x0 + 68.0, y0 + (h - 36.0) / 2.0, 36.0, 36.0);
    v.bordered(clear, 18.0, t.bg3, 1.0, t.border);
    v.text(clear, "🗑", 14.0, t.text2, Align::Center);
    v.button(clear, 18.0, UiCommand::ClearTranscript, t.hover);

    let toggles = [
        (
            "⌨",
            s.inject_enabled,
            t.accent_soft,
            t.accent_line,
            UiCommand::SetInjectEnabled(!s.inject_enabled),
        ),
        (
            "📋",
            s.clipboard_enabled,
            t.success_soft,
            t.success_line,
            UiCommand::SetClipboardEnabled(!s.clipboard_enabled),
        ),
    ];
    let mut x = x0 + w - 16.0 - 36.0;
    for (icon, on, soft, line, cmd) in toggles {
        let rect = r(x, y0 + (h - 28.0) / 2.0, 36.0, 28.0);
        v.bordered(
            rect,
            RADIUS_SM,
            if on { soft } else { t.bg3 },
            1.0,
            if on { line } else { t.border },
        );
        v.text(
            rect,
            icon,
            14.0,
            if on { t.text } else { t.text2 },
            Align::Center,
        );
        v.button(rect, RADIUS_SM, cmd, t.hover);
        x -= 36.0 + 8.0;
    }
}

/// 狀態列項目的色調。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Dim,
    Accent,
    Recording,
}

/// 已排版的狀態列項目：`x` 相對於狀態列左緣，`w` 為寬度。
#[derive(Clone, Debug)]
pub(super) struct StatusItem {
    pub label: String,
    pub tone: Tone,
    pub x: f32,
    pub w: f32,
}

/// 狀態列排版結果：右側的資訊項目與左側（省略後的）狀態文字。
#[derive(Clone, Debug, Default)]
pub(super) struct StatusLayout {
    pub items: Vec<StatusItem>,
    pub status: String,
    pub status_x: f32,
    pub status_w: f32,
}

const STATUS_PAD: f32 = 12.0;
const STATUS_GAP: f32 = 10.0;
const STATUS_MIN_TEXT: f32 = 56.0;

/// 把裝置標籤縮短（"CPU（此版本未含 GPU）" → "CPU"，"GPU (Vulkan)" → "Vulkan"）。
pub(super) fn short_device(label: &str) -> String {
    if label.starts_with("CPU") {
        "CPU".into()
    } else if label.contains("CUDA") {
        "CUDA".into()
    } else if label.contains("Vulkan") {
        "Vulkan".into()
    } else if label.contains("Metal") {
        "Metal".into()
    } else {
        label.split(" (").next().unwrap_or(label).to_string()
    }
}

/// 在 `width` 內排版狀態列。資訊項目依優先順序（最後才捨棄：模型 > 裝置 > 熱鍵 >
/// 字數 > PTT 熱鍵）捨棄；仍放不下時將唯一剩下的項目省略；狀態文字取剩餘空間。
pub(super) fn status_layout(text: &mut TextEngine, s: &AppViewState, width: f32) -> StatusLayout {
    // 顯示順序：字數、模型、裝置、熱鍵、PTT；drop_rank 越小越先捨棄。
    let mut pieces: Vec<(String, Tone, u8)> = vec![
        (format!("{} 字", s.char_count), Tone::Dim, 1),
        (s.model_name.clone(), Tone::Accent, 4),
        (
            short_device(&s.device_label),
            if s.device_label.starts_with("CPU") {
                Tone::Dim
            } else {
                Tone::Recording
            },
            3,
        ),
        (s.hotkey.clone(), Tone::Dim, 2),
    ];
    if s.ptt_mode {
        pieces.push((format!("🎤 {}", s.ptt_hotkey), Tone::Recording, 0));
    }
    let avail = (width - STATUS_PAD * 2.0).max(0.0);
    let mut widths: Vec<f32> = pieces
        .iter()
        .map(|(l, _, _)| text.measure_width(l, FONT_XS, false))
        .collect();
    let total = |pieces: &[(String, Tone, u8)], widths: &[f32]| -> f32 {
        let n = pieces.len() as f32;
        widths.iter().sum::<f32>() + (n - 1.0).max(0.0) * STATUS_GAP
    };
    while pieces.len() > 1 && total(&pieces, &widths) + STATUS_GAP + STATUS_MIN_TEXT > avail {
        let Some(idx) = (0..pieces.len()).min_by_key(|&i| pieces[i].2) else {
            break;
        };
        pieces.remove(idx);
        widths.remove(idx);
    }
    if pieces.len() == 1 && widths[0] > avail {
        let label = text.elide(&pieces[0].0, FONT_XS, false, avail);
        widths[0] = text.measure_width(&label, FONT_XS, false).min(avail);
        pieces[0].0 = label;
    }
    let mut items = Vec::with_capacity(pieces.len());
    let mut x = width - STATUS_PAD - total(&pieces, &widths);
    for ((label, tone, _), w) in pieces.into_iter().zip(widths) {
        items.push(StatusItem { label, tone, x, w });
        x += w + STATUS_GAP;
    }
    let first_x = items.first().map_or(width - STATUS_PAD, |i| i.x);
    let status_w = (first_x - STATUS_PAD - STATUS_GAP).max(0.0);
    let status = text.elide(&s.status_text, FONT_XS, false, status_w);
    StatusLayout {
        items,
        status,
        status_x: STATUS_PAD,
        status_w,
    }
}

/// 狀態列：狀態文字、字數、模型、裝置、熱鍵（放不下時依序捨棄 / 省略）。
fn status_bar(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let (x0, y0, w, h) = rx(rect);
    v.rect(rect, 0.0, t.bg);
    let layout = status_layout(cx.text, cx.s, w);
    for (i, item) in layout.items.iter().enumerate() {
        let color = match item.tone {
            Tone::Dim => t.text2,
            Tone::Accent => t.accent,
            Tone::Recording => t.recording,
        };
        v.text(
            r(x0 + item.x, y0, item.w + 1.0, h),
            item.label.clone(),
            FONT_XS,
            color,
            Align::Start,
        );
        if i > 0 {
            v.rect(
                r(x0 + item.x - STATUS_GAP / 2.0, y0 + 6.0, 1.0, 12.0),
                0.0,
                t.border,
            );
        }
    }
    v.text(
        r(x0 + layout.status_x, y0, layout.status_w, h),
        layout.status,
        FONT_XS,
        t.text2,
        Align::Start,
    );
}

#[cfg(test)]
mod status_tests {
    use super::*;

    fn state(model: &str, ptt: bool) -> AppViewState {
        AppViewState {
            model_name: model.into(),
            device_label: "CPU（此版本未含 GPU）".into(),
            hotkey: "Ctrl+Shift+W".into(),
            ptt_hotkey: "Ctrl+Alt+L".into(),
            ptt_mode: ptt,
            char_count: 12345,
            status_text: "就緒，按下錄音鍵或快捷鍵開始說話這是一段很長很長的狀態文字".into(),
            ..AppViewState::default()
        }
    }

    #[test]
    fn short_device_labels() {
        assert_eq!(short_device("CPU（此版本未含 GPU）"), "CPU");
        assert_eq!(short_device("CPU（GPU 初始化失敗）"), "CPU");
        assert_eq!(short_device("GPU (Vulkan)"), "Vulkan");
        assert_eq!(short_device("CPU"), "CPU");
        assert_eq!(short_device("GPU (CUDA)"), "CUDA");
        assert_eq!(short_device("GPU (Metal)"), "Metal");
    }

    #[test]
    fn status_items_fit_window_width() {
        let mut text = TextEngine::new();
        for width in [560.0_f32, 380.0, 300.0, 200.0] {
            for model in ["small", "ggml-large-v3-turbo-q5_0 (1.5 GB)"] {
                for ptt in [false, true] {
                    let s = state(model, ptt);
                    let l = status_layout(&mut text, &s, width);
                    let mut right = 0.0_f32;
                    for it in &l.items {
                        assert!(it.x >= 0.0, "{width}: x {} < 0", it.x);
                        assert!(it.x + it.w <= width + 0.01, "{width}: {it:?} overflows");
                        right = right.max(it.x + it.w);
                    }
                    assert!(right <= width + 0.01);
                    assert!(l.status_x + l.status_w <= width + 0.01);
                    if let Some(first) = l.items.first() {
                        assert!(l.status_x + l.status_w <= first.x + 0.01, "text overlaps");
                    }
                }
            }
        }
    }
}
