//! 設定面板（兩種模式共用，覆蓋整個視窗）。

use acme_core::{Logical, Rect};

use super::live::scrollbar;
use super::widgets::{
    progress, row_label, section_header, segmented, slider, stepper, text_button, text_field,
    toggle_row, FONT_LG, FONT_SM, FONT_XS, RADIUS_SM,
};
use super::Ctx;
use crate::core::UiCommand;
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
    y += toggle_row(
        v,
        t,
        x0,
        y,
        cw,
        "使用 GPU",
        s.use_gpu,
        edit(SettingEdit::UseGpu(!s.use_gpu)),
    );
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
