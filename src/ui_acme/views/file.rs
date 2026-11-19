//! 檔案轉錄模式：選檔、輸出格式、進度、開始、結果（複製 / 匯出）。版面見 `docs/ui-redesign.md` 2.6。

use acme_core::{Logical, Rect};

use super::live::{error_badge, scrollbar};
use super::widgets::{
    btn, card, progress, segmented_fill, status_pill, Btn, CTRL_LG, CTRL_MD, FONT_MD, FONT_SM,
    FONT_XS, RADIUS_MD, SP2, SP3, SP4,
};
use super::Ctx;
use crate::core::UiCommand;
use crate::ui_acme::msg::{Msg, ScrollId};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;
use crate::ui_acme::theme::Tone;

pub const FORMATS: [(&str, &str); 3] = [("txt", "TXT"), ("srt", "SRT"), ("vtt", "VTT")];

/// 頁尾高（與即時頁相同）。
const FOOTER_H: f32 = 48.0;

/// 路徑只顯示檔名。
pub fn display_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

pub fn view(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let x0 = rect.origin.x.get() + SP4;
    let w = (rect.size.width.get() - 2.0 * SP4).max(0.0);
    let bottom = rect.origin.y.get() + rect.size.height.get();
    let mut y = rect.origin.y.get() + SP2;
    let has_file = !s.file_name.is_empty();
    let busy = s.file_processing;

    // ── 檔案卡 ──
    let picker = r(x0, y, w, 72.0);
    card(v, t, picker, RADIUS_MD, Tone::Neutral);
    let btn_w = if has_file { 72.0 } else { 104.0 };
    let text_w = (w - 2.0 * SP4 - btn_w - SP3).max(40.0);
    if has_file {
        let name = cx
            .text
            .elide(display_name(&s.file_name), FONT_MD, true, text_w);
        v.bold_text(
            r(x0 + SP4, y, text_w, 72.0),
            name,
            FONT_MD,
            t.text,
            Align::Start,
        );
    } else {
        v.text(
            r(x0 + SP4, y, text_w, 72.0),
            "選擇音訊或影片檔…",
            FONT_MD,
            t.text2,
            Align::Start,
        );
    }
    btn(
        v,
        cx,
        r(
            x0 + w - SP4 - btn_w,
            y + (72.0 - CTRL_MD) / 2.0,
            btn_w,
            CTRL_MD,
        ),
        if has_file { "更換" } else { "選擇檔案" },
        None,
        if has_file {
            Btn::Secondary
        } else {
            Btn::Primary
        },
        (!busy).then_some(Msg::Core(UiCommand::OpenFileDialog)),
    );
    y += 72.0 + SP3;

    // ── 輸出格式 ──
    v.text(
        r(x0, y, 72.0, CTRL_MD),
        "輸出格式",
        FONT_SM,
        t.text2,
        Align::Start,
    );
    segmented_fill(
        v,
        cx,
        r(x0 + 80.0, y, 180.0, CTRL_MD),
        &FORMATS,
        &s.file_output_format,
        !busy,
        |id| Msg::Core(UiCommand::SetFileOutputFormat(id.to_string())),
    );
    if s.file_has_result && !busy {
        // 完成膠囊放在格式列右側，版面不會因完成而往下跳。
        let label = "轉錄完成";
        let pw = cx.text.measure_width(label, FONT_XS, true) + 32.0;
        status_pill(
            v,
            cx,
            x0 + w - pw,
            y + (CTRL_MD - 24.0) / 2.0,
            label,
            Tone::Success,
            None,
        );
    }
    y += CTRL_MD + SP3;

    // ── 進度 ──
    if busy {
        let pct = (s.file_progress.clamp(0.0, 1.0) * 100.0).round() as i32;
        v.text(
            r(x0, y, w, 16.0),
            format!("轉錄中 {pct}%"),
            FONT_XS,
            t.text2,
            Align::Start,
        );
        progress(v, t, r(x0, y + 20.0, w, 6.0), s.file_progress, t.accent);
        y += 34.0;
    }

    // ── 開始 ──
    let start = Msg::Core(UiCommand::StartFileTranscription);
    let (label, kind, msg) = if busy {
        ("轉錄中…", Btn::Secondary, None)
    } else if !has_file {
        ("開始轉錄", Btn::Primary, None)
    } else if s.file_has_result {
        ("重新轉錄", Btn::Secondary, Some(start))
    } else {
        ("開始轉錄", Btn::Primary, Some(start))
    };
    // 完成後「重新轉錄」降為 32 高，讓結果卡的「匯出」成為主要動作。
    let start_h = if s.file_has_result && !busy {
        CTRL_MD
    } else {
        CTRL_LG
    };
    btn(v, cx, r(x0, y, w, start_h), label, None, kind, msg);
    y += start_h + SP3;

    // ── 頁尾：與即時頁相同的 48px 列（說明 + 錯誤徽章）──
    let full = !s.floating_mode;
    let fy = bottom - FOOTER_H;
    let fx = rect.origin.x.get();
    let fw = rect.size.width.get();
    if full {
        v.rect(r(fx, fy, fw, FOOTER_H), 0.0, t.bg2);
        v.rect(r(fx, fy, fw, 1.0), 0.0, t.border);
    } else {
        v.rect(r(fx, fy, fw, 1.0), 0.0, t.glass_hairline);
    }
    let mut note_right = x0 + w;
    let errors = s.error_log.len();
    if full && errors > 0 {
        note_right -= error_badge(v, cx, x0 + w, fy + (FOOTER_H - 24.0) / 2.0, errors) + SP2;
    }
    v.text(
        r(x0, fy, (note_right - x0).max(0.0), FOOTER_H),
        "檔案轉錄一律使用本機模型",
        FONT_XS,
        t.text2,
        Align::Start,
    );
    let area_bottom = fy - SP3;

    // ── 結果 ──
    if s.file_has_result && !busy {
        let area_h = (area_bottom - y).max(96.0);
        let area = r(x0, y, w, area_h);
        card(v, t, area, RADIUS_MD, Tone::Neutral);
        v.bold_text(
            r(x0 + SP3, y, 120.0, 44.0),
            "轉錄結果",
            FONT_SM,
            t.text,
            Align::Start,
        );
        let export_w = 104.0;
        let copy_w = 60.0;
        let by = y + 8.0;
        btn(
            v,
            cx,
            r(x0 + w - SP3 - export_w, by, export_w, 28.0),
            &format!("匯出 {}", s.file_output_format.to_uppercase()),
            None,
            Btn::Primary,
            Some(Msg::Core(UiCommand::FileExportResult(
                s.file_output_format.clone(),
            ))),
        );
        btn(
            v,
            cx,
            r(x0 + w - SP3 - export_w - SP2 - copy_w, by, copy_w, 28.0),
            "複製",
            None,
            Btn::Secondary,
            Some(Msg::Core(UiCommand::FileCopyResult)),
        );
        v.rect(r(x0, y + 44.0, w, 1.0), 0.0, t.border);
        let text_rect = r(x0, y + 45.0, w, (area_h - 45.0).max(0.0));
        let inner_w = (w - 2.0 * SP3 - 6.0).max(20.0);
        let content_h = cx.text.measure_wrapped(&s.file_result, FONT_SM, inner_w) + 2.0 * SP3;
        let max = (content_h - text_rect.size.height.get()).max(0.0);
        let offset = cx.ui.scroll(ScrollId::FileResult).min(max);
        v.scroll_area(text_rect, ScrollId::FileResult, max);
        v.push_clip(text_rect);
        v.push_offset(0.0, -offset);
        v.wrapped(
            r(x0 + SP3, y + 45.0 + SP3, inner_w, content_h),
            s.file_result.clone(),
            FONT_SM,
            t.text,
        );
        v.pop_offset();
        v.pop_clip();
        scrollbar(v, cx, text_rect, offset, max, content_h);
    } else if !busy && area_bottom - y > 40.0 {
        v.text(
            r(x0, y + (area_bottom - y - 22.0) / 2.0 - 16.0, w, 22.0),
            "轉錄結果會顯示在這裡",
            FONT_SM,
            t.text2,
            Align::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::AppViewState;
    use crate::ui_acme::local::UiLocal;
    use crate::ui_acme::paint::{Action, Item};
    use crate::ui_acme::text::TextEngine;
    use crate::ui_acme::theme::Theme;

    fn build(s: &AppViewState) -> View {
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
        view(&mut v, &mut cx, r(0.0, 44.0, 560.0, 576.0));
        v
    }

    fn has_text(v: &View, needle: &str) -> bool {
        v.items.iter().any(|i| match i {
            Item::Text { content, .. } | Item::Wrapped { content, .. } => content == needle,
            _ => false,
        })
    }

    fn can(v: &View, cmd: UiCommand) -> bool {
        let want = Action::Click(Msg::Core(cmd));
        v.find_hit(|a| *a == want).is_some()
    }

    #[test]
    fn display_name_strips_directories() {
        assert_eq!(display_name("C:\\錄音\\會議.mp3"), "會議.mp3");
        assert_eq!(display_name("/tmp/a.wav"), "a.wav");
        assert_eq!(display_name("b.flac"), "b.flac");
    }

    #[test]
    fn empty_state_offers_picker_but_cannot_start() {
        let v = build(&AppViewState::default());
        assert!(has_text(&v, "選擇音訊或影片檔…"));
        assert!(has_text(&v, "選擇檔案"));
        assert!(can(&v, UiCommand::OpenFileDialog));
        assert!(!can(&v, UiCommand::StartFileTranscription));
    }

    #[test]
    fn selected_file_can_start_and_shows_name_only() {
        let s = AppViewState {
            file_name: "C:\\錄音\\訪談.mp3".into(),
            ..AppViewState::default()
        };
        let v = build(&s);
        assert!(has_text(&v, "訪談.mp3"));
        assert!(has_text(&v, "更換"));
        assert!(can(&v, UiCommand::StartFileTranscription));
    }

    #[test]
    fn processing_shows_percent_and_locks_controls() {
        let s = AppViewState {
            file_name: "a.mp3".into(),
            file_processing: true,
            file_progress: 0.37,
            ..AppViewState::default()
        };
        let v = build(&s);
        assert!(has_text(&v, "轉錄中 37%"));
        assert!(has_text(&v, "轉錄中…"));
        assert!(!can(&v, UiCommand::StartFileTranscription));
        assert!(!can(&v, UiCommand::OpenFileDialog));
    }

    #[test]
    fn footer_shows_error_badge_only_with_errors() {
        let mut s = AppViewState::default();
        let want = Action::Click(Msg::ToggleErrorLog);
        assert!(build(&s).find_hit(|a| *a == want).is_none());
        s.error_log = vec!["10:00:00 x".into()];
        let v = build(&s);
        assert!(v.find_hit(|a| *a == want).is_some());
        assert!(has_text(&v, "檔案轉錄一律使用本機模型"));
    }

    #[test]
    fn result_offers_export_copy_and_retranscribe() {
        let s = AppViewState {
            file_name: "a.mp3".into(),
            file_has_result: true,
            file_result: "結果".into(),
            file_output_format: "srt".into(),
            ..AppViewState::default()
        };
        let v = build(&s);
        assert!(has_text(&v, "匯出 SRT"));
        assert!(has_text(&v, "轉錄完成"));
        assert!(has_text(&v, "重新轉錄"));
        assert!(can(&v, UiCommand::FileExportResult("srt".into())));
        assert!(can(&v, UiCommand::FileCopyResult));
        assert!(can(&v, UiCommand::StartFileTranscription));
    }
}
