pub(crate) mod io;
pub(crate) mod pipeline;
pub(crate) mod transcribe;

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::runtime::Handle as TokioHandle;
use tracing::{error, info};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::audio::{AudioCapture, WAVEFORM_BAR_COUNT};
use crate::settings::config::AppConfig;
use crate::ui::bridge::model::{spawn_model_select_or_download, whisper_params_from_ui};
use crate::ui::bridge::ModelRuntime;
use crate::{AppWindow, TranscriptSegmentData};

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
                        ui.set_waveform_bars(ModelRc::new(VecModel::from(vec![
                            0.0f32;
                            WAVEFORM_BAR_COUNT
                        ])));
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

                // 讀取輸出設定（從 UI 讀取當前狀態；UI 為設定的即時快照）
                let (
                    privacy_mode,
                    vad_threshold,
                    vad_timeout,
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
                            ui.get_privacy_mode(),
                            ui.get_vad_threshold(),
                            ui.get_silence_timeout().max(0.0).round() as u64,
                            ui.get_clipboard_enabled(),
                            ui.get_inject_enabled(),
                            ui.get_use_gpu(),
                            ui.get_n_threads(),
                            ui.get_temperature(),
                            ui.get_mod_opencc(),
                            ui.get_conversion_mode().to_string(),
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            cfg.output.privacy_mode,
                            cfg.audio.vad_threshold,
                            cfg.audio.vad_silence_timeout_ms,
                            true,
                            true,
                            cfg.whisper.use_gpu,
                            cfg.whisper.n_threads,
                            cfg.whisper.temperature,
                            cfg.modules.opencc,
                            crate::settings::ui_settings::conversion_mode_to_str(
                                &cfg.chinese.conversion,
                            )
                            .to_string(),
                        )
                    });

                let stop_flag = Arc::new(AtomicBool::new(false));
                let stop_flag_thread = Arc::clone(&stop_flag);
                let mut whisper_params = cfg.whisper.clone();
                whisper_params.use_gpu = ui_use_gpu;
                whisper_params.n_threads = ui_n_threads;
                whisper_params.temperature = ui_temperature;
                let ui_thread = ui_ref.clone();
                let transcript_thread = Arc::clone(&transcript_arc);
                let runtime_thread = runtime.clone();

                std::thread::spawn(move || {
                    pipeline::run_recording_pipeline(
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
                        privacy_mode,
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
