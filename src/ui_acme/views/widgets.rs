//! 共用控制項：按鈕、chip、開關、分段選擇、滑桿、步進器、進度條、文字框、波形、錄音鍵。

use acme_core::{Color, CursorHint, Logical, Rect};
use acme_textinput::{byte_offset_to_x, scroll_caret_into_view};

use super::Ctx;
use crate::core::UiCommand;
use crate::ui_acme::controls::slider_fraction;
use crate::ui_acme::msg::{FieldId, Msg, SliderKind};
use crate::ui_acme::paint::{r, Action, SliderSpec, View};
use crate::ui_acme::text::{Align, TextEngine};
use crate::ui_acme::theme::Theme;

pub const FONT_XS: f32 = 11.0;
pub const FONT_SM: f32 = 13.0;
pub const FONT_MD: f32 = 15.0;
pub const FONT_LG: f32 = 17.0;
pub const RADIUS_SM: f32 = 6.0;
pub const RADIUS_MD: f32 = 10.0;

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

/// 方形圖示按鈕。`bg` 為 `None` 時透明。
#[allow(clippy::too_many_arguments)]
pub fn icon_button(
    v: &mut View,
    t: &Theme,
    rect: Rect<Logical>,
    label: &str,
    size: f32,
    fg: Color,
    bg: Option<Color>,
    msg: impl Into<Msg>,
) {
    let radius = (h(rect) / 4.0).min(RADIUS_SM + 1.0);
    if let Some(bg) = bg {
        v.rect(rect, radius, bg);
    }
    v.text(rect, label, size, fg, Align::Center);
    v.button(rect, radius, msg, t.hover);
}

/// 文字按鈕（填色或外框）。`msg` 為 `None` 時停用。
#[allow(clippy::too_many_arguments)]
pub fn text_button(
    v: &mut View,
    t: &Theme,
    rect: Rect<Logical>,
    label: &str,
    size: f32,
    fg: Color,
    bg: Color,
    border: Option<Color>,
    msg: Option<Msg>,
) {
    match border {
        Some(b) => v.bordered(rect, RADIUS_SM, bg, 1.0, b),
        None => v.rect(rect, RADIUS_SM, bg),
    };
    v.text(rect, label, size, fg, Align::Center);
    if let Some(msg) = msg {
        v.button(rect, RADIUS_SM, msg, t.hover);
    }
}

/// 膠囊 chip，回傳寬度。
#[allow(clippy::too_many_arguments)]
pub fn chip(
    v: &mut View,
    cx: &mut Ctx<'_>,
    x0: f32,
    y0: f32,
    height: f32,
    icon: &str,
    label: &str,
    active: bool,
    accent: Color,
    msg: impl Into<Msg>,
) -> f32 {
    let t = cx.t;
    let content = if icon.is_empty() {
        label.to_string()
    } else {
        format!("{icon} {label}")
    };
    let width = cx.text.measure_width(&content, FONT_XS, active) + 18.0;
    let rect = r(x0, y0, width, height);
    v.rect(
        rect,
        height / 2.0,
        if active { t.chip_bg_active } else { t.chip_bg },
    );
    v.styled_text(
        rect,
        content,
        FONT_XS,
        active,
        if active { accent } else { t.text },
        Align::Center,
    );
    v.button(rect, height / 2.0, msg, t.hover);
    width
}

/// 開關（44×24）。
pub fn switch(v: &mut View, t: &Theme, rect: Rect<Logical>, on: bool, msg: impl Into<Msg>) {
    let track = r(x(rect), y(rect) + (h(rect) - 24.0) / 2.0, 44.0, 24.0);
    v.rect(track, 12.0, if on { t.success } else { t.bg3 });
    let knob_x = if on { x(track) + 22.0 } else { x(track) + 2.0 };
    v.rect(r(knob_x, y(track) + 2.0, 20.0, 20.0), 10.0, t.bg2);
    v.button(track, 12.0, msg, t.hover);
}

/// 分段選擇；回傳總寬度。
#[allow(clippy::too_many_arguments)]
pub fn segmented(
    v: &mut View,
    t: &Theme,
    x0: f32,
    y0: f32,
    seg_w: f32,
    seg_h: f32,
    options: &[(&str, &str)],
    selected: &str,
    enabled: bool,
    make: impl Fn(&str) -> Msg,
) -> f32 {
    let mut cx = x0;
    for (id, label) in options {
        let active = *id == selected;
        let rect = r(cx, y0, seg_w, seg_h);
        v.bordered(
            rect,
            RADIUS_SM,
            if active { t.accent } else { t.bg3 },
            1.0,
            if active { t.accent } else { t.border },
        );
        v.text(
            rect,
            *label,
            FONT_XS,
            if active { t.on_accent } else { t.text2 },
            Align::Center,
        );
        if enabled && !active {
            v.button(rect, RADIUS_SM, make(id), t.hover);
        }
        cx += seg_w + 4.0;
    }
    cx - x0 - 4.0
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

/// 區段標題（32 高），回傳高度。
pub fn section_header(v: &mut View, t: &Theme, x0: f32, y0: f32, width: f32, title: &str) -> f32 {
    v.rect(r(x0, y0 + 31.0, width, 1.0), 0.0, t.border);
    v.bold_text(
        r(x0, y0, width, 32.0),
        title,
        FONT_XS,
        t.text2,
        Align::Start,
    );
    32.0
}

/// 列標籤（垂直置中）。
pub fn row_label(v: &mut View, t: &Theme, x0: f32, y0: f32, width: f32, height: f32, label: &str) {
    v.text(
        r(x0, y0, width, height),
        label,
        FONT_SM,
        t.text,
        Align::Start,
    );
}

/// 「標籤 + 右側開關」列（40 高），回傳高度。
#[allow(clippy::too_many_arguments)]
pub fn toggle_row(
    v: &mut View,
    t: &Theme,
    x0: f32,
    y0: f32,
    width: f32,
    label: &str,
    on: bool,
    msg: impl Into<Msg>,
) -> f32 {
    row_label(v, t, x0, y0, width - 50.0, 40.0, label);
    switch(v, t, r(x0 + width - 44.0, y0, 44.0, 40.0), on, msg);
    40.0
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
    let cursor = st.cursor().min(st.text.len());
    let display = format!("{}{}{}", &st.text[..cursor], st.preedit, &st.text[cursor..]);
    let fonts = cx.text.fonts_mut();
    if let Some((a, b)) = st.selection() {
        let xa = byte_offset_to_x(&st.text, a, fonts, &style, scale);
        let xb = byte_offset_to_x(&st.text, b, fonts, &style, scale);
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
        let p1 = byte_offset_to_x(&display, cursor + st.preedit.len(), fonts, &style, scale);
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
            Some((start, _)) if start <= st.preedit.len() => start,
            _ => st.preedit.len(),
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
        let color = if recording {
            if level > 0.6 {
                t.wave_tip
            } else {
                t.accent
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
    v.rect(
        rect,
        d / 2.0,
        if s.is_recording {
            t.recording
        } else {
            t.accent
        },
    );
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
                    crate::ui_acme::theme::with_alpha(t.on_accent, 0.5)
                },
            );
        }
    } else if s.is_recording {
        let q = d * 0.32;
        v.rect(r(c.0 - q / 2.0, c.1 - q / 2.0, q, q), 3.0, t.on_accent);
    } else {
        let q = d * 0.36;
        v.rect(r(c.0 - q / 2.0, c.1 - q / 2.0, q, q), q / 2.0, t.on_accent);
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
