use std::path::PathBuf;
use std::sync::{atomic::Ordering, Arc, Mutex};
use tokio::runtime::Handle as TokioHandle;
use tracing::{error, info};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::models::registry::ModelInfo;
use crate::models::{ModelDownloader, ModelManager};
use crate::settings::config::AppConfig;
use crate::whisper::{WhisperEngine, WhisperParams};
use crate::{AppWindow, ModelEntry};

use super::ModelRuntime;

// ─── 模型列表 ─────────────────────────────────────────────────────────────────

pub(super) fn model_entries_from(models: &[ModelInfo], _selected_id: &str) -> Vec<ModelEntry> {
    models
        .iter()
        .map(|m| ModelEntry {
            id: m.id.as_str().into(),
            display_name: m.display_name.as_str().into(),
            is_downloaded: m.is_downloaded,
            size_mb: m.size.disk_mb as i32,
            quality: m.quality_stars as i32,
            download_url: m.download_url.as_str().into(),
        })
        .collect()
}

pub(super) async fn setup_model_entries(ui: &AppWindow, _config: &AppConfig) -> anyhow::Result<()> {
    let models_dir = AppConfig::models_dir()?;
    let manager = ModelManager::new(models_dir);
    let models = manager.list_available().await;

    let selected = ui.get_selected_model_id().to_string();
    let entries = model_entries_from(&models, &selected);
    ui.set_model_entries(ModelRc::new(VecModel::from(entries)));

    if let Some(info) = models.iter().find(|m| m.id == selected) {
        ui.set_model_name(info.display_name.as_str().into());
    }
    Ok(())
}

pub(super) fn whisper_params_from_ui(ui: &AppWindow, config: &AppConfig) -> WhisperParams {
    let mut params = config.whisper.clone();
    params.use_gpu = ui.get_use_gpu();
    params.n_threads = ui.get_n_threads();
    params.temperature = ui.get_temperature();
    params
}

// ─── 模型預載 ─────────────────────────────────────────────────────────────────

pub(super) fn preload_selected_model(
    ui: &AppWindow,
    config: &AppConfig,
    runtime: &ModelRuntime,
    model_id: String,
) -> bool {
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let model_path = models_dir.join(format!("ggml-{model_id}.bin"));
    if !model_path.exists() {
        info!("略過模型預載，檔案不存在: {}", model_path.display());
        return false;
    }
    spawn_model_preload(
        runtime.clone(),
        model_id,
        model_path,
        whisper_params_from_ui(ui, config),
        ui.as_weak(),
    );
    true
}

pub(super) fn spawn_model_preload(
    runtime: ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    whisper_params: WhisperParams,
    ui_weak: slint::Weak<AppWindow>,
) {
    let generation = runtime.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let label = model_id.clone();

    {
        let u = ui_weak.clone();
        let status = format!("預載模型 {label}...");
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                if !ui.get_is_recording() {
                    ui.set_status_text(status.into());
                }
            }
        });
    }

    std::thread::spawn(move || {
        info!("背景預載模型: {} ({})", model_id, model_path.display());
        match WhisperEngine::load(&model_path, &whisper_params) {
            Ok(engine) => {
                if runtime.generation.load(Ordering::SeqCst) != generation {
                    info!("丟棄過期模型預載結果: {}", model_id);
                    return;
                }

                let device = engine.device_label.clone();
                {
                    let mut cache = runtime.cache.lock().unwrap();
                    *cache = Some(super::LoadedWhisperModel {
                        model_id: model_id.clone(),
                        model_path: model_path.clone(),
                        engine,
                    });
                }

                let u = ui_weak.clone();
                let status = format!("模型已預載: {model_id}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_device_label(device.into());
                        if !ui.get_is_recording() {
                            ui.set_status_text(status.into());
                        }
                    }
                });
            }
            Err(e) => {
                error!("模型預載失敗: {e}");
                let u = ui_weak.clone();
                let msg = format!("模型預載失敗: {e}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        if !ui.get_is_recording() {
                            ui.set_status_text(msg.into());
                        }
                    }
                });
            }
        }
    });
}

// ─── 模型選擇 / 下載 ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_model_select_or_download(
    tokio_handle: TokioHandle,
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    model_id: String,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    _config: AppConfig,
    whisper_params: WhisperParams,
    confirm_if_missing: bool,
) {
    tokio_handle.spawn(async move {
        let manager = ModelManager::new(models_dir.clone());
        let models = manager.list_available().await;

        let Some(info) = models.iter().find(|m| m.id == model_id).cloned() else {
            let msg = format!("未知模型: {model_id}");
            let u = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_status_text(msg.into());
                }
            });
            return;
        };

        if info.is_downloaded {
            select_downloaded_model(
                ui_weak,
                models_dir,
                model_id,
                selected_model_id,
                runtime,
                whisper_params,
                models,
                "模型已切換",
            );
            return;
        }

        if confirm_if_missing {
            let display_name = info.display_name.clone();
            let size_mb = info.size.disk_mb;
            let result = rfd::AsyncMessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("模型尚未下載")
                .set_description(format!(
                    "語音辨識模型 {display_name} 尚未下載。\n\n大小約 {size_mb} MB，是否現在自動下載？\n下載完成後會自動選用並預載。"
                ))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                .await;

            if result != rfd::MessageDialogResult::Yes {
                let msg = format!("已取消下載模型 {display_name}");
                let u = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = u.upgrade() {
                        ui.set_status_text(msg.into());
                    }
                });
                return;
            }
        }

        download_model_and_select(
            ui_weak,
            models_dir,
            info,
            selected_model_id,
            runtime,
            whisper_params,
        )
        .await;
    });
}

#[allow(clippy::too_many_arguments)]
fn select_downloaded_model(
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    model_id: String,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    whisper_params: WhisperParams,
    models: Vec<ModelInfo>,
    status: &'static str,
) {
    *selected_model_id.lock().unwrap() = model_id.clone();

    let display = models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| model_id.clone());
    let entries = model_entries_from(&models, &model_id);
    let model_path = models_dir.join(format!("ggml-{model_id}.bin"));
    let ui_for_preload = ui_weak.clone();
    let id_for_ui = model_id.clone();

    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_downloading_id("".into());
            ui.set_download_progress(0.0);
            ui.set_selected_model_id(id_for_ui.into());
            ui.set_model_name(display.into());
            ui.set_status_text(status.into());
            ui.set_model_entries(ModelRc::new(VecModel::from(entries)));
        }
    });

    spawn_model_preload(
        runtime,
        model_id,
        model_path,
        whisper_params,
        ui_for_preload,
    );
}

#[allow(clippy::too_many_arguments)]
async fn download_model_and_select(
    ui_weak: slint::Weak<AppWindow>,
    models_dir: PathBuf,
    info: ModelInfo,
    selected_model_id: Arc<Mutex<String>>,
    runtime: ModelRuntime,
    whisper_params: WhisperParams,
) {
    let model_id = info.id.clone();
    let size_mb = info.size.disk_mb;

    if runtime
        .download_in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        let u = ui_weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_status_text("已有模型正在下載，請稍候...".into());
            }
        });
        return;
    }

    {
        let u = ui_weak.clone();
        let id_show = model_id.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = u.upgrade() {
                ui.set_downloading_id(id_show.into());
                ui.set_download_progress(0.0);
                ui.set_status_text(format!("下載模型 ({size_mb} MB)...").into());
            }
        });
    }

    let downloader = ModelDownloader::new(models_dir.clone());
    let (tx, mut rx) =
        tokio::sync::mpsc::channel::<crate::models::downloader::DownloadProgress>(64);

    let ui_prog = ui_weak.clone();
    tokio::spawn(async move {
        while let Some(prog) = rx.recv().await {
            let ratio = prog
                .total_bytes
                .map(|t| prog.downloaded_bytes as f32 / t as f32)
                .unwrap_or(0.5_f32);
            let u = ui_prog.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = u.upgrade() {
                    ui.set_download_progress(ratio);
                }
            });
        }
    });

    match downloader.download(&info, tx).await {
        Ok(_) => {
            info!("模型 {model_id} 下載完成");
            let manager = ModelManager::new(models_dir.clone());
            let updated = manager.list_available().await;
            select_downloaded_model(
                ui_weak,
                models_dir,
                model_id,
                selected_model_id,
                runtime.clone(),
                whisper_params,
                updated,
                "模型下載完成，已選用",
            );
        }
        Err(e) => {
            error!("模型下載失敗: {e}");
            let msg = format!("下載失敗: {e}");
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_downloading_id("".into());
                    ui.set_download_progress(0.0);
                    ui.set_status_text(msg.into());
                }
            });
        }
    }
    runtime.download_in_progress.store(false, Ordering::SeqCst);
}

// ─── 模型 Callbacks ───────────────────────────────────────────────────────────

pub(super) fn setup_model_callbacks(
    ui: &AppWindow,
    config: &AppConfig,
    selected_model_id: Arc<Mutex<String>>,
    model_runtime: ModelRuntime,
) -> anyhow::Result<()> {
    let models_dir = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    let ui_weak = ui.as_weak();
    let model_id_arc = Arc::clone(&selected_model_id);
    let config = config.clone();
    let tokio_handle = TokioHandle::current();

    ui.on_model_select_or_download(move |model_id_shared| {
        let params = ui_weak
            .upgrade()
            .map(|ui| whisper_params_from_ui(&ui, &config))
            .unwrap_or_else(|| config.whisper.clone());
        spawn_model_select_or_download(
            tokio_handle.clone(),
            ui_weak.clone(),
            models_dir.clone(),
            model_id_shared.to_string(),
            Arc::clone(&model_id_arc),
            model_runtime.clone(),
            config.clone(),
            params,
            false,
        );
    });

    // 開啟模型資料夾
    let mdir2 = AppConfig::models_dir().unwrap_or_else(|_| PathBuf::from("."));
    ui.on_open_model_manager(move || {
        let dir = mdir2.clone();
        std::thread::spawn(move || {
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        });
    });

    Ok(())
}
