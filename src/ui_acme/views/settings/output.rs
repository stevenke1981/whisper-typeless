//! 輸出：中文輸出（OpenCC）、結尾換行、送出方式（自動貼上 / 剪貼簿）。

use super::super::widgets::{group_begin, group_end, segmented_fill, setting_row, GROUP_GAP};
use super::super::Ctx;
use super::{edit, switch_row, wide_control, CONVERSION_MODES};
use crate::core::{AppViewState, UiCommand};
use crate::ui_acme::msg::{ModKey, Msg, SettingEdit};
use crate::ui_acme::paint::View;
use crate::ui_acme::theme::Tone;

pub(super) fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let s = cx.s;

    // ── 文字 ──
    let g = group_begin(v, cx, x0, y0, cw, "文字", Tone::Neutral);
    let mut y = g.y;
    let (h, ctrl) = setting_row(v, cx, &g, y, "中文輸出", None, true, false);
    let ctrl = wide_control(cx, &g, ctrl, "中文輸出", 340.0);
    segmented_fill(
        v,
        cx,
        ctrl,
        &CONVERSION_MODES,
        chinese_output(s),
        true,
        |id| chinese_output_msg(s, id),
    );
    y += h;
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "結尾自動換行",
        None,
        s.append_newline,
        Some(edit(SettingEdit::AppendNewline(!s.append_newline))),
        true,
    );
    let y = group_end(v, g, y);

    // ── 送出方式 ──
    let g = group_begin(v, cx, x0, y, cw, "送出方式", Tone::Neutral);
    let mut y = g.y;
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "自動貼到游標位置",
        Some("辨識完成後直接輸入到你正在使用的視窗。"),
        s.inject_enabled,
        Some(Msg::Core(UiCommand::SetInjectEnabled(!s.inject_enabled))),
        false,
    );
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "複製到剪貼簿",
        None,
        s.clipboard_enabled,
        Some(Msg::Core(UiCommand::SetClipboardEnabled(
            !s.clipboard_enabled,
        ))),
        true,
    );
    group_end(v, g, y) - GROUP_GAP
}

/// 「中文輸出」目前的選項：關閉繁簡轉換模組時一律是「不轉換」。
pub(super) fn chinese_output(s: &AppViewState) -> &str {
    if s.mod_opencc {
        &s.conversion_mode
    } else {
        "raw"
    }
}

/// 選取「中文輸出」的某一項：轉換模式與繁簡轉換模組（`id != "raw"`）一起改。
///
/// 只需要改轉換模式時送 [`SettingEdit::ConversionMode`]（照舊經過 `apply_setting`）；
/// 模組開關也要跟著變時，一次送出同時改好兩者的設定快照。
pub(super) fn chinese_output_msg(s: &AppViewState, id: &str) -> Msg {
    let want_opencc = id != "raw";
    if s.mod_opencc == want_opencc {
        return edit(SettingEdit::ConversionMode(id.to_string()));
    }
    let mut snap = s.ui_settings();
    SettingEdit::ConversionMode(id.to_string()).apply(&mut snap);
    SettingEdit::Module(ModKey::OpenCc, want_opencc).apply(&mut snap);
    Msg::Core(UiCommand::UpdateSettings(Box::new(snap)))
}
