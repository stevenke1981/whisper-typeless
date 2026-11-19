//! 共用控制項與 v0.6 設計 tokens（`docs/ui-redesign.md` 2.2 / 2.3 / 4.2）。
//!
//! 元件：[`btn`]、[`icon_btn`]、[`toggle`]、[`segmented_fill`]、[`card`]、[`status_pill`]、
//! [`toggle_chip`]、[`keycap`]、[`group_begin`] / [`group_end`]、[`setting_row`]，以及
//! [`slider`]、[`stepper`]、[`progress`]、[`text_field`]、[`waveform`]、[`record_button`]。
//! `msg = None` 一律表示停用。

use acme_core::{Color, CursorHint, Logical, Rect};
use acme_textinput::{byte_offset_to_x, scroll_caret_into_view};

use super::Ctx;
use crate::core::UiCommand;
use crate::ui_acme::controls::slider_fraction;
use crate::ui_acme::icons::{icon, Icon};
use crate::ui_acme::msg::{FieldId, Msg, SliderKind};
use crate::ui_acme::paint::{r, Action, Item, SliderSpec, View};
use crate::ui_acme::text::{Align, TextEngine};
use crate::ui_acme::theme::{with_alpha, Theme, Tone};

/// 說明、時間戳。
pub const FONT_XS: f32 = 11.0;
/// 列標題、控制項。
pub const FONT_SM: f32 = 13.0;
/// 轉錄文字。
pub const FONT_MD: f32 = 15.0;
/// 頁標題。
pub const FONT_LG: f32 = 17.0;
/// 狀態卡標題（粗體）。
pub const FONT_XL: f32 = 20.0;
/// 控制項。
pub const RADIUS_SM: f32 = 6.0;
/// 卡片。
pub const RADIUS_MD: f32 = 10.0;
/// 狀態卡、浮動視窗。
pub const RADIUS_LG: f32 = 14.0;

/// 間距 4 / 8 / 12 / 16 / 20 / 24。內容邊距 16（`SP4`）、卡片內距 12（`SP3`）、群組間距 16。
pub const SP1: f32 = 4.0;
pub const SP2: f32 = 8.0;
pub const SP3: f32 = 12.0;
pub const SP4: f32 = 16.0;
pub const SP5: f32 = 20.0;
pub const SP6: f32 = 24.0;

/// 控制項高度。
pub const CTRL_SM: f32 = 28.0;
pub const CTRL_MD: f32 = 32.0;
pub const CTRL_LG: f32 = 44.0;

/// 設定列（無說明）高度。
pub const ROW_H: f32 = 44.0;
/// 有說明的設定列：上下內距與標題行高（列高 = 10 + 18 + 2 + 說明高 + 10）。
const HINT_ROW_PAD: f32 = 10.0;
const HINT_TITLE_H: f32 = 18.0;
/// 群組標題高度。
pub const GROUP_TITLE_H: f32 = 24.0;
/// 群組之間的距離。
pub const GROUP_GAP: f32 = 16.0;
/// 設定列右側控制區寬度（完整視窗 / 浮動面板）。
pub const CONTROL_W: f32 = 160.0;
pub const CONTROL_W_NARROW: f32 = 132.0;
/// 開關尺寸。
pub const TOGGLE_W: f32 = 44.0;
pub const TOGGLE_H: f32 = 24.0;
/// 停用控制項的透明度。
pub const DISABLED_ALPHA: f32 = 0.45;

fn x(rect: Rect<Logical>) -> f32 {
    rect.origin.x.get()
}
fn y(rect: Rect<Logical>) -> f32 {
    rect.origin.y.get()
}
fn w(rect: Rect<Logical>) -> f32 {
    rect.size.width.get()
}
fn h(rect: Rect<Logical>) -> f32 {
    rect.size.height.get()
}

/// 滑桿（自繪拖曳控制）。
#[allow(clippy::too_many_arguments)]
pub fn slider(
    v: &mut View,
    t: &Theme,
    rect: Rect<Logical>,
    kind: SliderKind,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
) {
    let knob = 16.0;
    let track_x = x(rect) + knob / 2.0;
    let track_w = (w(rect) - knob).max(1.0);
    let cy = y(rect) + h(rect) / 2.0;
    v.rect(r(track_x, cy - 2.0, track_w, 4.0), 2.0, t.bg3);
    let frac = slider_fraction(value, min, max);
    v.rect(r(track_x, cy - 2.0, track_w * frac, 4.0), 2.0, t.accent);
    let kx = track_x + track_w * frac - knob / 2.0;
    v.bordered(
        r(kx, cy - knob / 2.0, knob, knob),
        knob / 2.0,
        t.bg2,
        1.0,
        t.border_strong,
    );
    let (dx, _) = v.offset();
    v.hit(
        rect,
        0.0,
        Action::Slider(SliderSpec {
            kind,
            track_x: track_x + dx,
            track_w,
            min,
            max,
            step,
        }),
        CursorHint::Pointer,
        None,
        2,
    );
}

/// −/+ 步進器：顯示 `label`，兩側按鈕送出 `minus` / `plus`。
pub fn stepper(
    v: &mut View,
    t: &Theme,
    rect: Rect<Logical>,
    label: &str,
    minus: Option<Msg>,
    plus: Option<Msg>,
) {
    let bh = 26.0;
    let by = y(rect) + (h(rect) - bh) / 2.0;
    let left = r(x(rect), by, bh, bh);
    let right = r(x(rect) + w(rect) - bh, by, bh, bh);
    for (rect, glyph, msg) in [(left, "−", minus), (right, "+", plus)] {
        let enabled = msg.is_some();
        v.bordered(rect, RADIUS_SM, t.bg3, 1.0, t.border);
        v.text(
            rect,
            glyph,
            FONT_MD,
            if enabled { t.text } else { t.placeholder },
            Align::Center,
        );
        if let Some(msg) = msg {
            v.button(rect, RADIUS_SM, msg, t.hover);
        }
    }
    v.text(
        r(x(rect) + bh, y(rect), w(rect) - 2.0 * bh, h(rect)),
        label,
        FONT_SM,
        t.text,
        Align::Center,
    );
}

/// 進度條。
pub fn progress(v: &mut View, t: &Theme, rect: Rect<Logical>, frac: f32, color: Color) {
    let radius = h(rect) / 2.0;
    v.rect(rect, radius, t.bg3);
    let frac = if frac.is_finite() {
        frac.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if frac > 0.0 {
        v.rect(r(x(rect), y(rect), w(rect) * frac, h(rect)), radius, color);
    }
}

/// 單行文字輸入框（acme-textinput 狀態 + 自繪；含選取、輸入法組字底線與游標）。
pub fn text_field(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>, id: FieldId) {
    let t = cx.t;
    let state = cx.ui.fields.get(id);
    let focused = cx.ui.focus == Some(id);
    let readonly = state.readonly;
    v.bordered(
        rect,
        RADIUS_SM,
        if readonly { t.bg3 } else { t.bg2 },
        if focused { 1.5 } else { 1.0 },
        if focused { t.accent } else { t.border },
    );
    let pad = 8.0;
    let content_x = x(rect) + pad;
    let content_w = (w(rect) - 2.0 * pad).max(0.0);
    let style = TextEngine::style(FONT_SM, false);
    let scale = cx.scale;
    let mut st = state.clone();
    if focused {
        scroll_caret_into_view(&mut st, cx.text.fonts_mut(), &style, scale, content_w);
    } else {
        st.scroll_offset = 0.0;
    }
    let text_x = content_x - st.scroll_offset;
    // 遮蔽欄位（`password`，例如 API 金鑰）：一律畫遮蔽字元，位移換算到遮蔽後的字串。
    let masked = st.password;
    let mask_char = st.password_char;
    let shown = |s: &str| {
        if masked {
            mask_text(s, mask_char)
        } else {
            s.to_string()
        }
    };
    let map = |s: &str, b: usize| {
        if masked {
            masked_offset(s, b, mask_char)
        } else {
            b
        }
    };
    v.push_clip(r(content_x - 1.0, y(rect), content_w + 2.0, h(rect)));
    if st.text.is_empty() && st.preedit.is_empty() {
        v.text(
            r(content_x, y(rect), content_w, h(rect)),
            st.placeholder.clone(),
            FONT_SM,
            t.placeholder,
            Align::Start,
        );
    }
    let raw_cursor = st.cursor().min(st.text.len());
    let text_shown = shown(&st.text);
    let preedit_shown = shown(&st.preedit);
    let cursor = map(&st.text, raw_cursor);
    let display = format!(
        "{}{}{}",
        &text_shown[..cursor],
        preedit_shown,
        &text_shown[cursor..]
    );
    let fonts = cx.text.fonts_mut();
    if let Some((a, b)) = st.selection() {
        let xa = byte_offset_to_x(&text_shown, map(&st.text, a), fonts, &style, scale);
        let xb = byte_offset_to_x(&text_shown, map(&st.text, b), fonts, &style, scale);
        v.rect(
            r(text_x + xa, y(rect) + 5.0, xb - xa, h(rect) - 10.0),
            2.0,
            t.chip_bg_active,
        );
    }
    if !display.is_empty() {
        v.text(
            r(
                text_x,
                y(rect),
                content_w + st.scroll_offset + 400.0,
                h(rect),
            ),
            display.clone(),
            FONT_SM,
            if readonly { t.text2 } else { t.text },
            Align::Start,
        );
    }
    if !st.preedit.is_empty() {
        let p0 = byte_offset_to_x(&display, cursor, fonts, &style, scale);
        let p1 = byte_offset_to_x(&display, cursor + preedit_shown.len(), fonts, &style, scale);
        v.rect(
            r(
                text_x + p0,
                y(rect) + h(rect) - 7.0,
                (p1 - p0).max(1.0),
                1.5,
            ),
            0.0,
            t.text,
        );
    }
    if focused {
        let in_preedit = match st.preedit_cursor {
            Some((start, _)) if start <= st.preedit.len() => map(&st.preedit, start),
            _ => preedit_shown.len(),
        };
        let caret = byte_offset_to_x(&display, cursor + in_preedit, fonts, &style, scale);
        let caret_rect = r(text_x + caret, y(rect) + 6.0, 1.5, h(rect) - 12.0);
        if !readonly {
            v.rect(caret_rect, 0.0, t.accent);
        }
        let (dx, dy) = v.offset();
        v.ime_area = Some([
            caret_rect.origin.x.get() + dx,
            caret_rect.origin.y.get() + dy,
            1.5,
            h(rect) - 12.0,
        ]);
    }
    v.pop_clip();
    let (dx, _) = v.offset();
    v.hit(
        rect,
        RADIUS_SM,
        Action::Field {
            id,
            text_x: text_x + dx,
        },
        CursorHint::Text,
        None,
        2,
    );
}

/// 遮蔽顯示：每個字元換成 `mask`。
pub fn mask_text(s: &str, mask: char) -> String {
    std::iter::repeat_n(mask, s.chars().count()).collect()
}

/// 原字串的位元組位移 → 遮蔽字串中的位元組位移。
pub fn masked_offset(s: &str, byte: usize, mask: char) -> usize {
    let mut b = byte.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    s[..b].chars().count() * mask.len_utf8()
}

/// 波形條（置中，`bars` 為 0–1 音量）。
#[allow(clippy::too_many_arguments)]
pub fn waveform(
    v: &mut View,
    t: &Theme,
    rect: Rect<Logical>,
    bars: &[f32],
    count: usize,
    bar_w: f32,
    gap: f32,
    min_h: f32,
    max_h: f32,
    recording: bool,
) {
    let total = count as f32 * bar_w + (count.saturating_sub(1)) as f32 * gap;
    let x0 = x(rect) + (w(rect) - total) / 2.0;
    let cy = y(rect) + h(rect) / 2.0;
    for i in 0..count {
        let level = if recording && !bars.is_empty() {
            bars[i % bars.len()].clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bh = (level * max_h).max(min_h);
        // 錄音中的表面是紅色系：較小的音量用半透明紅，峰值用實心紅。
        let color = if recording {
            if level > 0.6 {
                t.recording
            } else {
                with_alpha(t.recording, 0.55)
            }
        } else {
            t.placeholder
        };
        v.rect(
            r(x0 + i as f32 * (bar_w + gap), cy - bh / 2.0, bar_w, bh),
            bar_w / 2.0,
            color,
        );
    }
}

/// 錄音鍵：PTT 模式按住說話（按下開始、放開停止），否則點擊切換。
pub fn record_button(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let d = w(rect);
    if s.is_recording {
        // 脈動光暈
        let phase = (cx.ui.anim % 30) as f32 / 30.0;
        let grow = 2.0 + 4.0 * (1.0 - (phase * 2.0 - 1.0).abs());
        v.rect(
            r(
                x(rect) - grow,
                y(rect) - grow,
                d + 2.0 * grow,
                d + 2.0 * grow,
            ),
            d / 2.0 + grow,
            t.recording_pulse,
        );
    }
    let idle = !s.is_recording && !s.is_processing;
    if s.is_recording {
        v.rect(rect, d / 2.0, t.recording);
    } else if s.is_processing {
        v.rect(rect, d / 2.0, t.accent);
    } else {
        // 閒置：bg2 圓盤 + 1.5px border_strong 外圈，中間紅點（與標題列的 logo 區分）。
        v.bordered(rect, d / 2.0, t.bg2, 1.5, t.border_strong);
    }
    let c = (x(rect) + d / 2.0, y(rect) + d / 2.0);
    if s.is_processing && !s.is_recording {
        for i in 0..3 {
            let lit = (cx.ui.anim / 6) % 3 == i;
            let dot = d * 0.12;
            v.rect(
                r(
                    c.0 - dot * 2.5 + i as f32 * dot * 2.0,
                    c.1 - dot / 2.0,
                    dot,
                    dot,
                ),
                dot / 2.0,
                if lit {
                    t.on_accent
                } else {
                    with_alpha(t.on_accent, 0.5)
                },
            );
        }
    } else if s.is_recording {
        let q = d * 0.32;
        v.rect(r(c.0 - q / 2.0, c.1 - q / 2.0, q, q), 3.0, t.on_accent);
    } else if idle {
        let q = d * 0.36;
        v.rect(r(c.0 - q / 2.0, c.1 - q / 2.0, q, q), q / 2.0, t.recording);
    }
    let enabled = s.is_recording || !s.is_processing;
    if enabled {
        let action = if s.ptt_mode {
            Action::Hold {
                down: Msg::RecordPress,
                up: Msg::RecordRelease,
            }
        } else {
            Action::Click(Msg::Core(UiCommand::ToggleRecording))
        };
        v.hit(rect, d / 2.0, action, CursorHint::Pointer, Some(t.hover), 2);
    }
}

// ─── v0.6 契約元件 ─────────────────────────────────────────────────────────────

/// 按鈕樣式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    /// accent 底白字（每個畫面最多一個）。
    Primary,
    /// `bg2` 底 + 1px `border_strong`。
    Secondary,
    /// 透明、`text2`，滑鼠移入才有底。
    Ghost,
    /// `danger_soft` 底 + `danger_text`。
    Danger,
}

/// 文字按鈕（可帶圖示）。`msg = None` 為停用：`bg3` 底、`placeholder` 字、無邊框；
/// Ghost 停用時維持透明底，只把字換成 `placeholder`。
#[allow(clippy::too_many_arguments)]
pub fn btn(
    v: &mut View,
    cx: &mut Ctx<'_>,
    rect: Rect<Logical>,
    label: &str,
    icon_kind: Option<Icon>,
    kind: Btn,
    msg: Option<Msg>,
) {
    let t = cx.t;
    let enabled = msg.is_some();
    let (bg, border, fg) = if !enabled {
        if kind == Btn::Ghost {
            (Color::TRANSPARENT, None, t.placeholder)
        } else {
            (t.bg3, None, t.placeholder)
        }
    } else {
        match kind {
            Btn::Primary => (t.accent, None, t.on_accent),
            Btn::Secondary => (t.bg2, Some(t.border_strong), t.text),
            Btn::Ghost => (Color::TRANSPARENT, None, t.text2),
            Btn::Danger => (t.danger_soft, None, t.danger_text),
        }
    };
    match border {
        Some(b) => {
            v.bordered(rect, RADIUS_SM, bg, 1.0, b);
        }
        None if bg.a > 0.0 => {
            v.rect(rect, RADIUS_SM, bg);
        }
        None => {}
    }
    let bold = kind == Btn::Primary && enabled;
    let icon_sz = 16.0;
    let gap = if label.is_empty() { 0.0 } else { 6.0 };
    let text_w = if label.is_empty() {
        0.0
    } else {
        cx.text.measure_width(label, FONT_SM, bold)
    };
    let content_w = text_w + icon_kind.map_or(0.0, |_| icon_sz + gap);
    let mut cx0 = x(rect) + ((w(rect) - content_w) / 2.0).max(6.0);
    if let Some(ic) = icon_kind {
        icon(
            v,
            r(cx0, y(rect) + (h(rect) - icon_sz) / 2.0, icon_sz, icon_sz),
            ic,
            fg,
        );
        cx0 += icon_sz + gap;
    }
    if !label.is_empty() {
        let avail = (x(rect) + w(rect) - 6.0 - cx0).max(0.0);
        v.styled_text(
            r(cx0, y(rect), avail, h(rect)),
            label,
            FONT_SM,
            bold,
            fg,
            Align::Start,
        );
    }
    if let Some(msg) = msg {
        v.button(rect, RADIUS_SM, msg, t.hover);
    }
}

/// 方形圖示按鈕（建議 28×28）。`active` 時有 `chip_bg_active` 底與 accent 圖示。
pub fn icon_btn(
    v: &mut View,
    t: &Theme,
    rect: Rect<Logical>,
    icon_kind: Icon,
    active: bool,
    msg: impl Into<Msg>,
) {
    let radius = RADIUS_SM;
    if active {
        v.rect(rect, radius, t.chip_bg_active);
    }
    let sz = 16.0_f32.min(w(rect)).min(h(rect));
    icon(
        v,
        r(
            x(rect) + (w(rect) - sz) / 2.0,
            y(rect) + (h(rect) - sz) / 2.0,
            sz,
            sz,
        ),
        icon_kind,
        if active { t.accent } else { t.text2 },
    );
    v.button(rect, radius, msg, t.hover);
}

/// 開關 44×24（位於 `rect` 左緣、垂直置中）。開 = accent；關 = `bg3` + 1px `border_strong`；
/// `msg = None` 停用時整體 45% 透明度。
pub fn toggle(v: &mut View, t: &Theme, rect: Rect<Logical>, on: bool, msg: Option<Msg>) {
    let a = if msg.is_some() { 1.0 } else { DISABLED_ALPHA };
    let fade = |c: Color| with_alpha(c, c.a * a);
    let track = r(
        x(rect),
        y(rect) + (h(rect) - TOGGLE_H) / 2.0,
        TOGGLE_W,
        TOGGLE_H,
    );
    let radius = TOGGLE_H / 2.0;
    if on {
        v.rect(track, radius, fade(t.accent));
    } else {
        v.bordered(track, radius, fade(t.bg3), 1.0, fade(t.border_strong));
    }
    let knob_x = if on { x(track) + 22.0 } else { x(track) + 2.0 };
    v.rect(
        r(knob_x, y(track) + 2.0, 20.0, 20.0),
        10.0,
        fade(t.on_accent),
    );
    if let Some(msg) = msg {
        v.button(track, radius, msg, t.hover);
    }
}

/// 填滿 `rect` 的分段選擇：`bg3` 軌道（圓角 8）+ 等寬選項；選取項為 `bg2` 膠囊 + 粗體 `text`。
/// `options` 為 `(id, 標籤)`。
#[allow(clippy::too_many_arguments)]
pub fn segmented_fill(
    v: &mut View,
    cx: &mut Ctx<'_>,
    rect: Rect<Logical>,
    options: &[(&str, &str)],
    selected: &str,
    enabled: bool,
    make: impl Fn(&str) -> Msg,
) {
    let t = cx.t;
    let a = if enabled { 1.0 } else { DISABLED_ALPHA };
    let fade = |c: Color| with_alpha(c, c.a * a);
    v.rect(rect, 8.0, fade(t.bg3));
    if options.is_empty() {
        return;
    }
    let inset = 2.0;
    let seg_w = (w(rect) - 2.0 * inset) / options.len() as f32;
    for (i, (id, label)) in options.iter().enumerate() {
        let active = *id == selected;
        let seg = r(
            x(rect) + inset + i as f32 * seg_w,
            y(rect) + inset,
            seg_w,
            h(rect) - 2.0 * inset,
        );
        if active {
            v.bordered(seg, 6.0, fade(t.bg2), 1.0, fade(t.border));
        }
        let shown = cx.text.elide(label, FONT_SM, active, seg_w - 4.0);
        v.styled_text(
            seg,
            shown,
            FONT_SM,
            active,
            fade(if active { t.text } else { t.text2 }),
            Align::Center,
        );
        if enabled && !active {
            v.button(seg, 6.0, make(id), t.hover);
        }
    }
}

/// 卡片：`Neutral` = `bg2` + 1px `border`；其他色調用該色的 soft 底與 line 邊框。
pub fn card(v: &mut View, t: &Theme, rect: Rect<Logical>, radius: f32, tone: Tone) {
    let (fill, line) = card_colors(t, tone);
    v.bordered(rect, radius, fill, 1.0, line);
}

fn card_colors(t: &Theme, tone: Tone) -> (Color, Color) {
    match tone {
        Tone::Neutral => (t.bg2, t.border),
        other => {
            let c = t.tone(other);
            (c.soft, c.line)
        }
    }
}

/// 狀態膠囊（高 24）：6px 圓點 + 11px 文字。有 `msg` 時可點。回傳寬度。
#[allow(clippy::too_many_arguments)]
pub fn status_pill(
    v: &mut View,
    cx: &mut Ctx<'_>,
    x0: f32,
    y0: f32,
    label: &str,
    tone: Tone,
    msg: Option<Msg>,
) -> f32 {
    let t = cx.t;
    let c = t.tone(tone);
    let height = 24.0;
    let pad = 10.0;
    let dot = 6.0;
    let text_w = cx.text.measure_width(label, FONT_XS, true);
    let width = pad + dot + 6.0 + text_w + pad;
    let rect = r(x0, y0, width, height);
    v.rect(rect, height / 2.0, c.soft);
    v.rect(
        r(x0 + pad, y0 + (height - dot) / 2.0, dot, dot),
        dot / 2.0,
        c.solid,
    );
    v.bold_text(
        r(x0 + pad + dot + 6.0, y0, text_w + 2.0, height),
        label,
        FONT_XS,
        c.fg,
        Align::Start,
    );
    if let Some(msg) = msg {
        v.button(rect, height / 2.0, msg, t.hover);
    }
    width
}

/// 切換膠囊（高 28）：開 = `chip_bg_active` + accent 粗體字與勾號；關 = `chip_bg` + `text2`。回傳寬度。
pub fn toggle_chip(
    v: &mut View,
    cx: &mut Ctx<'_>,
    x0: f32,
    y0: f32,
    label: &str,
    on: bool,
    msg: impl Into<Msg>,
) -> f32 {
    let t = cx.t;
    let height = CTRL_SM;
    let pad = 12.0;
    let check = 12.0;
    let text_w = cx.text.measure_width(label, FONT_XS, on);
    let width = pad + text_w + if on { 4.0 + check } else { 0.0 } + pad;
    let rect = r(x0, y0, width, height);
    v.rect(
        rect,
        height / 2.0,
        if on { t.chip_bg_active } else { t.chip_bg },
    );
    let fg = if on { t.accent } else { t.text2 };
    v.styled_text(
        r(x0 + pad, y0, text_w + 2.0, height),
        label,
        FONT_XS,
        on,
        fg,
        Align::Start,
    );
    if on {
        icon(
            v,
            r(
                x0 + pad + text_w + 4.0,
                y0 + (height - check) / 2.0,
                check,
                check,
            ),
            Icon::Check,
            fg,
        );
    }
    v.button(rect, height / 2.0, msg, t.hover);
    width
}

/// 鍵帽（高 22），顯示快捷鍵。回傳寬度。
pub fn keycap(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, label: &str) -> f32 {
    let t = cx.t;
    let height = 22.0;
    let text_w = cx.text.measure_width(label, FONT_XS, true);
    let width = text_w + 16.0;
    let rect = r(x0, y0, width, height);
    v.bordered(rect, 5.0, t.bg2, 1.0, t.border_strong);
    // 底部加厚一點，像實體按鍵。
    v.rect(
        r(x0 + 3.0, y0 + height - 1.5, width - 6.0, 1.5),
        0.75,
        t.border,
    );
    v.bold_text(rect, label, FONT_XS, t.text, Align::Center);
    width
}

/// 設定群組：標題 + 一張卡片（高度在 [`group_end`] 才知道）。
#[derive(Debug)]
pub struct Group {
    /// 卡片左上角與寬度（描述樹座標）。列從 `y` 開始往下排。
    pub x: f32,
    pub y: f32,
    pub w: f32,
    /// 卡片 `Item::Rect` 在 `View::items` 中的索引。
    card: usize,
}

/// 開始一個群組：畫 11px 粗體 `text2` 標題（高 24；空字串則不畫也不佔高），預留卡片。
pub fn group_begin(
    v: &mut View,
    cx: &mut Ctx<'_>,
    x0: f32,
    y0: f32,
    w0: f32,
    title: &str,
    tone: Tone,
) -> Group {
    let t = cx.t;
    let mut top = y0;
    if !title.is_empty() {
        v.bold_text(
            r(x0 + 4.0, y0, w0 - 4.0, GROUP_TITLE_H),
            title,
            FONT_XS,
            t.text2,
            Align::Start,
        );
        top += GROUP_TITLE_H;
    }
    let (fill, line) = card_colors(t, tone);
    let card = v.items.len();
    v.bordered(r(x0, top, w0, 0.0), RADIUS_MD, fill, 1.0, line);
    Group {
        x: x0,
        y: top,
        w: w0,
        card,
    }
}

/// 結束群組：把卡片高度補到 `y_end`，回傳下一個群組的 y（加上群組間距）。
pub fn group_end(v: &mut View, g: Group, y_end: f32) -> f32 {
    if let Some(Item::Rect { rect, .. }) = v.items.get_mut(g.card) {
        *rect = r(g.x, g.y, g.w, (y_end - g.y).max(0.0));
    }
    y_end + GROUP_GAP
}

/// 群組裡的一列：左側 13px 標題 + 可選的 11px `text2` 說明（自動換行）；右側控制區
/// （寬 160，群組窄於 400 時 132；高 32，與第一行垂直置中）。`divider` 在列上方畫 1px 分隔線（左縮 12）。
/// 回傳（列高, 控制區）。
#[allow(clippy::too_many_arguments)]
pub fn setting_row(
    v: &mut View,
    cx: &mut Ctx<'_>,
    g: &Group,
    y0: f32,
    title: &str,
    hint: Option<&str>,
    enabled: bool,
    divider: bool,
) -> (f32, Rect<Logical>) {
    let t = cx.t;
    let pad = SP3;
    let ctrl_w = if g.w < 400.0 {
        CONTROL_W_NARROW
    } else {
        CONTROL_W
    };
    let text_x = g.x + pad;
    let text_w = (g.w - 2.0 * pad - ctrl_w - SP3).max(40.0);
    if divider {
        v.rect(r(g.x + pad, y0, g.w - pad, 1.0), 0.0, t.border);
    }
    let title_color = if enabled { t.text } else { t.placeholder };
    let hint = hint.filter(|h| !h.is_empty());
    let Some(hint) = hint else {
        v.text(
            r(text_x, y0, text_w, ROW_H - 4.0),
            title,
            FONT_SM,
            title_color,
            Align::Start,
        );
        let control = r(
            g.x + g.w - pad - ctrl_w,
            y0 + (ROW_H - CTRL_MD) / 2.0,
            ctrl_w,
            CTRL_MD,
        );
        return (ROW_H, control);
    };
    // 有說明：上距 10 + 標題 18 + 2 + 說明 + 下距 10；控制區對齊標題那一行的中線。
    let title_top = y0 + HINT_ROW_PAD;
    v.text(
        r(text_x, title_top, text_w, HINT_TITLE_H),
        title,
        FONT_SM,
        title_color,
        Align::Start,
    );
    let hint_y = title_top + HINT_TITLE_H + 2.0;
    let hint_h = cx.text.measure_wrapped(hint, FONT_XS, text_w).max(FONT_XS);
    v.wrapped(r(text_x, hint_y, text_w, hint_h), hint, FONT_XS, t.text2);
    let height = HINT_ROW_PAD + HINT_TITLE_H + 2.0 + hint_h + HINT_ROW_PAD;
    let control = r(
        g.x + g.w - pad - ctrl_w,
        title_top + HINT_TITLE_H / 2.0 - CTRL_MD / 2.0,
        ctrl_w,
        CTRL_MD,
    );
    (height, control)
}
