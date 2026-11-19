use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tracing::{error, info};

use crate::audio::{
    AudioSegment, Resampler, VoiceActivityDetector, TARGET_SAMPLE_RATE, WAVEFORM_BAR_COUNT,
};
use crate::core::backend::RecordingRequest;
use crate::core::event::CoreEvent;
use crate::core::services::ModelRuntime;
use crate::core::Notifier;

use super::io::save_recording_wav;
use super::transcribe::transcribe_full_recording;

pub(crate) struct PipelineJob {
    pub(crate) model_id: String,
    pub(crate) model_path: std::path::PathBuf,
    pub(crate) sample_rate: u32,
    pub(crate) req: RecordingRequest,
}

pub(super) fn run_recording_pipeline(
    job: PipelineJob,
    audio_rx: std::sync::mpsc::Receiver<Vec<f32>>,
    level_rx: std::sync::mpsc::Receiver<f32>,
    stop_flag: Arc<AtomicBool>,
    notifier: Notifier,
    model_runtime: ModelRuntime,
) {
    let PipelineJob {
        model_id,
        model_path,
        sample_rate,
        req,
    } = job;
    let mut resampler = match Resampler::new(sample_rate) {
        Ok(r) => r,
        Err(e) => {
            error!("Resampler 初始化失敗: {e}");
            return;
        }
    };
    let mut vad = VoiceActivityDetector::new(req.vad_threshold, req.vad_timeout_ms);

    // 波形環形緩衝（20 條 bar）
    let mut bar_buf: Vec<f32> = vec![0.0f32; WAVEFORM_BAR_COUNT];
    let mut bar_idx: usize = 0;
    let mut last_wave_tick = Instant::now();
    let mut full_samples: Vec<f32> = Vec::new();

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            while let Ok(chunk) = audio_rx.try_recv() {
                process_recording_chunk(
                    chunk,
                    &mut resampler,
                    &mut vad,
                    &mut full_samples,
                    &notifier,
                );
            }
            let _ = vad.flush();

            if full_samples.is_empty() {
                info!("停止錄音，但沒有收到音訊樣本");
            } else {
                let duration_ms = (full_samples.len() as u64 * 1000) / TARGET_SAMPLE_RATE as u64;
                let wav_path = if req.privacy_mode {
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
                    &req,
                    segment,
                    &notifier,
                    wav_path,
                );
            }
            break;
        }

        while let Ok(chunk) = audio_rx.try_recv() {
            process_recording_chunk(
                chunk,
                &mut resampler,
                &mut vad,
                &mut full_samples,
                &notifier,
            );
        }

        // 更新音量等級 → 波形
        while let Ok(level) = level_rx.try_recv() {
            let normalized = (level * 4.0).min(1.0);
            bar_buf[bar_idx % WAVEFORM_BAR_COUNT] = normalized;
            bar_idx += 1;
        }

        if last_wave_tick.elapsed().as_millis() >= 80 {
            last_wave_tick = Instant::now();
            notifier.send(CoreEvent::Waveform(bar_buf.clone()));
        }

        if stop_flag.load(Ordering::SeqCst) {
            continue;
        }

        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // 清理
    info!("錄音管線結束");
    notifier.send(CoreEvent::PipelineFinished);
}

fn process_recording_chunk(
    chunk: Vec<f32>,
    resampler: &mut Resampler,
    vad: &mut VoiceActivityDetector,
    full_samples: &mut Vec<f32>,
    notifier: &Notifier,
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
            VadEvent::SpeechStart => notifier.send(CoreEvent::SpeechStarted),
            VadEvent::Segment(segment) => {
                info!(
                    "VAD 偵測到語音段落，長度 {}ms；PTT 模式保留至停止後整段轉錄",
                    segment.duration_ms
                );
            }
            VadEvent::Silence => notifier.send(CoreEvent::Silence),
            VadEvent::SpeechContinue { level } => notifier.send(CoreEvent::Level(level)),
        }
    }
}
