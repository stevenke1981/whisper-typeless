//! 進階：語音偵測（收音靈敏度、停頓送出）、辨識（溫度、執行緒）、診斷（錯誤紀錄）。

use super::super::widgets::{
    btn, group_begin, group_end, setting_row, slider, stepper, Btn, CTRL_MD, FONT_XS, GROUP_GAP,
    SP2,
};
use super::super::Ctx;
use super::{edit, right, SILENCE_RANGE, TEMPERATURE_RANGE, THREADS_RANGE, VAD_RANGE};
use crate::ui_acme::controls::step_value;
use crate::ui_acme::msg::{Msg, SettingEdit, SliderKind};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;
use acme_core::{Logical, Rect};

/// 滑桿左側數值的寬度。
const VALUE_W: f32 = 36.0;

pub(super) fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let s = cx.s;
    let t = cx.t;

    // ── 語音偵測 ──
    let g = group_begin(v, cx, x0, y0, cw, "語音偵測", Tone::Neutral);
    let mut y = g.y;
    let (h, ctrl) = setting_row(
        v,
        cx,
        &g,
        y,
        "收音靈敏度",
        Some("數值越高，越不容易把雜音當成說話。"),
        true,
        false,
    );
    value_slider(
        v,
        cx,
        ctrl,
        SliderKind::VadThreshold,
        s.vad_threshold,
        VAD_RANGE,
    );
    y += h;
    let (h, ctrl) = setting_row(v, cx, &g, y, "停頓多久後送出", None, true, true);
    let (min, max, step) = SILENCE_RANGE;
    let st = s.silence_timeout;
    stepper(
        v,
        t,
        ctrl,
        &seconds_label(st),
        (st > min).then(|| {
            edit(SettingEdit::SilenceTimeout(step_value(
                st, -1, step, min, max,
            )))
        }),
        (st < max).then(|| {
            edit(SettingEdit::SilenceTimeout(step_value(
                st, 1, step, min, max,
            )))
        }),
    );
    y += h;
    let y = group_end(v, g, y);

    // ── 辨識 ──
    let g = group_begin(v, cx, x0, y, cw, "辨識", Tone::Neutral);
    let mut y = g.y;
    let (h, ctrl) = setting_row(
        v,
        cx,
        &g,
        y,
        "隨機性（溫度）",
        Some("0 最穩定，一般不需調整。"),
        true,
        false,
    );
    value_slider(
        v,
        cx,
        ctrl,
        SliderKind::Temperature,
        s.temperature,
        TEMPERATURE_RANGE,
    );
    y += h;
    let (h, ctrl) = setting_row(v, cx, &g, y, "CPU 執行緒", None, true, true);
    let (tmin, tmax) = THREADS_RANGE;
    let n = s.n_threads;
    stepper(
        v,
        t,
        ctrl,
        &format!("{n}"),
        (n > tmin).then(|| edit(SettingEdit::Threads(n - 1))),
        (n < tmax).then(|| edit(SettingEdit::Threads(n + 1))),
    );
    y += h;
    let y = group_end(v, g, y);

    // ── 診斷 ──
    let g = group_begin(v, cx, x0, y, cw, "診斷", Tone::Neutral);
    let mut y = g.y;
    let count = format!("共 {} 筆", s.error_log.len());
    let (h, ctrl) = setting_row(v, cx, &g, y, "錯誤紀錄", Some(&count), true, false);
    let bw = 72.0;
    btn(
        v,
        cx,
        r(right(ctrl) - bw, ctrl.origin.y.get(), bw, CTRL_MD),
        "查看",
        None,
        Btn::Secondary,
        Some(Msg::ToggleErrorLog),
    );
    y += h;
    group_end(v, g, y) - GROUP_GAP
}

/// 停頓秒數：1500 ms →「1.5 秒」。
pub(super) fn seconds_label(ms: f32) -> String {
    format!("{:.1} 秒", ms / 1000.0)
}

/// 控制區：左側數值（11px）+ 滑桿。
fn value_slider(
    v: &mut View,
    cx: &mut Ctx<'_>,
    ctrl: Rect<Logical>,
    kind: SliderKind,
    value: f32,
    (min, max, step): (f32, f32, f32),
) {
    let t = cx.t;
    let (x, y, w, h) = (
        ctrl.origin.x.get(),
        ctrl.origin.y.get(),
        ctrl.size.width.get(),
        ctrl.size.height.get(),
    );
    v.text(
        r(x, y, VALUE_W, h),
        format!("{value:.2}"),
        FONT_XS,
        t.text2,
        Align::Start,
    );
    slider(
        v,
        t,
        r(
            x + VALUE_W + SP2 / 2.0,
            y + (h - 24.0) / 2.0,
            w - VALUE_W - SP2 / 2.0,
            24.0,
        ),
        kind,
        value,
        min,
        max,
        step,
    );
}
