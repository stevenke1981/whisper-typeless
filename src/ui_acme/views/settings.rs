//! 設定面板（兩種模式共用，覆蓋整個視窗）。

use acme_core::{Logical, Rect};

use super::live::scrollbar;
use super::widgets::{
    progress, row_label, section_header, segmented, slider, stepper, text_button, text_field,
    toggle_row, FONT_LG, FONT_SM, FONT_XS, RADIUS_SM,
};
use super::Ctx;
use crate::core::{AppViewState, UiCommand};
use crate::settings::UiSettings;
use crate::ui_acme::controls::step_value;
use crate::ui_acme::msg::{FieldId, ModKey, Msg, ScrollId, SettingEdit, SliderKind};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;

/// 繁簡轉換模式（與 Slint 設定面板相同）。
pub const CONVERSION_MODES: [(&str, &str); 4] = [
    ("zh-TW", "繁（台）"),
    ("zh-HK", "繁（港）"),
    ("zh-CN", "簡體"),
    ("raw", "原文"),
];

pub const VAD_RANGE: (f32, f32, f32) = (0.1, 0.9, 0.05);
pub const TEMPERATURE_RANGE: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const SILENCE_RANGE: (f32, f32, f32) = (300.0, 5000.0, 100.0);
pub const THREADS_RANGE: (i32, i32) = (1, 32);

const TITLE_H: f32 = 52.0;

pub fn panel(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let w = rect.size.width.get();
    let h = rect.size.height.get();
    v.rect(rect, 0.0, t.bg);
    v.hit(
        rect,
        0.0,
        crate::ui_acme::paint::Action::Block,
        acme_core::CursorHint::Default,
        None,
        0,
    );
    // 標題列（可拖曳）
    v.rect(r(0.0, 0.0, w, TITLE_H), 0.0, t.bg2);
    v.rect(r(0.0, TITLE_H - 1.0, w, 1.0), 0.0, t.border);
    v.hit(
        r(0.0, 0.0, w, TITLE_H),
        0.0,
        crate::ui_acme::paint::Action::DragMove,
        acme_core::CursorHint::Default,
        None,
        1,
    );
    v.bold_text(
        r(20.0, 0.0, 200.0, TITLE_H),
        "⚙  設定",
        FONT_LG,
        t.text,
        Align::Start,
    );
    let close = r(w - 16.0 - 32.0, 10.0, 32.0, 32.0);
    v.bordered(close, RADIUS_SM, t.bg3, 1.0, t.border);
    v.text(close, "✕", 14.0, t.text2, Align::Center);
    v.button(close, RADIUS_SM, UiCommand::SetSettingsOpen(false), t.hover);

    let body = r(0.0, TITLE_H, w, (h - TITLE_H).max(0.0));
    let x0 = 20.0;
    let cw = (w - 40.0).max(100.0);
    // 第一輪量高度（丟到暫存描述樹），第二輪才真的畫並套用捲動位移。
    let content_h = {
        let mut scratch = View::new();
        content(&mut scratch, cx, x0, TITLE_H, cw) - TITLE_H
    };
    let max = (content_h - body.size.height.get()).max(0.0);
    let offset = cx.ui.scroll(ScrollId::Settings).min(max);
    v.scroll_area(body, ScrollId::Settings, max);
    v.push_clip(body);
    v.push_offset(0.0, -offset);
    content(v, cx, x0, TITLE_H, cw);
    v.pop_offset();
    v.pop_clip();
    scrollbar(v, cx, body, offset, max, content_h);
}

// 以下三個區段由後續工作包各自填入；目前不畫任何東西，回傳佔用的高度 0。
// 簽名一致：(描述樹, 情境, 左緣 x, 起始 y, 內容寬) → 區段高度。

/// 轉錄引擎選項（`[transcription] engine`）。
pub const ENGINES: [(&str, &str); 2] = [("local", "本機"), ("openai", "OpenAI 雲端")];
/// OpenAI 轉錄模型。
pub const OPENAI_MODELS: [(&str, &str); 2] = [
    ("gpt-live-transcribe", "gpt-live-transcribe"),
    ("gpt-transcribe", "gpt-transcribe"),
];

/// 自動換行的說明文字，回傳佔用高度（含下方間距）。
fn note(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y: f32, cw: f32, text: &str, danger: bool) -> f32 {
    let h = cx.text.measure_wrapped(text, FONT_XS, cw);
    let color = if danger { cx.t.danger } else { cx.t.text2 };
    v.wrapped(r(x0, y, cw, h), text, FONT_XS, color);
    h + 8.0
}

/// 雲端轉錄：引擎選擇、OpenAI 模型、同意說明、金鑰輸入（WP-A）。
fn cloud_section(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let t = cx.t;
    let s = cx.s;
    let mut y = y0;
    y += section_header(v, t, x0, y, cw, "雲端轉錄");
    if s.privacy_mode {
        y += note(
            v,
            cx,
            x0,
            y,
            cw,
            "隱私模式啟用中：雲端轉錄已停用，一律使用本機引擎。",
            false,
        );
        return y - y0;
    }

    row_label(v, t, x0, y, cw, 36.0, "轉錄引擎");
    y += 36.0;
    let seg_w = ((cw - 4.0) / 2.0).min(120.0);
    segmented(v, t, x0, y, seg_w, 28.0, &ENGINES, &s.engine, true, |id| {
        edit(SettingEdit::Engine(id.to_string()))
    });
    y += 40.0;
    if s.engine != "openai" {
        return y - y0;
    }

    y += toggle_row(
        v,
        t,
        x0,
        y,
        cw,
        "同意將語音傳送到 OpenAI",
        s.openai_consent,
        edit(SettingEdit::OpenAiConsent(!s.openai_consent)),
    );
    let warning = if s.openai_consent {
        "⚠ 錄音會即時傳送到 OpenAI 伺服器轉錄（需要網路，費用由你的 API 帳號支付）。檔案模式仍使用本機模型。"
    } else {
        "⚠ 勾選同意後，錄音才會傳送到 OpenAI 伺服器轉錄；未同意時一律使用本機引擎。"
    };
    y += note(v, cx, x0, y, cw, warning, !s.openai_consent);

    // 金鑰：只顯示「已設定 / 未設定」，永不顯示內容
    row_label(v, t, x0, y, cw - 80.0, 36.0, "API 金鑰");
    let (label, color) = if s.openai_key_set {
        ("✓ 已設定", t.success)
    } else {
        ("未設定", t.text2)
    };
    v.text(
        r(x0 + cw - 80.0, y, 80.0, 36.0),
        label,
        FONT_XS,
        color,
        Align::End,
    );
    y += 36.0;
    let btn_w = 56.0;
    let field_w = (cw - 2.0 * btn_w - 12.0).max(60.0);
    text_field(v, cx, r(x0, y, field_w, 30.0), FieldId::OpenAiKey);
    text_button(
        v,
        t,
        r(x0 + field_w + 6.0, y, btn_w, 30.0),
        "儲存",
        FONT_XS,
        t.on_accent,
        t.accent,
        None,
        Some(Msg::OpenAiKeySave),
    );
    text_button(
        v,
        t,
        r(x0 + field_w + 12.0 + btn_w, y, btn_w, 30.0),
        "清除",
        FONT_XS,
        t.text2,
        t.bg3,
        Some(t.border),
        s.openai_key_set
            .then_some(Msg::Core(UiCommand::ClearOpenAiKey)),
    );
    y += 38.0;
    y += note(
        v,
        cx,
        x0,
        y,
        cw,
        "金鑰以 Windows DPAPI 加密存在本機設定資料夾，不寫入設定檔；也可改用環境變數 OPENAI_API_KEY。",
        false,
    );

    row_label(v, t, x0, y, cw, 36.0, "OpenAI 模型");
    y += 36.0;
    let seg_w = ((cw - 4.0) / 2.0).min(150.0);
    segmented(
        v,
        t,
        x0,
        y,
        seg_w,
        28.0,
        &OPENAI_MODELS,
        &s.openai_model,
        true,
        |id| edit(SettingEdit::OpenAiModel(id.to_string())),
    );
    y += 40.0;
    y - y0
}

/// 系統匣：關閉 / 最小化到系統匣、啟動時最小化（WP-B）。
fn tray_section(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y: f32, cw: f32) -> f32 {
    let t = cx.t;
    let s = cx.s;
    let start = y;
    let mut y = y;
    y += section_header(v, t, x0, y, cw, "系統匣");
    for (label, on, edit) in tray_toggles(s) {
        y += toggle_row(v, t, x0, y, cw, label, on, edit);
    }
    y - start
}

/// 系統匣三個開關：（文字、目前值、按下後送出的指令）。純函式，供測試。
fn tray_toggles(s: &AppViewState) -> [(&'static str, bool, UiCommand); 3] {
    let flip = |f: fn(&mut UiSettings)| {
        let mut snap = s.ui_settings();
        f(&mut snap);
        UiCommand::UpdateSettings(Box::new(snap))
    };
    [
        (
            "關閉視窗時縮小到系統匣",
            s.close_to_tray,
            flip(|u| u.close_to_tray = !u.close_to_tray),
        ),
        (
            "最小化時縮小到系統匣",
            s.minimize_to_tray,
            flip(|u| u.minimize_to_tray = !u.minimize_to_tray),
        ),
        (
            "啟動時縮小到系統匣",
            s.start_minimized,
            flip(|u| u.start_minimized = !u.start_minimized),
        ),
    ]
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // 緊鄰 tray_toggles，避免與其他工作包在檔尾衝突
mod tray_toggle_tests {
    use super::*;

    #[test]
    fn tray_toggles_flip_only_their_own_flag() {
        let s = AppViewState::default();
        let toggles = tray_toggles(&s);
        let base = s.ui_settings();
        let snaps: Vec<UiSettings> = toggles
            .iter()
            .map(|(_, _, c)| match c {
                UiCommand::UpdateSettings(u) => (**u).clone(),
                other => panic!("非設定指令: {other:?}"),
            })
            .collect();
        assert_eq!(toggles[0].1, base.close_to_tray);
        assert_eq!(snaps[0].close_to_tray, !base.close_to_tray);
        assert_eq!(snaps[1].minimize_to_tray, !base.minimize_to_tray);
        assert_eq!(snaps[2].start_minimized, !base.start_minimized);
        assert_eq!(snaps[0].minimize_to_tray, base.minimize_to_tray);
        assert!(toggles.iter().all(|(label, _, _)| label.contains("系統匣")));
    }
}

/// GPU 後端 / 模型建議（WP-C）。
fn gpu_model_section(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y: f32, cw: f32) -> f32 {
    let t = cx.t;
    let s = cx.s;
    let backend = crate::whisper::gpu_backend_name();
    let mut h = section_header(v, t, x0, y, cw, "GPU 與模型");
    let label = match backend {
        Some(name) => format!("使用 GPU（{name}）"),
        None => "使用 GPU".to_string(),
    };
    h += toggle_row(
        v,
        t,
        x0,
        y + h,
        cw,
        &label,
        s.use_gpu,
        edit(SettingEdit::UseGpu(!s.use_gpu)),
    );
    let note = match backend {
        Some(_) => "變更會在下次載入模型時生效。若當機，請關閉此選項改用 CPU。",
        None => "此版本未含 GPU 支援，仍以 CPU 辨識；需要 GPU 請使用 Vulkan 版。",
    };
    v.text(r(x0, y + h, cw, 32.0), note, FONT_XS, t.text2, Align::Start);
    h += 32.0;
    v.text(
        r(x0, y + h, cw, 32.0),
        "建議使用 Tiny / Base / Small 小型模型；較大的模型下載前會再次確認。",
        FONT_XS,
        t.text2,
        Align::Start,
    );
    h + 32.0
}

fn edit(e: SettingEdit) -> Msg {
    Msg::Setting(e)
}

/// 畫出全部設定列，回傳結束的 y。
fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let t = cx.t;
    let s = cx.s;
    let mut y = y0 + 12.0;
    let ctrl_w = 150.0_f32.min(cw * 0.5);
    let ctrl_x = x0 + cw - ctrl_w;

    // ── 音訊 ──
    y += section_header(v, t, x0, y, cw, "音訊");
    row_label(v, t, x0, y, cw - ctrl_w - 50.0, 48.0, "VAD 閾值");
    v.text(
        r(ctrl_x - 48.0, y, 44.0, 48.0),
        format!("{:.2}", s.vad_threshold),
        FONT_XS,
        t.text2,
        Align::End,
    );
    let (min, max, step) = VAD_RANGE;
    slider(
        v,
        t,
        r(ctrl_x, y + 12.0, ctrl_w, 24.0),
        SliderKind::VadThreshold,
        s.vad_threshold,
        min,
        max,
        step,
    );
    y += 48.0;
    row_label(v, t, x0, y, cw - ctrl_w, 44.0, "靜音逾時");
    let (min, max, step) = SILENCE_RANGE;
    let st = s.silence_timeout;
    stepper(
        v,
        t,
        r(ctrl_x, y, ctrl_w, 44.0),
        &format!("{st:.0} ms"),
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
    y += 44.0;

    // ── Whisper 引擎 ──
    y += section_header(v, t, x0, y, cw, "Whisper 引擎");
    row_label(v, t, x0, y, cw - ctrl_w - 50.0, 48.0, "溫度");
    v.text(
        r(ctrl_x - 48.0, y, 44.0, 48.0),
        format!("{:.2}", s.temperature),
        FONT_XS,
        t.text2,
        Align::End,
    );
    let (min, max, step) = TEMPERATURE_RANGE;
    slider(
        v,
        t,
        r(ctrl_x, y + 12.0, ctrl_w, 24.0),
        SliderKind::Temperature,
        s.temperature,
        min,
        max,
        step,
    );
    y += 48.0;
    row_label(v, t, x0, y, cw - ctrl_w, 44.0, "執行緒");
    let (tmin, tmax) = THREADS_RANGE;
    let n = s.n_threads;
    stepper(
        v,
        t,
        r(ctrl_x, y, ctrl_w, 44.0),
        &format!("{n}"),
        (n > tmin).then(|| edit(SettingEdit::Threads(n - 1))),
        (n < tmax).then(|| edit(SettingEdit::Threads(n + 1))),
    );
    y += 44.0;

    // ── 快捷鍵 ──
    y += section_header(v, t, x0, y, cw, "快捷鍵");
    row_label(v, t, x0, y, cw - ctrl_w, 44.0, "錄音快捷鍵");
    text_field(v, cx, r(ctrl_x, y + 8.0, ctrl_w, 28.0), FieldId::Hotkey);
    y += 44.0;
    y += toggle_row(
        v,
        t,
        x0,
        y,
        cw,
        "按住錄音 (PTT)",
        s.ptt_mode,
        edit(SettingEdit::PttMode(!s.ptt_mode)),
    );
    row_label(v, t, x0, y, cw - ctrl_w, 44.0, "PTT 快捷鍵");
    text_field(v, cx, r(ctrl_x, y + 8.0, ctrl_w, 28.0), FieldId::PttHotkey);
    y += 44.0;

    // ── 中文處理 ──
    y += section_header(v, t, x0, y, cw, "中文處理");
    row_label(v, t, x0, y, cw, 40.0, "繁簡轉換");
    y += 40.0;
    let seg_w = ((cw - 12.0) / 4.0).min(72.0);
    segmented(
        v,
        t,
        x0,
        y,
        seg_w,
        28.0,
        &CONVERSION_MODES,
        &s.conversion_mode,
        true,
        |id| edit(SettingEdit::ConversionMode(id.to_string())),
    );
    y += 40.0;

    // ── 雲端轉錄（WP-A）──
    y += cloud_section(v, cx, x0, y, cw);

    // ── 隱私 ──
    y += section_header(v, t, x0, y, cw, "隱私");
    y += toggle_row(
        v,
        t,
        x0,
        y,
        cw,
        "隱私模式",
        s.privacy_mode,
        UiCommand::SetPrivacyMode(!s.privacy_mode),
    );
    let note = "啟用後不保留錄音檔，並於輸出後清除剪貼簿";
    let note_h = cx.text.measure_wrapped(note, FONT_XS, cw);
    v.wrapped(r(x0, y, cw, note_h), note, FONT_XS, t.text2);
    y += note_h + 8.0;

    // ── 外觀 ──
    y += section_header(v, t, x0, y, cw, "外觀");
    y += toggle_row(
        v,
        t,
        x0,
        y,
        cw,
        "深色模式",
        s.dark_mode,
        UiCommand::SetDarkMode(!s.dark_mode),
    );

    // ── 系統匣（WP-B）──
    y += tray_section(v, cx, x0, y, cw);

    // ── 功能模組 ──
    y += section_header(v, t, x0, y, cw, "功能模組");
    let mods = [
        ("靜音偵測", ModKey::Vad, s.mod_vad),
        ("情境模板", ModKey::Context, s.mod_context),
        ("繁簡轉換", ModKey::OpenCc, s.mod_opencc),
        ("自動注入", ModKey::Inject, s.mod_inject),
        ("波形顯示", ModKey::Waveform, s.mod_waveform),
        ("歷史記錄", ModKey::History, s.mod_history),
    ];
    for (label, key, on) in mods {
        y += toggle_row(
            v,
            t,
            x0,
            y,
            cw,
            label,
            on,
            edit(SettingEdit::Module(key, !on)),
        );
    }

    // ── 儲存 ──
    y += 16.0;
    let save = r(x0 + (cw - 120.0) / 2.0, y, 120.0, 32.0);
    text_button(
        v,
        t,
        save,
        "💾  儲存設定",
        FONT_SM,
        t.on_accent,
        t.accent,
        None,
        Some(Msg::Core(UiCommand::SaveSettings)),
    );
    if !s.save_status.is_empty() {
        let ok = s.save_status == "✓ 已儲存";
        v.text(
            r(save.origin.x.get() + 130.0, y, 120.0, 32.0),
            s.save_status.clone(),
            FONT_SM,
            if ok { t.success } else { t.danger },
            Align::Start,
        );
    }
    y += 48.0;

    // ── 情境模板 ──
    y += section_header(v, t, x0, y, cw, "情境模板");
    let editing = cx.ui.template_edit.clone();
    for (i, entry) in s.template_entries.iter().enumerate() {
        let sel = editing.as_ref().and_then(|e| e.index) == Some(i);
        let row = r(x0, y + 2.0, cw, 34.0);
        if sel {
            v.bordered(row, RADIUS_SM, t.accent_soft, 1.0, t.accent_line);
        }
        v.styled_text(
            r(x0 + 8.0, y, cw - 60.0, 38.0),
            entry.name.clone(),
            FONT_SM,
            sel,
            if sel { t.accent } else { t.text },
            Align::Start,
        );
        if entry.is_builtin {
            v.text(
                r(x0 + cw - 48.0, y, 40.0, 38.0),
                "內建",
                9.0,
                t.text2,
                Align::End,
            );
        }
        v.rect(r(x0, y + 37.0, cw, 1.0), 0.0, t.border);
        v.button(row, RADIUS_SM, Msg::TemplateEditSelect(i), t.hover);
        y += 38.0;
    }
    y += 4.0;
    text_button(
        v,
        t,
        r(x0, y, cw, 34.0),
        "+ 新增模板",
        FONT_XS,
        t.text2,
        t.bg2,
        Some(t.border),
        Some(Msg::TemplateEditNew),
    );
    y += 34.0;
    if let Some(edit) = &editing {
        y += 12.0;
        v.text(
            r(x0, y, cw, 16.0),
            "模板名稱",
            FONT_XS,
            t.text2,
            Align::Start,
        );
        y += 22.0;
        text_field(v, cx, r(x0, y, cw, 30.0), FieldId::TemplateName);
        y += 36.0;
        v.text(r(x0, y, cw, 16.0), "提示詞", FONT_XS, t.text2, Align::Start);
        y += 22.0;
        text_field(v, cx, r(x0, y, cw, 30.0), FieldId::TemplatePrompt);
        y += 36.0;
        text_button(
            v,
            t,
            r(x0, y, 72.0, 28.0),
            "✓ 儲存",
            FONT_XS,
            t.on_accent,
            t.accent,
            None,
            Some(Msg::TemplateEditSave),
        );
        if !edit.builtin {
            text_button(
                v,
                t,
                r(x0 + 78.0, y, 64.0, 28.0),
                "🗑 刪除",
                FONT_XS,
                t.danger,
                t.danger_soft,
                Some(t.danger_line),
                Some(Msg::TemplateEditDelete),
            );
        }
        y += 36.0;
    }

    // ── GPU / 模型（WP-C）──
    y += gpu_model_section(v, cx, x0, y, cw);

    // ── 模型 ──
    y += 8.0;
    y += section_header(v, t, x0, y, cw, "模型");
    let busy = !s.downloading_id.is_empty();
    for entry in &s.model_entries {
        let sel = entry.id == s.selected_model_id;
        let dl = entry.id == s.downloading_id;
        if sel {
            v.bordered(
                r(x0, y + 2.0, cw, 64.0),
                RADIUS_SM,
                t.accent_soft,
                1.0,
                t.accent_line,
            );
        }
        let text_w = cw - 72.0 - 20.0;
        v.styled_text(
            r(x0 + 6.0, y + 8.0, text_w, 18.0),
            entry.display_name.clone(),
            FONT_SM,
            sel,
            if sel { t.accent } else { t.text },
            Align::Start,
        );
        v.text(
            r(x0 + 6.0, y + 27.0, text_w, 16.0),
            format!("{}★ · {} MB", entry.quality, entry.size_mb),
            FONT_XS,
            t.text2,
            Align::Start,
        );
        let url = cx.text.elide(&entry.download_url, 10.0, false, text_w);
        v.text(
            r(x0 + 6.0, y + 44.0, text_w, 14.0),
            url,
            10.0,
            t.text2,
            Align::Start,
        );
        let bx = x0 + cw - 6.0 - 72.0;
        if dl {
            progress(
                v,
                t,
                r(bx, y + 26.0, 72.0, 4.0),
                s.download_progress,
                t.accent,
            );
            v.text(
                r(bx, y + 34.0, 72.0, 16.0),
                format!("{:.0}%", (s.download_progress * 100.0).clamp(0.0, 100.0)),
                FONT_XS,
                t.text2,
                Align::Center,
            );
        } else {
            let (label, fg, bg, border) = if sel {
                ("✓ 使用中", t.on_accent, t.accent, t.accent)
            } else if entry.is_downloaded {
                ("✓ 選擇", t.success, t.success_soft, t.success_line)
            } else {
                ("↓ 下載", t.text2, t.bg3, t.border)
            };
            text_button(
                v,
                t,
                r(bx, y + 20.0, 72.0, 28.0),
                label,
                FONT_XS,
                fg,
                bg,
                Some(border),
                (!busy).then(|| Msg::Core(UiCommand::ModelSelectOrDownload(entry.id.clone()))),
            );
        }
        v.rect(r(x0, y + 67.0, cw, 1.0), 0.0, t.border);
        y += 68.0;
    }
    y += 8.0;
    text_button(
        v,
        t,
        r(x0, y, cw, 30.0),
        "📂 開啟模型資料夾",
        FONT_XS,
        t.text2,
        t.bg2,
        Some(t.border),
        Some(Msg::Core(UiCommand::OpenModelManager)),
    );
    y += 30.0 + 16.0;
    y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::AppViewState;
    use crate::ui_acme::controls::set_text;
    use crate::ui_acme::local::UiLocal;
    use crate::ui_acme::paint::{Action, Item};
    use crate::ui_acme::text::TextEngine;
    use crate::ui_acme::theme::Theme;

    fn cloud(s: &AppViewState, ui: &UiLocal) -> (View, f32) {
        let mut text = TextEngine::new();
        let mut cx = Ctx {
            s,
            ui,
            t: Theme::for_mode(true),
            text: &mut text,
            scale: 1.0,
        };
        let mut v = View::new();
        let h = cloud_section(&mut v, &mut cx, 20.0, 0.0, 400.0);
        (v, h)
    }

    fn texts(v: &View) -> Vec<String> {
        v.items
            .iter()
            .filter_map(|i| match i {
                Item::Text { content, .. } | Item::Wrapped { content, .. } => Some(content.clone()),
                _ => None,
            })
            .collect()
    }

    fn actions(v: &View) -> Vec<Action> {
        v.items
            .iter()
            .filter_map(|i| match i {
                Item::Hit { action, .. } => Some(action.clone()),
                _ => None,
            })
            .collect()
    }

    fn openai_state() -> AppViewState {
        AppViewState {
            engine: "openai".into(),
            ..AppViewState::default()
        }
    }

    #[test]
    fn privacy_mode_disables_cloud_controls() {
        let s = AppViewState {
            privacy_mode: true,
            ..openai_state()
        };
        let (v, h) = cloud(&s, &UiLocal::default());
        assert!(h > 0.0);
        assert!(actions(&v).is_empty());
        assert!(texts(&v).iter().any(|t| t.contains("隱私模式")));
    }

    #[test]
    fn local_engine_shows_only_engine_choice() {
        let (v, _) = cloud(&AppViewState::default(), &UiLocal::default());
        let acts = actions(&v);
        assert!(
            acts.contains(&Action::Click(Msg::Setting(SettingEdit::Engine(
                "openai".into()
            ))))
        );
        assert!(!acts.iter().any(|a| matches!(
            a,
            Action::Field {
                id: FieldId::OpenAiKey,
                ..
            }
        )));
    }

    #[test]
    fn openai_engine_shows_consent_key_and_model() {
        let mut ui = UiLocal::default();
        set_text(&mut ui.fields.openai_key, "sk-secret-xyz");
        let (v, _) = cloud(&openai_state(), &ui);
        let acts = actions(&v);
        assert!(
            acts.contains(&Action::Click(Msg::Setting(SettingEdit::OpenAiConsent(
                true
            ))))
        );
        assert!(acts.contains(&Action::Click(Msg::OpenAiKeySave)));
        assert!(
            acts.contains(&Action::Click(Msg::Setting(SettingEdit::OpenAiModel(
                "gpt-transcribe".into()
            ))))
        );
        // 未設定金鑰時「清除」停用
        assert!(!acts.contains(&Action::Click(Msg::Core(UiCommand::ClearOpenAiKey))));
        let all = texts(&v);
        assert!(all.iter().any(|t| t == "未設定"));
        assert!(all.iter().any(|t| t.contains("OpenAI 伺服器")));
        // 金鑰欄位只畫遮蔽字元
        assert!(!all.iter().any(|t| t.contains("sk-secret")));
        assert!(all.iter().any(|t| t == &"•".repeat("sk-secret-xyz".len())));
    }

    #[test]
    fn key_set_enables_clear() {
        let s = AppViewState {
            openai_key_set: true,
            openai_consent: true,
            ..openai_state()
        };
        let (v, _) = cloud(&s, &UiLocal::default());
        assert!(actions(&v).contains(&Action::Click(Msg::Core(UiCommand::ClearOpenAiKey))));
        assert!(texts(&v).iter().any(|t| t == "✓ 已設定"));
    }
}
