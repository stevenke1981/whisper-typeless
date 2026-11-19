//! 檔案模式：選檔、整檔轉錄、複製、匯出（UI 無關）。

use std::path::PathBuf;
use tracing::{error, info};

use crate::audio::AudioSegment;
use crate::core::backend::FileTranscriptionRequest;
use crate::core::event::CoreEvent;
use crate::core::Notifier;
use crate::file_transcription::{decode_audio_file, format_transcript, ExportFormat};
use crate::whisper::WhisperEngine;

use super::ModelContext;

/// 支援的音訊 / 影片副檔名（選檔對話框篩選）。
pub(crate) const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "mp4", "wav", "flac", "ogg", "m4a", "aac", "mkv", "mov", "avi", "webm",
];

/// 開啟選檔對話框（背景執行緒）；選到檔案時送出 `FilePicked`。
pub(crate) fn spawn_pick_file(notifier: Notifier) {
    std::thread::spawn(move || {
        let result = rfd::FileDialog::new()
            .add_filter("音訊/影片", AUDIO_EXTENSIONS)
            .pick_file();

        if let Some(path) = result {
            notifier.send(CoreEvent::FilePicked(path.to_string_lossy().into_owned()));
        }
    });
}

/// 背景轉錄整個檔案。
pub(crate) fn spawn_file_transcription(req: FileTranscriptionRequest, models: ModelContext) {
    let file_path = PathBuf::from(&req.file_path);
    if !file_path.exists() {
        return;
    }

    std::thread::spawn(move || {
        let notifier = models.notifier.clone();
        notifier.send(CoreEvent::FileStarted);

        let decoded = match decode_audio_file(&file_path) {
            Ok(d) => d,
            Err(e) => {
                notifier.send(CoreEvent::FileFailed(format!("解碼失敗: {e}")));
                return;
            }
        };

        notifier.send(CoreEvent::FileProgress(0.3));

        let id_lower = req.model_id.to_lowercase();
        let model_path = super::model::model_path(&models.models_dir, &id_lower);

        if !model_path.exists() {
            notifier.send(CoreEvent::FileFailed(format!(
                "模型尚未下載: {}\n已開啟下載確認，下載完成後請再開始轉錄。",
                model_path.display()
            )));
            models.spawn_select_or_download(id_lower, req.whisper_params.clone(), true);
            return;
        }

        let engine = match WhisperEngine::load(&model_path, &req.whisper_params) {
            Ok(e) => e,
            Err(e) => {
                notifier.send(CoreEvent::FileFailed(format!("模型載入失敗: {e}")));
                return;
            }
        };

        notifier.send(CoreEvent::FileProgress(0.5));

        let audio_seg = AudioSegment {
            samples: decoded.samples,
            duration_ms: decoded.duration_ms,
        };

        let result = match engine.transcribe(&audio_seg, &req.whisper_params) {
            Ok(r) => r,
            Err(e) => {
                notifier.send(CoreEvent::FileFailed(format!("轉錄失敗: {e}")));
                return;
            }
        };

        let fmt = ExportFormat::from_extension(&req.output_format);
        let text = super::convert_chinese_text(
            &format_transcript(&result.segments, fmt),
            req.opencc_enabled,
            &req.conversion_mode,
        );

        notifier.send(CoreEvent::FileFinished(text));
    });
}

/// 複製到剪貼簿。
pub(crate) fn copy_text(text: String) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    }
}

/// 匯出檔名：來源檔主檔名 + 格式副檔名。
pub(crate) fn export_file_name(source_file: &str, fmt: ExportFormat) -> String {
    let stem = PathBuf::from(source_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("transcript")
        .to_string();
    format!("{}.{}", stem, fmt.extension())
}

/// 另存新檔（背景執行緒）。
pub(crate) fn spawn_export(text: String, source_file: String, format: String) {
    let fmt = ExportFormat::from_extension(&format);
    let file_name = export_file_name(&source_file, fmt);
    std::thread::spawn(move || {
        let save_path = rfd::FileDialog::new()
            .set_file_name(file_name)
            .add_filter(fmt.extension().to_uppercase().as_str(), &[fmt.extension()])
            .save_file();

        if let Some(path) = save_path {
            if let Err(e) = std::fs::write(&path, &text) {
                error!("匯出失敗: {e}");
            } else {
                info!("已匯出至: {}", path.display());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_name_uses_source_stem() {
        let fmt = ExportFormat::from_extension("srt");
        assert_eq!(
            export_file_name("C:/audio/meeting.mp3", fmt),
            format!("meeting.{}", fmt.extension())
        );
        assert_eq!(
            export_file_name("", fmt),
            format!("transcript.{}", fmt.extension())
        );
    }
}
