//! 辨識：轉錄引擎、OpenAI 雲端（同意 / 金鑰 / 模型）、隱私。

use super::super::widgets::{
    btn, group_begin, group_end, segmented_fill, setting_row, status_pill, text_field, Btn,
    CTRL_MD, GROUP_GAP, ROW_H, SP2, SP3,
};
use super::super::Ctx;
use super::{edit, pill_width, right, switch_row, wide_control, ENGINES, OPENAI_MODELS};
use crate::core::UiCommand;
use crate::ui_acme::msg::{FieldId, Msg, SettingEdit};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::theme::Tone;

/// 引擎列的說明。
const ENGINE_HINT: &str = "本機：語音不離開電腦。雲端：語音會傳到 OpenAI。";
const ENGINE_HINT_PRIVACY: &str = "隱私模式開啟中：一律使用本機轉錄，雲端設定暫不生效。";

pub(super) fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let s = cx.s;

    // ── 轉錄引擎 ──
    let g = group_begin(v, cx, x0, y0, cw, "轉錄引擎", Tone::Neutral);
    let mut y = g.y;
    let hint = if s.privacy_mode {
        ENGINE_HINT_PRIVACY
    } else {
        ENGINE_HINT
    };
    let (h, ctrl) = setting_row(v, cx, &g, y, "引擎", Some(hint), !s.privacy_mode, false);
    let ctrl = wide_control(cx, &g, ctrl, "引擎", 200.0);
    segmented_fill(v, cx, ctrl, &ENGINES, &s.engine, !s.privacy_mode, |id| {
        edit(SettingEdit::Engine(id.to_string()))
    });
    y += h;
    let mut y = group_end(v, g, y);

    // ── OpenAI 雲端（只在選了 OpenAI 時出現）──
    if s.engine == "openai" {
        y = cloud_group(v, cx, x0, y, cw);
    }

    // ── 隱私 ──
    let g = group_begin(v, cx, x0, y, cw, "隱私", Tone::Neutral);
    let mut y = g.y;
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "隱私模式",
        Some("一律本機轉錄、不保留錄音檔，輸出後清除剪貼簿。"),
        s.privacy_mode,
        Some(Msg::Core(UiCommand::SetPrivacyMode(!s.privacy_mode))),
        false,
    );
    group_end(v, g, y) - GROUP_GAP
}

/// 琥珀色卡片：同意 → 金鑰（狀態 + 遮蔽輸入 + 儲存 / 移除）→ 雲端模型。回傳下一個群組的 y。
fn cloud_group(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let s = cx.s;
    let g = group_begin(v, cx, x0, y0, cw, "OpenAI 雲端", Tone::Warning);
    let mut y = g.y;
    y += switch_row(
        v,
        cx,
        &g,
        y,
        "我同意把語音傳送到 OpenAI",
        Some("需要網路，費用由你的 API 帳號支付；檔案轉錄仍使用本機模型。"),
        s.openai_consent,
        Some(edit(SettingEdit::OpenAiConsent(!s.openai_consent))),
        false,
    );

    // 金鑰：只顯示「已設定 / 未設定」，永不顯示內容。
    let (h, ctrl) = setting_row(
        v,
        cx,
        &g,
        y,
        "API 金鑰",
        Some("以 Windows 加密保存在本機，不寫入設定檔。"),
        true,
        true,
    );
    let (label, tone) = if s.openai_key_set {
        ("已設定", Tone::Success)
    } else {
        ("未設定", Tone::Danger)
    };
    let pw = pill_width(cx, label);
    status_pill(
        v,
        cx,
        right(ctrl) - pw,
        ctrl.origin.y.get() + (CTRL_MD - 24.0) / 2.0,
        label,
        tone,
        None,
    );
    y += h;
    // 下一列：遮蔽文字框 + 儲存（+ 已設定時的移除金鑰）
    let row_y = y + (ROW_H - CTRL_MD) / 2.0 - SP2;
    let inner_x = g.x + SP3;
    let inner_r = g.x + g.w - SP3;
    let save_w = 64.0;
    let remove_w = if s.openai_key_set { 84.0 } else { 0.0 };
    let gaps = if s.openai_key_set { 2.0 * SP2 } else { SP2 };
    let field_w = (inner_r - inner_x - save_w - remove_w - gaps).max(60.0);
    text_field(
        v,
        cx,
        r(inner_x, row_y, field_w, CTRL_MD),
        FieldId::OpenAiKey,
    );
    let has_input = !cx.ui.fields.get(FieldId::OpenAiKey).text.trim().is_empty();
    btn(
        v,
        cx,
        r(inner_x + field_w + SP2, row_y, save_w, CTRL_MD),
        "儲存",
        None,
        Btn::Primary,
        has_input.then_some(Msg::OpenAiKeySave),
    );
    if s.openai_key_set {
        btn(
            v,
            cx,
            r(inner_r - remove_w, row_y, remove_w, CTRL_MD),
            "移除金鑰",
            None,
            Btn::Ghost,
            Some(Msg::Core(UiCommand::ClearOpenAiKey)),
        );
    }
    y += ROW_H - SP2;

    let (h, ctrl) = setting_row(v, cx, &g, y, "雲端模型", None, true, true);
    let ctrl = wide_control(cx, &g, ctrl, "雲端模型", 320.0);
    segmented_fill(v, cx, ctrl, &OPENAI_MODELS, &s.openai_model, true, |id| {
        edit(SettingEdit::OpenAiModel(id.to_string()))
    });
    y += h;
    group_end(v, g, y)
}
