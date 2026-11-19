//! 模型：GPU 加速、語音模型清單（不顯示下載網址）、開啟模型資料夾。

use super::super::widgets::{
    btn, group_begin, group_end, progress, status_pill, Btn, CTRL_MD, FONT_SM, FONT_XS, GROUP_GAP,
    SP1, SP2, SP3,
};
use super::super::Ctx;
use super::{edit, note, pill_width, switch_row};
use crate::core::{ModelEntryView, UiCommand};
use crate::ui_acme::icons::Icon;
use crate::ui_acme::msg::{Msg, SettingEdit};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

/// 模型列高。
const MODEL_ROW_H: f32 = 56.0;
/// 模型列右側控制區寬。
const MODEL_CTRL_W: f32 = 104.0;

pub(super) fn content(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, cw: f32) -> f32 {
    let s = cx.s;

    // ── 效能 ──
    let backend = crate::whisper::gpu_backend_name();
    let g = group_begin(v, cx, x0, y0, cw, "效能", Tone::Neutral);
    let mut y = g.y;
    let (title, hint, on, msg) = gpu_row(backend, s.use_gpu);
    y += switch_row(v, cx, &g, y, &title, Some(hint), on, msg, false);
    let y = group_end(v, g, y);

    // ── 語音模型 ──
    let g = group_begin(v, cx, x0, y, cw, "語音模型", Tone::Neutral);
    let mut y = g.y;
    let busy = !s.downloading_id.is_empty();
    for (i, entry) in s.model_entries.iter().enumerate() {
        model_row(v, cx, g.x, y, g.w, entry, busy, i > 0);
        y += MODEL_ROW_H;
    }
    if s.model_entries.is_empty() {
        v.text(
            r(g.x + SP3, y, g.w - 2.0 * SP3, 44.0),
            "找不到模型清單",
            FONT_SM,
            cx.t.text2,
            Align::Start,
        );
        y += 44.0;
    }
    let y = group_end(v, g, y) - GROUP_GAP + SP2;
    let y = note(
        v,
        cx,
        x0 + 4.0,
        y,
        cw - 4.0,
        "建議使用 Tiny / Base / Small；較大的模型下載前會再次確認。",
    );
    // Ghost 按鈕的文字與上方說明左緣大致對齊。
    let folder = "開啟模型資料夾";
    let fw = cx.text.measure_width(folder, FONT_SM, false) + 2.0 * SP3;
    btn(
        v,
        cx,
        r(x0 - SP1, y, fw, CTRL_MD),
        folder,
        None,
        Btn::Ghost,
        Some(Msg::Core(UiCommand::OpenModelManager)),
    );
    y + CTRL_MD
}

/// GPU 列：（標題、說明、顯示的開關值、訊息）。此版本不含 GPU 時停用且顯示為關。
pub(super) fn gpu_row(
    backend: Option<&str>,
    use_gpu: bool,
) -> (String, &'static str, bool, Option<Msg>) {
    match backend {
        Some(name) => (
            format!("使用 GPU 加速（{name}）"),
            "變更在下次載入模型時生效；若當機請關閉。",
            use_gpu,
            Some(edit(SettingEdit::UseGpu(!use_gpu))),
        ),
        None => (
            "使用 GPU 加速".to_string(),
            "此版本不含 GPU；需要時請改用 Vulkan 版",
            false,
            None,
        ),
    }
}

/// 「準確度 ★★★☆☆ · 466 MB」。
pub(super) fn model_detail(entry: &ModelEntryView) -> String {
    let q = entry.quality.clamp(0, 5) as usize;
    format!(
        "準確度 {}{} · {} MB",
        "★".repeat(q),
        "☆".repeat(5 - q),
        entry.size_mb
    )
}

#[allow(clippy::too_many_arguments)]
fn model_row(
    v: &mut View,
    cx: &mut Ctx<'_>,
    gx: f32,
    y: f32,
    gw: f32,
    entry: &ModelEntryView,
    busy: bool,
    divider: bool,
) {
    let t = cx.t;
    let s = cx.s;
    let sel = entry.id == s.selected_model_id;
    let downloading = entry.id == s.downloading_id;
    if divider {
        v.rect(r(gx + SP3, y, gw - SP3, 1.0), 0.0, t.border);
    }
    let text_x = gx + SP3;
    let text_w = (gw - 3.0 * SP3 - MODEL_CTRL_W).max(40.0);
    let name = cx.text.elide(&entry.display_name, FONT_SM, true, text_w);
    v.bold_text(
        r(text_x, y + 9.0, text_w, 20.0),
        name,
        FONT_SM,
        t.text,
        Align::Start,
    );
    v.text(
        r(text_x, y + 29.0, text_w, 18.0),
        model_detail(entry),
        FONT_XS,
        t.text2,
        Align::Start,
    );

    let cx_r = gx + gw - SP3;
    let cy = y + (MODEL_ROW_H - CTRL_MD) / 2.0;
    let pick = (!busy).then(|| Msg::Core(UiCommand::ModelSelectOrDownload(entry.id.clone())));
    if downloading {
        let pct = (s.download_progress.clamp(0.0, 1.0) * 100.0).round() as i32;
        let bar_w = MODEL_CTRL_W - 40.0;
        let bx = cx_r - MODEL_CTRL_W;
        progress(
            v,
            t,
            r(bx, y + (MODEL_ROW_H - 6.0) / 2.0, bar_w, 6.0),
            s.download_progress,
            t.accent,
        );
        v.text(
            r(bx + bar_w, y, 40.0, MODEL_ROW_H),
            format!("{pct}%"),
            FONT_XS,
            t.text2,
            Align::End,
        );
    } else if sel && entry.is_downloaded {
        let pw = pill_width(cx, "使用中");
        status_pill(
            v,
            cx,
            cx_r - pw,
            y + (MODEL_ROW_H - 24.0) / 2.0,
            "使用中",
            Tone::Success,
            None,
        );
    } else if entry.is_downloaded {
        let w = 64.0;
        btn(
            v,
            cx,
            r(cx_r - w, cy, w, CTRL_MD),
            "使用",
            None,
            Btn::Secondary,
            pick,
        );
    } else {
        let w = 84.0;
        btn(
            v,
            cx,
            r(cx_r - w, cy, w, CTRL_MD),
            "下載",
            Some(Icon::Download),
            Btn::Secondary,
            pick,
        );
    }
}
