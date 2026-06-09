use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::runtime::Handle as TokioHandle;
use tracing::{error, info};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::audio::{AudioCapture, Resampler, VadEvent, VoiceActivityDetector};
use crate::settings::config::AppConfig;
use crate::whisper::{WhisperEngine, WhisperParams};
use crate::{AppWindow, TranscriptSegmentData};

use super::{LoadedWhisperModel, ModelRuntime};
use super::convert_chinese_text;
use super::{spawn_model_select_or_download, whisper_params_from_ui};

// ─── 錄音控制句柄 ─────────────────────────────────────────────────────────────

pub(crate) struct RecordingHandle {
    _stream: cpal::Stream, // 保持 CPAL 串流存活
    stop_flag: Arc<AtomicBool>,
}

// cpal::Stream 在所有主流平台實作 Send
unsafe impl Send for RecordingHandle {}

// ─── 錄音控制 ─────────────────────────────────────────────────────────────────

pub fn setup_recording_callbacks(
    ui: &AppWindow,
    config: AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
    transcript_list: Arc<Mutex<Vec<TranscriptSegmentData>>>,
    model_runtime: ModelRuntime,
) {
    let ui_weak = ui.as_weak();
    let tokio_handle = TokioHandle::current();

    // ── toggle-recording ─────────────────────────────────────────────────────
    ui.on_toggle_recording({
        let handle_arc = Arc::clone(&recording_handle);
        let model_id_arc = Arc::clone(&selected_model_id);
        let transcript_arc = Arc::clone(&transcript_list);
        let ui_ref = ui_weak.clone();
        let cfg = config.clone();
        let runtime = model_runtime.clone();
        let tokio_handle = tokio_handle.clone();

        move || {
            let mut handle = handle_arc.lock().unwrap();

            if handle.is_some() {
                // ══ 停止錄音 ══════════════════════════════════════
                let h = handle.take().unwrap();
                h.stop_flag.store(true, Ordering::SeqCst);
                // drop h → _stream dropped → CPAL 停止音訊

                crate::ui::window_icon::update(false);
                let u = ui_ref.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_is_recording(false);
                        ui.set_status_text("就緒".into());
                        ui.set_audio_level(0.0);
                        ui.set_is_processing(false);
                        ui.set_current_partial("".into());
                        ui.set_waveform_bars(ModelRc::new(VecModel::from(vec![0.0f32; 20])));
                    }
                });
            } else {
                // ══ 開始錄音 ══════════════════════════════════════
                let model_id = model_id_arc.lock().unwrap().clone();
                let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
                let model_path = models_dir.join(format!("ggml-{}.bin", model_id));

                info!("嘗試啟動錄音，模型: {}", model_id);
                if !model_path.exists() {
                    let msg = format!("模型 {model_id} 尚未下載，等待使用者確認下載");
                    error!("{msg}");
                    let u = ui_ref.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = u.upgrade() {
                            ui.set_status_text("模型尚未下載，請確認是否自動下載".into());
                        }
                    });
                    spawn_model_select_or_download(
                        tokio_handle.clone(),
                        ui_ref.clone(),
                        models_dir,
                        model_id,
                        Arc::clone(&model_id_arc),
                        runtime.clone(),
                        cfg.clone(),
                        ui_ref
                            .upgrade()
                            .map(|ui| whisper_params_from_ui(&ui, &cfg))
                            .unwrap_or_else(|| cfg.whisper.clone()),
                        true,
                    );
                    return;
                }

                // 建立音訊捕捉
                let capture = match AudioCapture::new(cfg.audio.device_name.as_deref()) {
                    Ok(c) => c,
                    Err(e) => {
                        let u = ui_ref.clone();
                        let msg = format!("音訊裝置錯誤: {e}");
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = u.upgrade() {
                                ui.set_status_text(msg.into());
                            }
                        });
                        return;
                    }
                };
                let sample_rate = capture.sample_rate();

                // tokio 通道（供 cpal 回呼使用 try_send）
                let (audio_tok_tx, audio_tok_rx) = tokio::sync::mpsc::channel::<Vec<f32>>(128);
                let (level_tok_tx, level_tok_rx) = tokio::sync::mpsc::channel::<f32>(128);

                // std 通道（供處理執行緒使用 try_recv / blocking）
                let (audio_std_tx, audio_std_rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(64);
                let (level_std_tx, level_std_rx) = std::sync::mpsc::sync_channel::<f32>(64);

                // 橋接：tokio → std
                // buffer 滿時丟棄 chunk（模型載入期間）而非退出，否則管線會提早結束
                tokio::spawn(async move {
                    let mut rx = audio_tok_rx;
                    while let Some(data) = rx.recv().await {
                        let _ = audio_std_tx.try_send(data);
                    }
                });
                tokio::spawn(async move {
                    let mut rx = level_tok_rx;
                    while let Some(lvl) = rx.recv().await {
                        let _ = level_std_tx.try_send(lvl);
                    }
                });

                // 啟動 CPAL 串流
                let stream = match capture.start(audio_tok_tx, level_tok_tx) {
                    Ok(s) => s,
                    Err(e) => {
                        let u = ui_ref.clone();
                        let msg = format!("無法啟動錄音串流: {e}");
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = u.upgrade() {
                                ui.set_status_text(msg.into());
                            }
                        });
                        return;
                    }
                };

                // 讀取輸出設定（從 UI 讀取當前狀態）
                let (
                    clipboard_on,
                    inject_on,
                    ui_use_gpu,
                    ui_n_threads,
                    ui_temperature,
                    opencc_enabled,
                    conversion_mode,
                ) = ui_ref
                    .upgrade()
                    .map(|ui| {
                        (
                            ui.get_clipboard_enabled(),
                            ui.get_inject_enabled(),
                            ui.get_use_gpu(),
                            ui.get_n_threads(),
                            ui.get_temperature(),
                            ui.get_mod_opencc(),
                            ui.get_conversion_mode().to_string(),
                        )
                    })
                    .unwrap_or((true, true, true, 4, 0.0, true, "zh-TW".to_string()));

                let stop_flag = Arc::new(AtomicBool::new(false));
                let stop_flag_thread = Arc::clone(&stop_flag);
                let mut whisper_params = cfg.whisper.clone();
                whisper_params.use_gpu = ui_use_gpu;
                whisper_params.n_threads = ui_n_threads;
                whisper_params.temperature = ui_temperature;
                let vad_threshold = cfg.audio.vad_threshold;
                let vad_timeout = cfg.audio.vad_silence_timeout_ms;
                let ui_thread = ui_ref.clone();
                let transcript_thread = Arc::clone(&transcript_arc);
                let runtime_thread = runtime.clone();

                std::thread::spawn(move || {
                    run_recording_pipeline(
                        model_id,
                        model_path,
                        whisper_params,
                        sample_rate,
                        vad_threshold,
                        vad_timeout,
                        clipboard_on,
                        inject_on,
                        opencc_enabled,
                        conversion_mode,
                        audio_std_rx,
                        level_std_rx,
                        stop_flag_thread,
                        ui_thread,
                        transcript_thread,
                        runtime_thread,
                    );
                });

                *handle = Some(RecordingHandle {
                    _stream: stream,
                    stop_flag,
                });

                crate::ui::window_icon::update(true);
                let u = ui_ref.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_is_recording(true);
                        ui.set_status_text("● 錄音中...".into());
                    }
                });
            }
        }
    });

    // ── clear-transcript ──────────────────────────────────────────────────────
    let ui_weak2 = ui_weak.clone();
    ui.on_clear_transcript(move || {
        let mut list = transcript_list.lock().unwrap();
        list.clear();
        let u = ui_weak2.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_transcript_segments(ModelRc::new(VecModel::from(Vec::<
                    TranscriptSegmentData,
                >::new())));
                ui.set_char_count(0);
            }
        });
    });
}

// ─── 錄音處理管線（獨立執行緒）────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_recording_pipeline(
    model_id: String,
    model_path: PathBuf,
    whisper_params: WhisperParams,
    sample_rate: u32,
    vad_threshold: f32,
    vad_timeout_ms: u64,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
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
    let mut bar_buf: Vec<f32> = vec![0.0f32; 20];
    let mut bar_idx: usize = 0;
    let mut last_wave_tick = std::time::Instant::now();
    let mut full_samples: Vec<f32> = Vec::new();

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            while let Ok(chunk) = audio_rx.try_recv() {
                process_recording_chunk(
                    chunk,
                    &mut resampler,
                    &mut vad,
                    &mut full_samples,
                    &ui_weak,
                );
            }
            let _ = vad.flush();

            if full_samples.is_empty() {
                info!("停止錄音，但沒有收到音訊樣本");
            } else {
                let duration_ms = (full_samples.len() as u64 * 1000) / 16_000;
                let wav_path = match save_recording_wav(&full_samples, 16_000) {
                    Ok(path) => {
                        info!("已保留完整錄音: {} ({}ms)", path.display(), duration_ms);
                        Some(path)
                    }
                    Err(e) => {
                        error!("保存錄音失敗: {e}");
                        None
                    }
                };
                let segment = crate::audio::AudioSegment {
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
            bar_buf[bar_idx % 20] = normalized;
            bar_idx += 1;
        }

        if last_wave_tick.elapsed().as_millis() >= 80 {
            last_wave_tick = std::time::Instant::now();
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

#[allow(clippy::too_many_arguments)]
fn transcribe_full_recording(
    runtime: &ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    whisper_params: &WhisperParams,
    segment: crate::audio::AudioSegment,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
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
            ui_weak,
            transcript_list,
            wav_path,
        );
    }
}

fn save_recording_wav(samples: &[f32], sample_rate: u32) -> anyhow::Result<PathBuf> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recordings");
    std::fs::create_dir_all(&base)?;

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = base.join(format!("ptt-{millis}.wav"));
    let mut file = std::fs::File::create(&path)?;

    let data_len = samples.len() as u32 * 2;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;

    for sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        file.write_all(&pcm.to_le_bytes())?;
    }

    Ok(path)
}

#[allow(clippy::too_many_arguments)]
fn transcribe_and_update(
    engine: &WhisperEngine,
    segment: crate::audio::AudioSegment,
    whisper_params: &WhisperParams,
    clipboard_enabled: bool,
    inject_enabled: bool,
    opencc_enabled: bool,
    conversion_mode: String,
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
            let raw_text = result.text.trim().to_string();
            let text = convert_chinese_text(&raw_text, opencc_enabled, &conversion_mode);
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

            if clipboard_enabled {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(&text);
                }
            }
            if inject_enabled {
                inject_text_to_focused_window(&text);
            }

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

fn current_time_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    // 轉換為 UTC+8（台灣時間）
    let h_tw = (h + 8) % 24;
    format!("{:02}:{:02}:{:02}", h_tw, m, s)
}

fn inject_text_to_focused_window(text: &str) {
    use enigo::{Enigo, Keyboard, Settings};
    match Enigo::new(&Settings::default()) {
        Ok(mut enigo) => {
            let _ = enigo.text(text);
        }
        Err(e) => {
            error!("enigo 初始化失敗: {e}");
        }
    }
}
