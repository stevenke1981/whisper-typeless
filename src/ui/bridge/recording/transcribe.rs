use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

use slint::{ModelRc, VecModel};

use crate::audio::AudioSegment;
use crate::output::{InjectMethod, OutputFormatter, OutputMode, OutputRouter};
use crate::transcription::TranscriptionPipeline;
use crate::ui::bridge::{LoadedWhisperModel, ModelRuntime};
use crate::whisper::{WhisperEngine, WhisperParams};
use crate::{AppWindow, TranscriptSegmentData};

use super::io::current_time_str;

#[allow(clippy::too_many_arguments)]
pub(super) fn transcribe_full_recording(
    runtime: &ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    whisper_params: &WhisperParams,
    segment: AudioSegment,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
    privacy_mode: bool,
    ui_weak: &slint::Weak<AppWindow>,
    transcript_list: &Arc<Mutex<Vec<TranscriptSegmentData>>>,
    wav_path: Option<PathBuf>,
) {
    {
        let mut cache = runtime.cache.lock().unwrap();
        if let Some(loaded) = cache
            .as_ref()
            .filter(|loaded| loaded.model_id == model_id && loaded.model_path == model_path)
        {
            info!("使用已預載模型轉錄完整錄音: {}", model_id);
            transcribe_and_update(
                &loaded.engine,
                segment,
                whisper_params,
                clipboard_enabled,
                inject_enabled,
                opencc_enabled,
                conversion_mode.clone(),
                privacy_mode,
                ui_weak,
                transcript_list,
                wav_path,
            );
            return;
        }

        cache.take();
    }

    {
        let u = ui_weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_status_text("模型尚未預載完成，正在載入...".into());
                ui.set_is_processing(true);
            }
        });
    }

    let engine = match WhisperEngine::load(&model_path, whisper_params) {
        Ok(engine) => engine,
        Err(e) => {
            let msg = format!("模型載入失敗: {e}");
            error!("{msg}");
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_status_text(msg.into());
                    ui.set_is_processing(false);
                }
            });
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
        transcribe_and_update(
            &loaded.engine,
            segment,
            whisper_params,
            clipboard_enabled,
            inject_enabled,
            opencc_enabled,
            conversion_mode,
            privacy_mode,
            ui_weak,
            transcript_list,
            wav_path,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn transcribe_and_update(
    engine: &WhisperEngine,
    segment: AudioSegment,
    whisper_params: &WhisperParams,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
    privacy_mode: bool,
    ui_weak: &slint::Weak<AppWindow>,
    transcript_list: &Arc<Mutex<Vec<TranscriptSegmentData>>>,
    wav_path: Option<PathBuf>,
) {
    let u = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = u.upgrade() {
            ui.set_current_partial("⏳ 轉錄中...".into());
            ui.set_is_processing(true);
        }
    });

    match engine.transcribe(&segment, whisper_params) {
        Ok(result) => {
            let text = match TranscriptionPipeline::for_live(opencc_enabled, &conversion_mode) {
                Ok(pipeline) => pipeline.process_text(&result.text, None),
                Err(e) => {
                    error!("OpenCC 轉換初始化失敗: {e}");
                    result.text.trim().to_string()
                }
            };
            info!("轉錄結果: {:?}（{} 字）", text, text.chars().count());

            if text.is_empty() {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_current_partial("".into());
                        ui.set_is_processing(false);
                    }
                });
                return;
            }

            // 剪貼簿 / 鍵入 / 隱私清除皆由 OutputRouter 處理；
            // 即時路徑沿用 enigo.text 直接鍵入、無額外延遲。失敗時 router 會記 warn。
            let mut router = OutputRouter::new(
                OutputMode::from_flags(clipboard_enabled, inject_enabled),
                OutputFormatter::passthrough(),
            )
            .with_inject_method(InjectMethod::Type)
            .with_inject_delay_ms(0)
            .with_privacy_mode(privacy_mode);
            let _ = router.send(&text);

            let ts = current_time_str();
            let seg = TranscriptSegmentData {
                text: text.clone().into(),
                timestamp: ts.into(),
            };

            let mut list = transcript_list.lock().unwrap();
            list.push(seg);
            let entries: Vec<TranscriptSegmentData> = list.clone();
            let char_total: i32 = list.iter().map(|s| s.text.chars().count() as i32).sum();
            drop(list);

            let u = ui_weak.clone();
            let status = wav_path
                .as_ref()
                .map(|p| format!("已轉錄，錄音保留於 {}", p.display()))
                .unwrap_or_else(|| "已轉錄".to_string());
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_transcript_segments(ModelRc::new(VecModel::from(entries)));
                    ui.set_char_count(char_total);
                    ui.set_current_partial("".into());
                    ui.set_is_processing(false);
                    ui.set_status_text(status.into());
                }
            });
        }
        Err(e) => {
            error!("轉錄失敗: {e}");
            let msg = format!("轉錄錯誤: {e}");
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_status_text(msg.into());
                    ui.set_current_partial("".into());
                    ui.set_is_processing(false);
                }
            });
        }
    }
}
