//! 檔案轉錄模式：選檔、輸出格式、進度、開始、結果（複製 / 匯出）。

use acme_core::{Logical, Rect};

use super::live::scrollbar;
use super::widgets::{progress, segmented, text_button, FONT_MD, FONT_SM, FONT_XS, RADIUS_MD};
use super::Ctx;
use crate::core::UiCommand;
use crate::ui_acme::msg::{Msg, ScrollId};
use crate::ui_acme::paint::{r, View};
use crate::ui_acme::text::Align;

pub const FORMATS: [(&str, &str); 3] = [("txt", "TXT"), ("srt", "SRT"), ("vtt", "VTT")];

/// 路徑只顯示檔名。
pub fn display_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

pub fn view(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>) {
    let t = cx.t;
    let s = cx.s;
    let x0 = rect.origin.x.get() + 20.0;
    let w = (rect.size.width.get() - 40.0).max(0.0);
    let bottom = rect.origin.y.get() + rect.size.height.get();
    let mut y = rect.origin.y.get() + 20.0;
    let has_file = !s.file_name.is_empty();

    // ── 選檔區 ──
    let picker = r(x0, y, w, 100.0);
    v.bordered(
        picker,
        RADIUS_MD,
        if has_file { t.accent_soft } else { t.bg2 },
        2.0,
        if has_file { t.accent } else { t.border },
    );
    v.text(
        r(x0, y + 12.0, w, 44.0),
        if has_file { "📄" } else { "📂" },
        32.0,
        t.text,
        Align::Center,
    );
    let label = if has_file {
        display_name(&s.file_name).to_string()
    } else {
        "點擊選擇音訊/影片檔案".to_string()
    };
    let label = cx.text.elide(&label, FONT_SM, false, w - 24.0);
    v.text(
        r(x0, y + 62.0, w, 24.0),
        label,
        FONT_SM,
        if has_file { t.text } else { t.placeholder },
        Align::Center,
    );
    if !s.file_processing {
        v.button(picker, RADIUS_MD, UiCommand::OpenFileDialog, t.hover);
    }
    y += 112.0;

    // ── 輸出格式 ──
    v.text(
        r(x0, y, 80.0, 28.0),
        "輸出格式：",
        FONT_SM,
        t.text2,
        Align::Start,
    );
    segmented(
        v,
        t,
        x0 + 80.0,
        y,
        52.0,
        28.0,
        &FORMATS,
        &s.file_output_format,
        !s.file_processing,
        |id| Msg::Core(UiCommand::SetFileOutputFormat(id.to_string())),
    );
    y += 40.0;

    // ── 進度 ──
    if s.file_processing || s.file_has_result {
        let frac = if s.file_has_result {
            1.0
        } else {
            s.file_progress
        };
        let color = if s.file_has_result {
            t.success
        } else {
            t.accent
        };
        progress(v, t, r(x0, y, w, 6.0), frac, color);
        y += 18.0;
    }

    // ── 開始 ──
    let can_start = has_file && !s.file_processing;
    let start = r(x0, y, w, 44.0);
    v.bordered(
        start,
        RADIUS_MD,
        if can_start { t.accent } else { t.bg2 },
        1.0,
        if can_start { t.accent } else { t.border },
    );
    v.text(
        start,
        if s.file_processing {
            "⏳  轉錄中…"
        } else {
            "▶  開始轉錄"
        },
        FONT_MD,
        if can_start {
            t.on_accent
        } else {
            t.placeholder
        },
        Align::Center,
    );
    if can_start {
        v.button(start, RADIUS_MD, UiCommand::StartFileTranscription, t.hover);
    }
    y += 56.0;

    // ── 結果 ──
    if s.file_has_result {
        let area_h = (bottom - 12.0 - y).max(80.0);
        let area = r(x0, y, w, area_h);
        v.bordered(area, RADIUS_MD, t.bg2, 1.0, t.border);
        v.rect(r(x0 + 1.0, y + 1.0, w - 2.0, 35.0), RADIUS_MD - 1.0, t.bg3);
        v.rect(r(x0, y + 36.0, w, 1.0), 0.0, t.border);
        v.bold_text(
            r(x0 + 12.0, y, 120.0, 36.0),
            "轉錄結果",
            FONT_XS,
            t.text2,
            Align::Start,
        );
        text_button(
            v,
            t,
            r(x0 + w - 12.0 - 52.0, y + 6.0, 52.0, 24.0),
            "匯出",
            FONT_XS,
            t.on_accent,
            t.accent,
            None,
            Some(Msg::Core(UiCommand::FileExportResult(
                s.file_output_format.clone(),
            ))),
        );
        text_button(
            v,
            t,
            r(x0 + w - 12.0 - 52.0 - 6.0 - 52.0, y + 6.0, 52.0, 24.0),
            "複製",
            FONT_XS,
            t.text,
            t.bg2,
            Some(t.border),
            Some(Msg::Core(UiCommand::FileCopyResult)),
        );
        let text_rect = r(x0, y + 37.0, w, (area_h - 37.0).max(0.0));
        let inner_w = (w - 30.0).max(20.0);
        let content_h = cx.text.measure_wrapped(&s.file_result, FONT_SM, inner_w) + 24.0;
        let max = (content_h - text_rect.size.height.get()).max(0.0);
        let offset = cx.ui.scroll(ScrollId::FileResult).min(max);
        v.scroll_area(text_rect, ScrollId::FileResult, max);
        v.push_clip(text_rect);
        v.push_offset(0.0, -offset);
        v.wrapped(
            r(x0 + 12.0, y + 37.0 + 12.0, inner_w, content_h),
            s.file_result.clone(),
            FONT_SM,
            t.text,
        );
        v.pop_offset();
        v.pop_clip();
        scrollbar(v, cx, text_rect, offset, max, content_h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_strips_directories() {
        assert_eq!(display_name("C:\\錄音\\會議.mp3"), "會議.mp3");
        assert_eq!(display_name("/tmp/a.wav"), "a.wav");
        assert_eq!(display_name("b.flac"), "b.flac");
    }
}
