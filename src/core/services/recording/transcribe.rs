use std::path::PathBuf;
use tracing::{error, info};

use crate::audio::AudioSegment;
use crate::core::backend::RecordingRequest;
use crate::core::event::CoreEvent;
use crate::core::services::{LoadedWhisperModel, ModelRuntime};
use crate::core::Notifier;
use crate::output::{InjectMethod, OutputFormatter, OutputMode, OutputRouter};
use crate::transcription::TranscriptionPipeline;
use crate::whisper::WhisperEngine;

use super::io::current_time_str;

pub(super) fn transcribe_full_recording(
    runtime: &ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    req: &RecordingRequest,
    segment: AudioSegment,
    notifier: &Notifier,
    wav_path: Option<PathBuf>,
) {
    {
        let mut cache = runtime.cache.lock().unwrap();
        if let Some(loaded) = cache
            .as_ref()
            .filter(|loaded| loaded.model_id == model_id && loaded.model_path == model_path)
        {
            info!("使用已預載模型轉錄完整錄音: {}", model_id);
            transcribe_and_output(&loaded.engine, segment, req, notifier, wav_path);
            return;
        }

        cache.take();
    }

    notifier.send(CoreEvent::ModelLoading);

    let engine = match WhisperEngine::load(&model_path, &req.whisper_params) {
        Ok(engine) => engine,
        Err(e) => {
            let msg = format!("模型載入失敗: {e}");
            error!("{msg}");
            notifier.send(CoreEvent::ModelLoadFailed(msg));
            return;
        }
    };

    let mut cache = runtime.cache.lock().unwrap();
    *cache = Some(LoadedWhisperModel {
        model_id,
        model_path,
        engine,
    });
    if let Some(loaded) = cache.as_ref() {
        transcribe_and_output(&loaded.engine, segment, req, notifier, wav_path);
    }
}

fn transcribe_and_output(
    engine: &WhisperEngine,
    segment: AudioSegment,
    req: &RecordingRequest,
    notifier: &Notifier,
    wav_path: Option<PathBuf>,
) {
    notifier.send(CoreEvent::TranscribeStarted);

    match engine.transcribe(&segment, &req.whisper_params) {
        Ok(result) => {
            let text =
                match TranscriptionPipeline::for_live(req.opencc_enabled, &req.conversion_mode) {
                    Ok(pipeline) => pipeline.process_text(&result.text, None),
                    Err(e) => {
                        error!("OpenCC 轉換初始化失敗: {e}");
                        result.text.trim().to_string()
                    }
                };
            info!("轉錄結果: {:?}（{} 字）", text, text.chars().count());

            if text.is_empty() {
                notifier.send(CoreEvent::TranscriptEmpty);
                return;
            }

            // 剪貼簿 / 鍵入 / 隱私清除皆由 OutputRouter 處理；
            // 即時路徑沿用 enigo.text 直接鍵入、無額外延遲。失敗時 router 會記 warn。
            let mut router = OutputRouter::new(
                OutputMode::from_flags(req.clipboard_enabled, req.inject_enabled),
                OutputFormatter::passthrough(),
            )
            .with_inject_method(InjectMethod::Type)
            .with_inject_delay_ms(0)
            .with_privacy_mode(req.privacy_mode);
            let _ = router.send(&text);

            notifier.send(CoreEvent::TranscriptSegment {
                text,
                timestamp: current_time_str(),
                status: transcribed_status(wav_path.as_ref()),
            });
        }
        Err(e) => {
            error!("轉錄失敗: {e}");
            notifier.send(CoreEvent::TranscriptionFailed(format!("轉錄錯誤: {e}")));
        }
    }
}

/// 轉錄完成後的狀態列文字（隱私模式不保留錄音檔時不提路徑）。
pub(crate) fn transcribed_status(wav_path: Option<&PathBuf>) -> String {
    wav_path
        .map(|p| format!("已轉錄，錄音保留於 {}", p.display()))
        .unwrap_or_else(|| "已轉錄".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mentions_wav_only_when_kept() {
        assert_eq!(transcribed_status(None), "已轉錄");
        let p = PathBuf::from("rec.wav");
        assert!(transcribed_status(Some(&p)).contains("rec.wav"));
    }
}
