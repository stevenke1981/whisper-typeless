//! 即時模式：浮動緊湊列與展開面板、完整視窗（標題列、狀態卡、模板列、紀錄、頁尾）。
//!
//! 版面與文案依 `docs/ui-redesign.md` 2.4 / 2.5。熱鍵 / PTT 路徑不在這裡做任何視窗動作。

use acme_core::{Logical, Rect};

use super::widgets::{
    btn, card, icon_btn, keycap, progress, record_button, segmented_fill, status_pill, toggle_chip,
    waveform, Btn, CTRL_SM, FONT_MD, FONT_SM, FONT_XL, FONT_XS, RADIUS_LG, RADIUS_SM, SP2, SP4,
};
use super::{cloud_ready, file, onboarding, phase, Ctx, Phase, BAR_H, TEMPLATES};
use crate::core::{AppViewState, UiCommand};
use crate::ui_acme::icons::{icon, Icon};
use crate::ui_acme::msg::{Msg, ScrollId, SettingsTab};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

/// 完整視窗標題列高。
const TITLE_H: f32 = 44.0;
/// 頁尾高。
const FOOTER_H: f32 = 48.0;
/// 轉錄清單裡時間欄的寬度。
const TIME_W: f32 = 48.0;

fn rx(rect: Rect<Logical>) -> (f32, f32, f32, f32) {
    (
        rect.origin.x.get(),
        rect.origin.y.get(),
        rect.size.width.get(),
        rect.size.height.get(),
    )
}

// ─── 狀態文字 ────────────────────────────────────────────────────────────────

/// 核心的狀態文字看起來像錯誤（要用警示色顯示）。
pub(super) fn status_is_error(text: &str) -> bool {
    const KEYS: [&str; 8] = [
        "錯誤",
        "失敗",
        "問題",
        "無法",
        "逾時",
        "不存在",
        "\u{26a0}",
        "壞",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

/// 去掉核心狀態文字前面的單色符號（● / 警告符號）。
fn clean_status(text: &str) -> &str {
    text.trim_start_matches(['\u{25cf}', '\u{26a0}', ' '])
}

/// 例行狀態（就緒 / 錄音中 / 辨識中 / 下載中）以外、值得在狀態卡顯示的訊息。
pub(super) fn status_note(s: &AppViewState) -> Option<&str> {
    let text = clean_status(&s.status_text);
    const ROUTINE: [&str; 5] = ["就緒", "錄音中", "辨識中", "下載模型", "等待"];
    if text.is_empty() || ROUTINE.iter().any(|p| text.starts_with(p)) {
        None
    } else {
        Some(text)
    }
}

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

/// 頁尾引擎膠囊的文字與色調（spec 2.5）。
pub(super) fn engine_pill(s: &AppViewState, with_device: bool) -> (String, Tone) {
    if s.privacy_mode {
        ("隱私模式 · 本機".into(), Tone::Success)
    } else if cloud_ready(s) {
        ("雲端 · OpenAI".into(), Tone::Warning)
    } else if s.engine == "openai" {
        ("雲端未完成設定 · 使用本機".into(), Tone::Danger)
    } else {
        let mut label = String::from("本機");
        if !s.model_name.is_empty() {
            label.push_str(" · ");
            label.push_str(&s.model_name);
        }
        if with_device && !s.device_label.is_empty() {
            label.push_str(" · ");
            label.push_str(&short_device(&s.device_label));
        }
        (label, Tone::Neutral)
    }
}

fn pill_width(cx: &mut Ctx<'_>, label: &str) -> f32 {
    cx.text.measure_width(label, FONT_XS, true) + 32.0
}

// ─── 浮動模式 ────────────────────────────────────────────────────────────────

/// 浮動視窗：緊湊列 + 展開內容。
pub fn floating(v: &mut View, cx: &mut Ctx<'_>, width: f32, height: f32) {
    let t = cx.t;
    v.bordered(
        r(0.0, 0.0, width, height),
        RADIUS_LG,
        t.glass_bg,
        1.0,
        t.border,
    );
    compact_bar(v, cx, width);
    if !(cx.s.floating_expanded && height > BAR_H + 1.0) {
        return;
    }
    v.rect(r(0.0, BAR_H, width, 1.0), 0.0, t.glass_hairline);
    // 52-92：即時 / 檔案、置頂、錯誤徽章
    let ry = BAR_H + 6.0;
    segment_tabs(v, cx, r(SP4, ry, 136.0, CTRL_SM));
    let mut right = width - SP4;
    let errors = cx.s.error_log.len();
    if errors > 0 {
        right -= error_badge(v, cx, right, ry + 2.0, errors) + SP2;
    }
    let pinned = cx.ui.pinned;
    let pin_w =
        cx.text.measure_width("置頂", FONT_XS, pinned) + 24.0 + if pinned { 16.0 } else { 0.0 };
    toggle_chip(v, cx, right - pin_w, ry, "置頂", pinned, Msg::TogglePin);
    let y = BAR_H + 40.0;
    if cx.s.file_mode {
        file::view(v, cx, r(0.0, y, width, (height - y).max(0.0)));
    } else {
        let bottom = (height - FOOTER_H).max(y);
        live_content(v, cx, r(0.0, y, width, bottom - y), false);
        footer(v, cx, r(0.0, bottom, width, FOOTER_H), false);
    }
}

/// 380×52 緊湊列：狀態色條、錄音鍵、兩行文字、四個圖示。
pub fn compact_bar(v: &mut View, cx: &mut Ctx<'_>, width: f32) {
    let t = cx.t;
    let s = cx.s;
    let ph = phase(s);
    let error = ph == Phase::Ready && status_is_error(&s.status_text);
    let (bar, tint) = match ph {
        Phase::Recording => (t.recording, Some(t.recording_soft)),
        Phase::Processing => (t.accent, None),
        Phase::NeedsModel | Phase::Downloading => (t.warning, None),
        Phase::Ready if error => (t.danger, None),
        Phase::Ready => (t.border_strong, None),
    };
    if let Some(c) = tint {
        if s.floating_expanded {
            // 只染上面 52px：多畫一截圓角再裁掉。
            v.push_clip(r(1.0, 1.0, width - 2.0, BAR_H - 1.0));
            v.rect(
                r(1.0, 1.0, width - 2.0, BAR_H + RADIUS_LG),
                RADIUS_LG - 1.0,
                c,
            );
            v.pop_clip();
        } else {
            v.rect(r(1.0, 1.0, width - 2.0, BAR_H - 2.0), RADIUS_LG - 1.0, c);
        }
    }
    v.rect(r(0.0, 12.0, 4.0, BAR_H - 24.0), 2.0, bar);
    // 整條都可以拖曳；按鈕壓在上面
    v.drag_area(r(0.0, 0.0, width, BAR_H));
    record_button(v, cx, r(14.0, 8.0, 36.0, 36.0));

    // 圖示（由右往左：關閉、切到完整視窗、展開、設定）
    let tool = 28.0;
    let gap = 2.0;
    let tools: [(Icon, Msg, bool); 4] = [
        (Icon::Gear, Msg::ToggleSettings, s.settings_open),
        (
            if s.floating_expanded {
                Icon::ChevronUp
            } else {
                Icon::ChevronDown
            },
            Msg::ToggleExpanded,
            false,
        ),
        (Icon::Window, Msg::ToggleFloating, false),
        (Icon::Close, Msg::Core(UiCommand::RequestClose), false),
    ];
    let tools_x = width - 8.0 - (4.0 * tool + 3.0 * gap);
    for (i, (ic, msg, active)) in tools.into_iter().enumerate() {
        let rect = r(
            tools_x + i as f32 * (tool + gap),
            (BAR_H - tool) / 2.0,
            tool,
            tool,
        );
        icon_btn(v, t, rect, ic, active, msg);
    }

    // 兩行文字
    let tx = 60.0;
    let tw = (tools_x - 8.0 - tx).max(0.0);
    let l1 = r(tx, 8.0, tw, 20.0);
    let l2 = r(tx, 28.0, tw, 16.0);
    let hint = |cx: &mut Ctx<'_>| -> String {
        let s = cx.s;
        if s.hotkey.is_empty() {
            return "尚未設定快捷鍵".into();
        }
        let full = if s.ptt_mode && !s.ptt_hotkey.is_empty() {
            format!("{} 開始 · 按住 {}", s.hotkey, s.ptt_hotkey)
        } else {
            format!("{} 開始", s.hotkey)
        };
        if cx.text.measure_width(&full, FONT_XS, false) <= tw {
            full
        } else {
            format!("{} 開始", s.hotkey)
        }
    };
    match ph {
        Phase::Recording => {
            let partial = !s.current_partial.is_empty();
            let line = if partial {
                cx.text.elide(&s.current_partial, FONT_SM, true, tw)
            } else {
                "聆聽中…".to_string()
            };
            v.bold_text(l1, line, FONT_SM, t.text, Align::Start);
            if s.mod_waveform {
                waveform(
                    v,
                    t,
                    r(tx, 28.0, 24.0 * 7.0 - 4.0, 16.0),
                    &s.waveform_bars,
                    24,
                    3.0,
                    4.0,
                    3.0,
                    14.0,
                    true,
                );
            }
        }
        Phase::Processing => {
            v.bold_text(l1, "辨識中…", FONT_SM, t.accent, Align::Start);
            v.text(l2, "完成後會輸入到游標位置", FONT_XS, t.text2, Align::Start);
        }
        Phase::NeedsModel => {
            v.bold_text(l1, "尚未下載模型", FONT_SM, t.warning_text, Align::Start);
            v.text(l2, "展開以完成設定", FONT_XS, t.text2, Align::Start);
        }
        Phase::Downloading => {
            let pct = (s.download_progress.clamp(0.0, 1.0) * 100.0).round() as i32;
            v.bold_text(
                l1,
                format!("下載模型 {pct}%"),
                FONT_SM,
                t.warning_text,
                Align::Start,
            );
            progress(v, t, r(tx, 32.0, tw, 4.0), s.download_progress, t.warning);
        }
        Phase::Ready if error => {
            let line = cx
                .text
                .elide(clean_status(&s.status_text), FONT_SM, true, tw);
            v.bold_text(l1, line, FONT_SM, t.danger_text, Align::Start);
            let sub = if !s.error_log.is_empty() && !s.floating_expanded {
                "點 ˅ 查看錯誤紀錄".to_string()
            } else {
                hint(cx)
            };
            v.text(l2, sub, FONT_XS, t.text2, Align::Start);
        }
        Phase::Ready => {
            v.bold_text(l1, "就緒", FONT_SM, t.text, Align::Start);
            let sub = hint(cx);
            v.text(l2, sub, FONT_XS, t.text2, Align::Start);
        }
    }
}

/// 即時 / 檔案 分段。
fn segment_tabs(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let selected = if cx.s.file_mode { "file" } else { "live" };
    segmented_fill(
        v,
        cx,
        rect,
        &[("live", "即時"), ("file", "檔案")],
        selected,
        true,
        |id| Msg::Core(UiCommand::SetFileMode(id == "file")),
    );
}

/// 錯誤徽章（Alert 圖示 + 數量），右緣在 `right`。回傳寬度。
fn error_badge(v: &mut View, cx: &mut Ctx<'_>, right: f32, y0: f32, count: usize) -> f32 {
    let t = cx.t;
    let label = count.to_string();
    let text_w = cx.text.measure_width(&label, FONT_XS, true);
    let width = 10.0 + 12.0 + 4.0 + text_w + 10.0;
    let height = 24.0;
    let x0 = right - width;
    let rect = r(x0, y0, width, height);
    v.rect(rect, height / 2.0, t.danger_soft);
    icon(
        v,
        r(x0 + 10.0, y0 + 6.0, 12.0, 12.0),
        Icon::Alert,
        t.danger_text,
    );
    v.bold_text(
        r(x0 + 26.0, y0, text_w + 2.0, height),
        label,
        FONT_XS,
        t.danger_text,
        Align::Start,
    );
    v.button(rect, height / 2.0, Msg::ToggleErrorLog, t.hover);
    width
}

// ─── 完整模式 ────────────────────────────────────────────────────────────────

/// 完整視窗：標題列、狀態卡、模板列、紀錄、頁尾；檔案分頁用 [`file::view`]。
pub fn full(v: &mut View, cx: &mut Ctx<'_>, width: f32, height: f32) {
    let t = cx.t;
    v.rect(r(0.0, 0.0, width, height), 0.0, t.bg);
    title_bar(v, cx, width);
    if cx.s.file_mode {
        file::view(v, cx, r(0.0, TITLE_H, width, (height - TITLE_H).max(0.0)));
    } else {
        let bottom = (height - FOOTER_H).max(TITLE_H);
        live_content(v, cx, r(0.0, TITLE_H, width, bottom - TITLE_H), true);
        footer(v, cx, r(0.0, bottom, width, FOOTER_H), true);
    }
}

/// 標題列（自繪：無 OS 外框）。
fn title_bar(v: &mut View, cx: &mut Ctx<'_>, width: f32) {
    let t = cx.t;
    v.rect(r(0.0, 0.0, width, TITLE_H), 0.0, t.bg2);
    v.rect(r(0.0, TITLE_H - 1.0, width, 1.0), 0.0, t.border);
    v.drag_area(r(0.0, 0.0, width, TITLE_H));
    v.rect(r(16.0, 14.0, 16.0, 16.0), 8.0, t.accent);
    v.rect(r(21.0, 19.0, 6.0, 6.0), 3.0, t.on_accent);
    v.bold_text(
        r(40.0, 0.0, 150.0, TITLE_H),
        "whisper-typeless",
        FONT_MD,
        t.text,
        Align::Start,
    );
    segment_tabs(v, cx, r((width - 136.0) / 2.0, 8.0, 136.0, CTRL_SM));
    let b = 28.0;
    let by = (TITLE_H - b) / 2.0;
    let mut x = width - 12.0 - b;
    icon_btn(
        v,
        t,
        r(x, by, b, b),
        Icon::Close,
        false,
        Msg::Core(UiCommand::RequestClose),
    );
    x -= b + 4.0;
    icon_btn(v, t, r(x, by, b, b), Icon::Minimize, false, Msg::Minimize);
    x -= b + 4.0;
    icon_btn(
        v,
        t,
        r(x, by, b, b),
        Icon::Gear,
        cx.s.settings_open,
        Msg::ToggleSettings,
    );
    x -= b + 4.0;
    icon_btn(
        v,
        t,
        r(x, by, b, b),
        Icon::Window,
        false,
        Msg::ToggleFloating,
    );
}

/// 即時內容：（完整視窗）狀態卡、模板列、紀錄標題列、轉錄清單。
fn live_content(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>, with_card: bool) {
    let (x0, y0, w, h) = rx(rect);
    let bottom = y0 + h;
    let mut y = y0;
    if with_card {
        y += SP2;
        status_card(v, cx, r(x0 + SP4, y, w - 2.0 * SP4, 88.0));
        y += 88.0 + SP2;
    }
    if cx.s.mod_context {
        context_bar(v, cx, r(x0, y, w, 40.0));
        y += 40.0;
    }
    // 紀錄標題列
    {
        let t = cx.t;
        let s = cx.s;
        v.bold_text(
            r(x0 + SP4, y, w - 2.0 * SP4 - 64.0, 28.0),
            format!("紀錄 · {} 字", s.char_count),
            FONT_XS,
            t.text2,
            Align::Start,
        );
        let has_content = !s.transcript_segments.is_empty() || !s.current_partial.is_empty();
        btn(
            v,
            cx,
            r(x0 + w - SP4 - 56.0, y + 2.0, 56.0, 24.0),
            "清除",
            None,
            Btn::Ghost,
            has_content.then_some(Msg::Core(UiCommand::ClearTranscript)),
        );
        y += 28.0;
    }
    transcript(v, cx, r(x0, y, w, (bottom - y).max(0.0)));
}

/// 主狀態卡（88 高）：色調、標題、內文，缺模型 / 下載中交給 [`onboarding::card`]。
fn status_card(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let ph = phase(s);
    if matches!(ph, Phase::NeedsModel | Phase::Downloading) {
        onboarding::card(v, cx, rect);
        return;
    }
    let (x0, y0, w, _) = rx(rect);
    let (tone, title) = match ph {
        Phase::Recording => (Tone::Recording, "錄音中"),
        Phase::Processing => (Tone::Accent, "辨識中…"),
        _ => (Tone::Neutral, "就緒"),
    };
    card(v, t, rect, RADIUS_LG, tone);
    record_button(v, cx, r(x0 + SP4, y0 + SP4, 56.0, 56.0));
    let tx = x0 + SP4 + 56.0 + SP4;
    let tw = (x0 + w - SP4 - tx).max(0.0);
    let title_color = if tone == Tone::Neutral {
        t.text
    } else {
        t.tone(tone).fg
    };
    v.bold_text(
        r(tx, y0 + 12.0, tw, 30.0),
        title,
        FONT_XL,
        title_color,
        Align::Start,
    );
    let body = r(tx, y0 + 52.0, tw, 22.0);
    let note = status_note(s);
    match (ph, note) {
        (_, Some(note)) => {
            let color = if status_is_error(note) {
                t.danger_text
            } else {
                t.text2
            };
            let shown = cx.text.elide(note, FONT_SM, false, tw);
            v.text(body, shown, FONT_SM, color, Align::Start);
        }
        (Phase::Recording, None) => {
            if s.mod_waveform {
                waveform(
                    v,
                    t,
                    r(tx, y0 + 46.0, 32.0 * 6.0 - 3.0, 28.0),
                    &s.waveform_bars,
                    32,
                    3.0,
                    3.0,
                    3.0,
                    28.0,
                    true,
                );
            } else {
                v.text(body, "正在聆聽…", FONT_SM, t.text2, Align::Start);
            }
        }
        (Phase::Processing, None) => {
            v.text(
                body,
                "完成後會自動輸入到游標所在的視窗",
                FONT_SM,
                t.text2,
                Align::Start,
            );
        }
        _ => hotkey_line(v, cx, tx, y0 + 52.0),
    }
}

/// 鍵帽樣式的熱鍵說明：「[熱鍵] 開始／停止 · 按住 [PTT 熱鍵] 說話」。
fn hotkey_line(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32) {
    let s = cx.s;
    let t = cx.t;
    let mut x = x0;
    let label = |v: &mut View, cx: &mut Ctx<'_>, x: &mut f32, text: &str| {
        let tw = cx.text.measure_width(text, FONT_SM, false);
        v.text(
            r(*x, y0, tw + 2.0, 22.0),
            text,
            FONT_SM,
            t.text2,
            Align::Start,
        );
        *x += tw + 6.0;
    };
    if s.hotkey.is_empty() {
        label(v, cx, &mut x, "尚未設定快捷鍵，請到設定 › 一般指定");
        return;
    }
    x += keycap(v, cx, x, y0, &s.hotkey) + 6.0;
    label(v, cx, &mut x, "開始／停止");
    if s.ptt_mode && !s.ptt_hotkey.is_empty() {
        label(v, cx, &mut x, "· 按住");
        x += keycap(v, cx, x, y0, &s.ptt_hotkey) + 6.0;
        label(v, cx, &mut x, "說話");
    }
}

/// 情境模板列：文字膠囊（高 28），選取項為 accent 實心。
fn context_bar(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let (x0, y0, _, h) = rx(rect);
    let mut x = x0 + SP4;
    let y = y0 + (h - CTRL_SM) / 2.0;
    for (id, _, name) in TEMPLATES {
        let selected = cx.s.current_template == id;
        let tw = cx.text.measure_width(name, FONT_XS, selected);
        let chip = r(x, y, tw + 24.0, CTRL_SM);
        v.rect(
            chip,
            CTRL_SM / 2.0,
            if selected { t.accent } else { t.chip_bg },
        );
        v.styled_text(
            chip,
            name,
            FONT_XS,
            selected,
            if selected { t.on_accent } else { t.text2 },
            Align::Center,
        );
        if !selected {
            v.button(
                chip,
                CTRL_SM / 2.0,
                Msg::Core(UiCommand::TemplateSelected(id.to_string())),
                t.hover,
            );
        }
        x += tw + 24.0 + 6.0;
    }
}

/// 頁尾：輸出方式開關、（完整視窗的）錯誤徽章、引擎膠囊。
fn footer(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>, full: bool) {
    let t = cx.t;
    let s = cx.s;
    let (x0, y0, w, h) = rx(rect);
    if full {
        v.rect(rect, 0.0, t.bg2);
        v.rect(r(x0, y0, w, 1.0), 0.0, t.border);
    } else {
        v.rect(r(x0, y0, w, 1.0), 0.0, t.glass_hairline);
    }
    let mut x = x0 + SP4;
    let cy = y0 + (h - CTRL_SM) / 2.0;
    x += toggle_chip(
        v,
        cx,
        x,
        cy,
        "自動貼上",
        s.inject_enabled,
        Msg::Core(UiCommand::SetInjectEnabled(!s.inject_enabled)),
    ) + SP2;
    x += toggle_chip(
        v,
        cx,
        x,
        cy,
        "剪貼簿",
        s.clipboard_enabled,
        Msg::Core(UiCommand::SetClipboardEnabled(!s.clipboard_enabled)),
    ) + SP2;
    if !s.inject_enabled && !s.clipboard_enabled {
        x += status_pill(
            v,
            cx,
            x,
            y0 + (h - 24.0) / 2.0,
            "文字不會輸出",
            Tone::Warning,
            Some(Msg::OpenSettings(SettingsTab::Output)),
        ) + SP2;
    }
    let mut right = x0 + w - SP4;
    let errors = s.error_log.len();
    if full && errors > 0 {
        right -= error_badge(v, cx, right, y0 + (h - 24.0) / 2.0, errors) + SP2;
    }
    let (label, tone) = engine_pill(s, full);
    let max_text = right - x - 32.0;
    if max_text >= 48.0 {
        let label = cx.text.elide(&label, FONT_XS, true, max_text);
        let pw = pill_width(cx, &label);
        status_pill(
            v,
            cx,
            right - pw,
            y0 + (h - 24.0) / 2.0,
            &label,
            tone,
            Some(Msg::OpenSettings(SettingsTab::Engine)),
        );
    }
}

// ─── 轉錄清單 ────────────────────────────────────────────────────────────────

/// 提示文字：空白紀錄的三行說明。
fn empty_lines(s: &AppViewState) -> [String; 3] {
    match phase(s) {
        Phase::NeedsModel | Phase::Downloading => [
            "模型準備好之後就能開始說話".into(),
            "文字會輸入到游標所在的位置".into(),
            "說完的句子會顯示在這裡".into(),
        ],
        _ => [
            if s.hotkey.is_empty() {
                "在任何視窗開始說話".into()
            } else {
                format!("在任何視窗按 {} 開始說話", s.hotkey)
            },
            "文字會輸入到游標所在的位置".into(),
            "說完的句子會顯示在這裡".into(),
        ],
    }
}

/// 可捲動的轉錄紀錄：無卡片外框的列（時間欄 + 文字），進行中的句子用 accent 底與左條標示。
pub fn transcript(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let (x0, y0, w, h) = rx(rect);
    if h <= 0.0 {
        return;
    }
    let row_pad = 10.0;
    let list_w = (w - 2.0 * SP4 - 6.0).max(40.0);
    let text_w = (list_w - TIME_W - SP2).max(40.0);
    let text_h = |cx: &mut Ctx<'_>, text: &str| cx.text.measure_wrapped(text, FONT_MD, text_w);

    let mut heights = Vec::with_capacity(s.transcript_segments.len());
    for seg in &s.transcript_segments {
        heights.push(text_h(cx, &seg.text) + 2.0 * row_pad);
    }
    let partial = !s.current_partial.is_empty();
    let partial_h = if partial {
        text_h(cx, &s.current_partial) + 2.0 * row_pad
    } else {
        0.0
    };
    let empty = s.transcript_segments.is_empty() && !partial;
    let content_h = heights.iter().sum::<f32>() + partial_h + SP2;
    let max = (content_h - h).max(0.0);
    let offset = if cx.ui.follow_transcript {
        max
    } else {
        cx.ui.scroll(ScrollId::Transcript).min(max)
    };
    v.scroll_area(rect, ScrollId::Transcript, max);

    v.push_clip(rect);
    v.push_offset(0.0, -offset);
    let lx = x0 + SP4;
    let mut y = y0;
    for (i, (seg, hh)) in s.transcript_segments.iter().zip(&heights).enumerate() {
        if y + hh >= y0 + offset && y <= y0 + offset + h {
            if i > 0 {
                v.rect(r(lx, y, list_w, 1.0), 0.0, t.border);
            }
            v.text(
                r(lx, y + row_pad + 1.0, TIME_W, FONT_MD * 1.4),
                seg.timestamp.clone(),
                FONT_XS,
                t.text2,
                Align::Start,
            );
            v.wrapped(
                r(lx + TIME_W + SP2, y + row_pad, text_w, hh - 2.0 * row_pad),
                seg.text.clone(),
                FONT_MD,
                t.text,
            );
        }
        y += hh;
    }
    if partial {
        if !s.transcript_segments.is_empty() {
            v.rect(r(lx, y, list_w, 1.0), 0.0, t.border);
        }
        v.rect(r(lx, y, list_w, partial_h), 6.0, t.accent_soft);
        v.rect(
            r(lx, y + 4.0, 3.0, (partial_h - 8.0).max(0.0)),
            1.5,
            t.accent,
        );
        v.bold_text(
            r(lx + 10.0, y + row_pad, TIME_W, FONT_MD * 1.4),
            if s.is_processing {
                "辨識中"
            } else {
                "聆聽中"
            },
            FONT_XS,
            t.accent,
            Align::Start,
        );
        v.wrapped(
            r(
                lx + TIME_W + SP2,
                y + row_pad,
                text_w,
                partial_h - 2.0 * row_pad,
            ),
            s.current_partial.clone(),
            FONT_MD,
            t.text,
        );
    }
    if empty {
        let lines = empty_lines(s);
        let top = y0 + (h - 3.0 * 22.0) / 2.0 - 16.0;
        for (i, line) in lines.iter().enumerate() {
            v.text(
                r(x0 + SP4, top + i as f32 * 22.0, w - 2.0 * SP4, 22.0),
                cx.text.elide(line, FONT_SM, false, w - 2.0 * SP4),
                FONT_SM,
                t.text2,
                Align::Center,
            );
        }
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
        RADIUS_SM / 3.0,
        cx.t.border_strong,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ModelEntryView;
    use crate::ui_acme::local::UiLocal;
    use crate::ui_acme::paint::{Action, Item};
    use crate::ui_acme::text::TextEngine;
    use crate::ui_acme::theme::Theme;

    fn render(s: &AppViewState, w: f32, h: f32, f: fn(&mut View, &mut Ctx<'_>, f32, f32)) -> View {
        let ui = UiLocal::default();
        let mut text = TextEngine::new();
        let mut cx = Ctx {
            s,
            ui: &ui,
            t: Theme::for_mode(false),
            text: &mut text,
            scale: 1.0,
        };
        let mut v = View::new();
        f(&mut v, &mut cx, w, h);
        v
    }

    fn full_view(s: &AppViewState) -> View {
        render(s, 560.0, 620.0, full)
    }

    fn bar(s: &AppViewState) -> View {
        render(s, 380.0, 52.0, |v, cx, w, _| compact_bar(v, cx, w))
    }

    fn has_text(v: &View, needle: &str) -> bool {
        v.items.iter().any(|i| match i {
            Item::Text { content, .. } | Item::Wrapped { content, .. } => content.contains(needle),
            _ => false,
        })
    }

    fn can(v: &View, msg: Msg) -> bool {
        let want = Action::Click(msg);
        v.find_hit(|a| *a == want).is_some()
    }

    fn state() -> AppViewState {
        AppViewState {
            hotkey: "Ctrl+Shift+W".into(),
            ptt_hotkey: "Ctrl+Alt+L".into(),
            ptt_mode: true,
            model_name: "small".into(),
            device_label: "CPU（此版本未含 GPU）".into(),
            ..AppViewState::default()
        }
    }

    #[test]
    fn short_device_labels() {
        assert_eq!(short_device("CPU（此版本未含 GPU）"), "CPU");
        assert_eq!(short_device("GPU (Vulkan)"), "Vulkan");
        assert_eq!(short_device("GPU (CUDA)"), "CUDA");
        assert_eq!(short_device("GPU (Metal)"), "Metal");
    }

    #[test]
    fn engine_pill_follows_spec_table() {
        let mut s = state();
        assert_eq!(
            engine_pill(&s, true),
            ("本機 · small · CPU".to_string(), Tone::Neutral)
        );
        assert_eq!(engine_pill(&s, false).0, "本機 · small");
        s.engine = "openai".into();
        assert_eq!(
            engine_pill(&s, true),
            ("雲端未完成設定 · 使用本機".to_string(), Tone::Danger)
        );
        s.openai_consent = true;
        s.openai_key_set = true;
        assert_eq!(
            engine_pill(&s, true),
            ("雲端 · OpenAI".to_string(), Tone::Warning)
        );
        s.privacy_mode = true;
        assert_eq!(
            engine_pill(&s, true),
            ("隱私模式 · 本機".to_string(), Tone::Success)
        );
    }

    #[test]
    fn status_note_hides_routine_and_keeps_messages() {
        let mut s = state();
        for routine in ["就緒", "● 錄音中...", "辨識中", "下載模型 (142 MB)..."] {
            s.status_text = routine.into();
            assert_eq!(status_note(&s), None, "{routine}");
        }
        s.status_text = "音訊裝置錯誤: x".into();
        assert_eq!(status_note(&s), Some("音訊裝置錯誤: x"));
        assert!(status_is_error("下載失敗: 逾時"));
        assert!(!status_is_error("模板已儲存"));
        s.status_text = "\u{26a0} 設定載入有問題".into();
        assert_eq!(status_note(&s), Some("設定載入有問題"));
    }

    #[test]
    fn idle_card_teaches_hotkeys_and_empty_list_repeats_them() {
        let v = full_view(&state());
        assert!(has_text(&v, "就緒"));
        assert!(has_text(&v, "Ctrl+Shift+W"));
        assert!(has_text(&v, "Ctrl+Alt+L"));
        assert!(has_text(&v, "在任何視窗按 Ctrl+Shift+W 開始說話"));
        assert!(!has_text(&v, "按下錄音按鈕"));
    }

    #[test]
    fn card_titles_differ_per_phase() {
        let mut s = state();
        s.is_recording = true;
        assert!(has_text(&full_view(&s), "錄音中"));
        s.is_recording = false;
        s.is_processing = true;
        assert!(has_text(&full_view(&s), "辨識中…"));
        let nm = AppViewState {
            selected_model_id: "base".into(),
            model_entries: vec![ModelEntryView {
                id: "base".into(),
                display_name: "Base".into(),
                ..ModelEntryView::default()
            }],
            ..state()
        };
        assert!(has_text(&full_view(&nm), "還差一步：下載語音模型"));
    }

    #[test]
    fn footer_toggles_send_inverse_and_warn_when_output_is_off() {
        let mut s = state();
        let v = full_view(&s);
        assert!(can(&v, Msg::Core(UiCommand::SetInjectEnabled(false))));
        assert!(can(&v, Msg::Core(UiCommand::SetClipboardEnabled(false))));
        assert!(!has_text(&v, "文字不會輸出"));
        s.inject_enabled = false;
        s.clipboard_enabled = false;
        let v = full_view(&s);
        assert!(has_text(&v, "文字不會輸出"));
        assert!(can(&v, Msg::Core(UiCommand::SetInjectEnabled(true))));
        assert!(can(&v, Msg::OpenSettings(SettingsTab::Output)));
    }

    #[test]
    fn error_badge_only_with_errors_and_opens_log() {
        let mut s = state();
        assert!(!can(&full_view(&s), Msg::ToggleErrorLog));
        s.error_log = vec!["10:00:00 x".into()];
        assert!(can(&full_view(&s), Msg::ToggleErrorLog));
    }

    #[test]
    fn engine_pill_opens_engine_settings() {
        let v = full_view(&state());
        assert!(can(&v, Msg::OpenSettings(SettingsTab::Engine)));
        assert!(has_text(&v, "本機 · small · CPU"));
    }

    #[test]
    fn title_bar_has_four_icons_and_tabs_but_no_theme_toggle() {
        let s = state();
        let v = full_view(&s);
        assert!(can(&v, Msg::ToggleSettings));
        assert!(can(&v, Msg::ToggleFloating));
        assert!(can(&v, Msg::Minimize));
        assert!(can(&v, Msg::Core(UiCommand::RequestClose)));
        assert!(can(&v, Msg::Core(UiCommand::SetFileMode(true))));
        assert!(!can(&v, Msg::Core(UiCommand::SetDarkMode(true))));
    }

    #[test]
    fn transcript_rows_show_time_and_partial_marker() {
        let mut s = state();
        s.transcript_segments = vec![crate::core::TranscriptSegment {
            text: "你好".into(),
            timestamp: "10:05".into(),
        }];
        s.is_recording = true;
        s.current_partial = "接下來我們討論".into();
        let v = full_view(&s);
        assert!(has_text(&v, "10:05"));
        assert!(has_text(&v, "聆聽中"));
        assert!(has_text(&v, "接下來我們討論"));
        assert!(!has_text(&v, "在任何視窗按"), "有內容時不顯示空白提示");
    }

    #[test]
    fn template_row_selects_other_templates_only() {
        let mut s = state();
        s.current_template = "meeting".into();
        let v = full_view(&s);
        assert!(!can(
            &v,
            Msg::Core(UiCommand::TemplateSelected("meeting".into()))
        ));
        assert!(can(
            &v,
            Msg::Core(UiCommand::TemplateSelected("casual".into()))
        ));
        s.mod_context = false;
        assert!(!can(
            &full_view(&s),
            Msg::Core(UiCommand::TemplateSelected("casual".into()))
        ));
    }

    #[test]
    fn compact_bar_text_differs_per_state() {
        let mut s = state();
        let v = bar(&s);
        assert!(has_text(&v, "就緒"));
        assert!(has_text(&v, "Ctrl+Shift+W 開始 · 按住 Ctrl+Alt+L"));
        assert!(can(&v, Msg::ToggleSettings));
        assert!(can(&v, Msg::ToggleExpanded));
        assert!(can(&v, Msg::ToggleFloating));
        s.is_processing = true;
        assert!(has_text(&bar(&s), "辨識中…"));
        s.is_processing = false;
        s.is_recording = true;
        assert!(has_text(&bar(&s), "聆聽中…"));
        s.current_partial = "接下來我們討論一下下一個版本的".into();
        assert!(has_text(&bar(&s), "接下來我們討論"));
        s.is_recording = false;
        s.current_partial.clear();
        s.status_text = "音訊裝置錯誤: x".into();
        s.error_log = vec!["10:00:00 x".into()];
        let v = bar(&s);
        assert!(has_text(&v, "音訊裝置錯誤: x"));
        assert!(has_text(&v, "點 ˅ 查看錯誤紀錄"));
    }

    #[test]
    fn compact_bar_shows_download_percent_and_missing_model() {
        let mut s = state();
        s.selected_model_id = "base".into();
        s.model_entries = vec![ModelEntryView {
            id: "base".into(),
            ..ModelEntryView::default()
        }];
        assert!(has_text(&bar(&s), "尚未下載模型"));
        s.downloading_id = "base".into();
        s.download_progress = 0.42;
        assert!(has_text(&bar(&s), "下載模型 42%"));
    }

    #[test]
    fn expanded_floating_has_pin_and_footer_but_no_status_card() {
        let mut s = state();
        s.floating_mode = true;
        s.floating_expanded = true;
        let v = render(&s, 380.0, 540.0, floating);
        assert!(can(&v, Msg::TogglePin));
        assert!(can(&v, Msg::Core(UiCommand::SetFileMode(true))));
        assert!(can(&v, Msg::OpenSettings(SettingsTab::Engine)));
        assert!(has_text(&v, "本機 · small"));
        assert!(!has_text(&v, "還差一步"));
    }
}
