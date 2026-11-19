use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

use crate::audio::{
    AudioSegment, Resampler, VoiceActivityDetector, TARGET_SAMPLE_RATE, WAVEFORM_BAR_COUNT,
};
use crate::cloud::openai::{CloudSession, SessionOutput};
use crate::core::backend::RecordingRequest;
use crate::core::event::CoreEvent;
use crate::core::services::ModelRuntime;
use crate::core::Notifier;

use super::io::save_recording_wav;
use super::transcribe::{postprocess_and_output, transcribe_full_recording};

/// 雲端轉錄進行中顯示的裝置標籤。
pub(crate) const CLOUD_DEVICE_LABEL: &str = "雲端 OpenAI";
/// 停止錄音後等待 OpenAI 回傳剩餘語句的上限。
const CLOUD_FINISH_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct PipelineJob {
    pub(crate) model_id: String,
    pub(crate) model_path: std::path::PathBuf,
    pub(crate) sample_rate: u32,
    pub(crate) req: RecordingRequest,
    /// OpenAI 引擎的工作階段；`None` = 本機 whisper。
    pub(crate) cloud: Option<CloudSession>,
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
        mut cloud,
    } = job;
    let mut resampler = match Resampler::new(sample_rate) {
        Ok(r) => r,
        Err(e) => {
            error!("Resampler 初始化失敗: {e}");
            notifier.send(CoreEvent::PipelineFinished);
            return;
        }
    };
    let mut vad = VoiceActivityDetector::new(req.vad_threshold, req.vad_timeout_ms);
    if cloud.is_some() {
        notifier.send(CoreEvent::DeviceLabel(CLOUD_DEVICE_LABEL.into()));
    }

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
                    cloud.as_mut(),
                );
            }
            let _ = vad.flush();

            let recorded = if full_samples.is_empty() {
                info!("停止錄音，但沒有收到音訊樣本");
                None
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
                Some((segment, wav_path))
            };

            match cloud.as_mut() {
                Some(session) => finish_cloud(
                    session,
                    &req,
                    &notifier,
                    &model_runtime,
                    (model_id.clone(), model_path.clone()),
                    recorded,
                ),
                None => {
                    if let Some((segment, wav_path)) = recorded {
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
                }
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
                cloud.as_mut(),
            );
        }

        // 雲端：取回已完成的語句（不阻塞）
        if let Some(session) = cloud.as_mut() {
            let outputs = session.poll(Duration::ZERO);
            handle_cloud_outputs(outputs, &req, &notifier, true);
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

/// 停止錄音後的雲端收尾：最後 commit + 有限等待；完全沒有輸出且發生錯誤時，若本機模型存在就改用本機轉錄。
fn finish_cloud(
    session: &mut CloudSession,
    req: &RecordingRequest,
    notifier: &Notifier,
    runtime: &ModelRuntime,
    (model_id, model_path): (String, std::path::PathBuf),
    recorded: Option<(AudioSegment, Option<std::path::PathBuf>)>,
) {
    if recorded.is_some() && !session.is_dead() {
        notifier.send(CoreEvent::TranscribeStarted);
    }
    let outputs = session.finish(CLOUD_FINISH_TIMEOUT);
    handle_cloud_outputs(outputs, req, notifier, false);

    if !session.delivered_any() {
        match recorded {
            Some((segment, wav_path)) if session.had_error() && model_path.exists() => {
                warn!("雲端轉錄失敗，改用本機模型 {model_id}");
                notifier.send(CoreEvent::Status(
                    "雲端轉錄失敗，改用本機模型轉錄...".into(),
                ));
                transcribe_full_recording(
                    runtime, model_id, model_path, req, segment, notifier, wav_path,
                );
            }
            // 清掉「轉錄中」；錯誤訊息（若有）保留在狀態列
            Some(_) => notifier.send(CoreEvent::TranscriptEmpty),
            None => {}
        }
    }

    // 恢復本機裝置標籤
    let label = runtime
        .cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|m| m.engine.device_label.clone())
        .unwrap_or_else(|| "CPU".into());
    notifier.send(CoreEvent::DeviceLabel(label));
}

/// 暫時文字也做簡繁轉換（雲端回傳多為簡體）；每個錄音執行緒只建立一次轉換器。
fn convert_partial(text: &str, req: &RecordingRequest) -> String {
    use crate::transcription::pipeline::TranscriptionPipeline;
    thread_local! {
        static PIPE: std::cell::RefCell<Option<(bool, String, Option<TranscriptionPipeline>)>> =
            const { std::cell::RefCell::new(None) };
    }
    PIPE.with(|cell| {
        let mut cell = cell.borrow_mut();
        let stale = !matches!(&*cell, Some((on, mode, _)) if *on == req.opencc_enabled && *mode == req.conversion_mode);
        if stale {
            let pipe = TranscriptionPipeline::for_live(req.opencc_enabled, &req.conversion_mode).ok();
            *cell = Some((req.opencc_enabled, req.conversion_mode.clone(), pipe));
        }
        match cell.as_ref().and_then(|(_, _, p)| p.as_ref()) {
            Some(p) if req.opencc_enabled => p.process_text(text, None),
            _ => text.to_string(),
        }
    })
}

/// 把工作階段結果送到畫面：暫時文字 → `Partial`；完成語句 → 與本機相同的後處理與輸出（逐句）。
fn handle_cloud_outputs(
    outputs: Vec<SessionOutput>,
    req: &RecordingRequest,
    notifier: &Notifier,
    recording: bool,
) {
    for output in outputs {
        match output {
            SessionOutput::Partial(text) => {
                notifier.send(CoreEvent::Partial(convert_partial(&text, req)))
            }
            SessionOutput::Final(text) => {
                let status = if recording {
                    "● 錄音中...（OpenAI）"
                } else {
                    "已轉錄（OpenAI）"
                };
                postprocess_and_output(&text, req, notifier, status.to_string());
            }
            SessionOutput::Failed(msg) => {
                warn!("{msg}");
                notifier.send(CoreEvent::TranscriptionFailed(msg));
            }
        }
    }
}

fn process_recording_chunk(
    chunk: Vec<f32>,
    resampler: &mut Resampler,
    vad: &mut VoiceActivityDetector,
    full_samples: &mut Vec<f32>,
    notifier: &Notifier,
    mut cloud: Option<&mut CloudSession>,
) {
    let resampled = match resampler.process(&chunk) {
        Ok(r) => r,
        Err(e) => {
            error!("重採樣失敗: {e}");
            return;
        }
    };

    full_samples.extend_from_slice(&resampled);
    if let Some(session) = cloud.as_deref_mut() {
        session.push_audio(&resampled);
    }

    for event in vad.process(&resampled) {
        use crate::audio::VadEvent;
        match event {
            VadEvent::SpeechStart => notifier.send(CoreEvent::SpeechStarted),
            VadEvent::Segment(segment) => match cloud.as_deref_mut() {
                Some(session) => {
                    // 本機 VAD 段落結束 → 這段語音交給 OpenAI 轉錄
                    session.commit();
                }
                None => info!(
                    "VAD 偵測到語音段落，長度 {}ms；PTT 模式保留至停止後整段轉錄",
                    segment.duration_ms
                ),
            },
            VadEvent::Silence => notifier.send(CoreEvent::Silence),
            VadEvent::SpeechContinue { level } => notifier.send(CoreEvent::Level(level)),
        }
    }
}
