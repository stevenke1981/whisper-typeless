use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle as TokioHandle;
use tracing::{error, info};

use slint::ComponentHandle;

use crate::audio::AudioSegment;
use crate::file_transcription::{decode_audio_file, format_transcript, ExportFormat};
use crate::settings::config::AppConfig;
use crate::whisper::WhisperEngine;
use crate::AppWindow;

use super::ModelRuntime;
use super::model::spawn_model_select_or_download;

// ─── 檔案模式 Callbacks ───────────────────────────────────────────────────────

pub fn setup_file_mode_callbacks(
    ui: &AppWindow,
    config: &AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    model_runtime: ModelRuntime,
) -> anyhow::Result<()> {
    let whisper_params = config.whisper.clone();
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let tokio_handle = TokioHandle::current();

    // 開啟檔案對話框
    let ui_handle = ui.as_weak();
    ui.on_open_file_dialog(move || {
        let ui_handle = ui_handle.clone();
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .add_filter(
                    "音訊/影片",
                    &[
                        "mp3", "mp4", "wav", "flac", "ogg", "m4a", "aac", "mkv", "mov", "avi",
                        "webm",
                    ],
                )
                .pick_file();

            if let Some(path) = result {
                let full_path = path.to_string_lossy().into_owned();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_file_name(full_path.into());
                        ui.set_file_has_result(false);
                        ui.set_file_result("".into());
                        ui.set_file_progress(0.0);
                    }
                });
            }
        });
    });

    // 開始轉錄
    let ui_handle = ui.as_weak();
    let params = whisper_params.clone();
    let mdir = models_dir.clone();
    let model_id_arc = Arc::clone(&selected_model_id);
    let runtime = model_runtime.clone();
    let cfg = config.clone();
    let tokio_handle2 = tokio_handle.clone();
    ui.on_start_file_transcription(move || {
        let ui_weak = ui_handle.clone();

        let (file_path, model_id, fmt_str, opencc_enabled, conversion_mode) = {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let full: String = ui.get_file_name().into();
            if full.is_empty() {
                return;
            }
            let model: String = ui.get_selected_model_id().into();
            let fmt: String = ui.get_file_output_format().into();
            (
                PathBuf::from(full),
                model,
                fmt,
                ui.get_mod_opencc(),
                ui.get_conversion_mode().to_string(),
            )
        };

        if !file_path.exists() {
            return;
        }

        let ui_weak2 = ui_weak.clone();
        let params2 = params.clone();
        let mdir2 = mdir.clone();
        let model_id_arc2 = Arc::clone(&model_id_arc);
        let runtime2 = runtime.clone();
        let cfg2 = cfg.clone();
        let tokio_handle3 = tokio_handle2.clone();

        std::thread::spawn(move || {
            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_processing(true);
                        ui.set_file_progress(0.05);
                    }
                });
            }

            let decoded = match decode_audio_file(&file_path) {
                Ok(d) => d,
                Err(e) => {
                    let msg = format!("解碼失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_progress(0.3);
                    }
                });
            }

            let id_lower = model_id.to_lowercase();
            let model_path = mdir2.join(format!("ggml-{}.bin", id_lower));

            if !model_path.exists() {
                let msg = format!(
                    "模型尚未下載: {}\n已開啟下載確認，下載完成後請再開始轉錄。",
                    model_path.display()
                );
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_processing(false);
                        ui.set_file_result(msg.into());
                        ui.set_file_has_result(true);
                    }
                });
                spawn_model_select_or_download(
                    tokio_handle3,
                    ui_weak2,
                    mdir2,
                    id_lower,
                    model_id_arc2,
                    runtime2,
                    cfg2,
                    params2.clone(),
                    true,
                );
                return;
            }

            let engine = match WhisperEngine::load(&model_path, &params2) {
                Ok(e) => e,
                Err(e) => {
                    let msg = format!("模型載入失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            {
                let uw = ui_weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = uw.upgrade() {
                        ui.set_file_progress(0.5);
                    }
                });
            }

            let audio_seg = AudioSegment {
                samples: decoded.samples,
                duration_ms: decoded.duration_ms,
            };

            let result = match engine.transcribe(&audio_seg, &params2) {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("轉錄失敗: {e}");
                    let uw = ui_weak2.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = uw.upgrade() {
                            ui.set_file_processing(false);
                            ui.set_file_result(msg.into());
                            ui.set_file_has_result(true);
                        }
                    });
                    return;
                }
            };

            let fmt = ExportFormat::from_extension(&fmt_str);
            let text = super::convert_chinese_text(
                &format_transcript(&result.segments, fmt),
                opencc_enabled,
                &conversion_mode,
            );

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak2.upgrade() {
                    ui.set_file_progress(1.0);
                    ui.set_file_processing(false);
                    ui.set_file_result(text.into());
                    ui.set_file_has_result(true);
                }
            });
        });
    });

    // 複製結果
    let ui_handle = ui.as_weak();
    ui.on_file_copy_result(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let text: String = ui.get_file_result().into();
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text);
            }
        }
    });

    // 匯出結果
    let ui_handle = ui.as_weak();
    ui.on_file_export_result(move |fmt_shared| {
        let fmt_str: String = fmt_shared.into();
        let fmt = ExportFormat::from_extension(&fmt_str);

        if let Some(ui) = ui_handle.upgrade() {
            let text: String = ui.get_file_result().into();
            let file_name: String = ui.get_file_name().into();

            let stem = PathBuf::from(&file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("transcript")
                .to_string();

            std::thread::spawn(move || {
                let save_path = rfd::FileDialog::new()
                    .set_file_name(format!("{}.{}", stem, fmt.extension()))
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
    });

    Ok(())
}
