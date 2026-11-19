use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;
use tracing::{error, info};

use slint::{ModelRc, VecModel};

use crate::audio::{AudioSegment, Resampler, VoiceActivityDetector, TARGET_SAMPLE_RATE, WAVEFORM_BAR_COUNT};
use crate::ui::bridge::ModelRuntime;
use crate::whisper::WhisperParams;
use crate::{AppWindow, TranscriptSegmentData};

use super::io::save_recording_wav;
use super::transcribe::transcribe_full_recording;

pub(super) fn run_recording_pipeline(
    model_id: String,
    model_path: std::path::PathBuf,
    whisper_params: WhisperParams,
    sample_rate: u32,
    vad_threshold: f32,
    vad_timeout_ms: u64,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
    privacy_mode: bool,
    audio_rx: std::sync::mpsc::Receiver<Vec<f32>>,
    level_rx: std::sync::mpsc::Receiver<f32>,
    stop_flag: Arc<AtomicBool>,
    ui_weak: slint::Weak<AppWindow>,
    transcript_list: Arc<Mutex<Vec<TranscriptSegmentData>>>,
    model_runtime: ModelRuntime,
) {
    let mut resampler = match Resampler::new(sample_rate) {
        Ok(r) => r,
        Err(e) => {
            error!("Resampler 初始化失敗: {e}");
            return;
        }
    };
    let mut vad = VoiceActivityDetector::new(vad_threshold, vad_timeout_ms);

    // 波形環形緩衝（20 條 bar）
    let mut bar_buf: Vec<f32> = vec![0.0f32; WAVEFORM_BAR_COUNT];
    let mut bar_idx: usize = 0;
    let mut last_wave_tick = Instant::now();
    let mut full_samples: Vec<f32> = Vec::new();

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            while let Ok(chunk) = audio_rx.try_recv() {
                process_recording_chunk(chunk, &mut resampler, &mut vad, &mut full_samples, &ui_weak);
            }
            let _ = vad.flush();

            if full_samples.is_empty() {
                info!("停止錄音，但沒有收到音訊樣本");
            } else {
                let duration_ms = (full_samples.len() as u64 * 1000) / TARGET_SAMPLE_RATE as u64;
                let wav_path = if privacy_mode {
                    info!("隱私模式啟用，跳過錄音檔案儲存");
                    None
                } else {
                    match save_recording_wav(&full_samples, TARGET_SAMPLE_RATE) {
                        Ok(path) => {
                            info!("已保留完整錄音: {} ({}ms)", path.display(), duration_ms);
                            Some(path)
                        }
                        Err(e) => {
                            error!("保存錄音失敗: {e}");
                            None
                        }
                    }
                };
                let segment = AudioSegment {
                    samples: std::mem::take(&mut full_samples),
                    duration_ms,
                };
                transcribe_full_recording(
                    &model_runtime,
                    model_id.clone(),
                    model_path.clone(),
                    &whisper_params,
                    segment,
                    clipboard_enabled,
                    inject_enabled,
                    opencc_enabled,
                    conversion_mode.clone(),
                    privacy_mode,
                    &ui_weak,
                    &transcript_list,
                    wav_path,
                );
            }
            break;
        }

        while let Ok(chunk) = audio_rx.try_recv() {
            process_recording_chunk(chunk, &mut resampler, &mut vad, &mut full_samples, &ui_weak);
        }

        // 更新音量等級 → 波形
        while let Ok(level) = level_rx.try_recv() {
            let normalized = (level * 4.0).min(1.0);
            bar_buf[bar_idx % WAVEFORM_BAR_COUNT] = normalized;
            bar_idx += 1;
        }

        if last_wave_tick.elapsed().as_millis() >= 80 {
            last_wave_tick = Instant::now();
            let bars = bar_buf.clone();
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_waveform_bars(ModelRc::new(VecModel::from(bars)));
                }
            });
        }

        if stop_flag.load(Ordering::SeqCst) {
            continue;
        }

        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // 清理
    info!("錄音管線結束");
    let u = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = u.upgrade() {
            ui.set_waveform_bars(ModelRc::new(VecModel::from(vec![0.0f32; 20])));
            ui.set_is_processing(false);
            ui.set_audio_level(0.0);
        }
    });
}

fn process_recording_chunk(
    chunk: Vec<f32>,
    resampler: &mut Resampler,
    vad: &mut VoiceActivityDetector,
    full_samples: &mut Vec<f32>,
    ui_weak: &slint::Weak<AppWindow>,
) {
    let resampled = match resampler.process(&chunk) {
        Ok(r) => r,
        Err(e) => {
            error!("重採樣失敗: {e}");
            return;
        }
    };

    full_samples.extend_from_slice(&resampled);

    for event in vad.process(&resampled) {
        use crate::audio::VadEvent;
        match event {
            VadEvent::SpeechStart => {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_current_partial("聆聽中...".into());
                        ui.set_is_processing(true);
                    }
                });
            }
            VadEvent::Segment(segment) => {
                info!(
                    "VAD 偵測到語音段落，長度 {}ms；PTT 模式保留至停止後整段轉錄",
                    segment.duration_ms
                );
            }
            VadEvent::Silence => {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_current_partial("".into());
                    }
                });
            }
            VadEvent::SpeechContinue { level } => {
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_audio_level(level);
                    }
                });
            }
        }
    }
}
