//! 一般：快捷鍵、視窗（系統匣）、外觀。

use super::super::widgets::{group_begin, group_end, segmented_fill, setting_row, text_field};
use super::super::Ctx;
use super::{disabled_field, edit, switch_row};
use crate::core::UiCommand;
use crate::ui_acme::msg::{FieldId, ModKey, Msg, SettingEdit};
use crate::ui_acme::paint::View;
use crate::ui_acme::theme::Tone;

/// 主題分段。
const THEMES: [(&str, &str); 2] = [("light", "淺色"), ("dark", "深色")];

pub(super) fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let s = cx.s;

    // ── 快捷鍵 ──
    let g = group_begin(v, cx, x0, y0, cw, "快捷鍵", Tone::Neutral);
    let mut y = g.y;
    let (h, ctrl) = setting_row(v, cx, &g, y, "開始／停止錄音", None, true, false);
    text_field(v, cx, ctrl, FieldId::Hotkey);
    y += h;
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "按住說話",
        Some("按住快捷鍵時錄音，放開就送出辨識"),
        s.ptt_mode,
        Some(edit(SettingEdit::PttMode(!s.ptt_mode))),
        true,
    );
    let (h, ctrl) = setting_row(v, cx, &g, y, "按住說話快捷鍵", None, s.ptt_mode, true);
    if s.ptt_mode {
        text_field(v, cx, ctrl, FieldId::PttHotkey);
    } else {
        let text = cx.ui.fields.get(FieldId::PttHotkey).text.clone();
        disabled_field(v, cx, ctrl, &text);
    }
    y += h;
    let y = group_end(v, g, y);

    // ── 視窗 ──
    let g = group_begin(v, cx, x0, y, cw, "視窗", Tone::Neutral);
    let mut y = g.y;
    for (i, (title, on, msg)) in
        window_toggles(s.close_to_tray, s.minimize_to_tray, s.start_minimized)
            .into_iter()
            .enumerate()
    {
        y += switch_row(v, cx, &g, y, title, None, on, Some(msg), i > 0);
    }
    let y = group_end(v, g, y);

    // ── 外觀 ──
    let g = group_begin(v, cx, x0, y, cw, "外觀", Tone::Neutral);
    let mut y = g.y;
    let (h, ctrl) = setting_row(v, cx, &g, y, "主題", None, true, false);
    let selected = if s.dark_mode { "dark" } else { "light" };
    segmented_fill(v, cx, ctrl, &THEMES, selected, true, |id| {
        Msg::Core(UiCommand::SetDarkMode(id == "dark"))
    });
    y += h;
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "顯示波形",
        None,
        s.mod_waveform,
        Some(edit(SettingEdit::Module(ModKey::Waveform, !s.mod_waveform))),
        true,
    );
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "顯示情境模板列",
        None,
        s.mod_context,
        Some(edit(SettingEdit::Module(ModKey::Context, !s.mod_context))),
        true,
    );
    group_end(v, g, y) - super::super::widgets::GROUP_GAP
}

/// 系統匣三個開關：（標題、目前值、按下後送出的訊息）。純函式，供測試。
pub(super) fn window_toggles(
    close_to_tray: bool,
    minimize_to_tray: bool,
    start_minimized: bool,
) -> [(&'static str, bool, Msg); 3] {
    [
        (
            "關閉時縮到系統匣",
            close_to_tray,
            edit(SettingEdit::CloseToTray(!close_to_tray)),
        ),
        (
            "最小化時縮到系統匣",
            minimize_to_tray,
            edit(SettingEdit::MinimizeToTray(!minimize_to_tray)),
        ),
        (
            "開機後直接縮到系統匣",
            start_minimized,
            edit(SettingEdit::StartMinimized(!start_minimized)),
        ),
    ]
}
