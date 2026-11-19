//! 模型清單、背景預載、選用與下載（UI 無關）。

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tracing::{error, info};

use crate::core::event::CoreEvent;
use crate::core::state::ModelEntryView;
use crate::models::registry::ModelInfo;
use crate::models::{ModelDownloader, ModelManager};
use crate::whisper::{WhisperEngine, WhisperParams};

use super::{LoadedWhisperModel, ModelContext, ModelRuntime};
use crate::core::Notifier;

// ─── 模型列表 ─────────────────────────────────────────────────────────────────

/// 清單排序：建議的小型模型（tiny / base / small…）置頂、由小到大；其餘維持 catalog 順序。
pub(crate) fn ordered_for_display(models: &[ModelInfo]) -> Vec<&ModelInfo> {
    let mut rec: Vec<&ModelInfo> = models.iter().filter(|m| m.recommended).collect();
    rec.sort_by_key(|m| m.size.disk_mb);
    rec.extend(models.iter().filter(|m| !m.recommended));
    rec
}

/// 在已下載的模型中挑最小的一個（選用模型遺失時自動改用；絕不建議更大的模型）。
pub(crate) fn smallest_downloaded(models: &[ModelInfo]) -> Option<&ModelInfo> {
    models
        .iter()
        .filter(|m| m.is_downloaded)
        .min_by_key(|m| m.size.disk_mb)
}

/// 需要明確確認大小才下載：不在建議清單內（medium 以上）的模型。
pub(crate) fn needs_size_confirmation(info: &ModelInfo) -> bool {
    !info.recommended
}

/// 同步檢查磁碟，若 `missing_id` 的檔案不存在，回傳可改用的最小已下載模型 id。
pub(crate) fn fallback_model_id(models_dir: &Path, missing_id: &str) -> Option<String> {
    let mut models = crate::models::registry::ModelRegistry::all();
    for m in &mut models {
        m.is_downloaded = model_path(models_dir, &m.id).exists();
    }
    smallest_downloaded(&models)
        .map(|m| m.id.clone())
        .filter(|id| id != missing_id)
}

pub(crate) fn model_entries_from(models: &[ModelInfo]) -> Vec<ModelEntryView> {
    ordered_for_display(models)
        .into_iter()
        .map(|m| ModelEntryView {
            id: m.id.clone(),
            display_name: m.display_name.clone(),
            is_downloaded: m.is_downloaded,
            size_mb: m.size.disk_mb as i32,
            quality: m.quality_stars as i32,
            download_url: m.download_url.clone(),
        })
        .collect()
}

/// 列出模型；回傳清單與 `selected_id` 的顯示名稱（若存在）。
pub(crate) async fn list_model_entries(
    models_dir: PathBuf,
    selected_id: &str,
) -> (Vec<ModelEntryView>, Option<String>) {
    let manager = ModelManager::new(models_dir);
    let models = manager.list_available().await;
    let name = models
        .iter()
        .find(|m| m.id == selected_id)
        .map(|m| m.display_name.clone());
    (model_entries_from(&models), name)
}

pub(crate) fn model_path(models_dir: &Path, model_id: &str) -> PathBuf {
    models_dir.join(format!("ggml-{model_id}.bin"))
}

// ─── 模型預載 ─────────────────────────────────────────────────────────────────

pub(crate) fn spawn_model_preload(
    runtime: ModelRuntime,
    model_id: String,
    model_path: PathBuf,
    whisper_params: WhisperParams,
    notifier: Notifier,
) {
    let generation = runtime.generation.fetch_add(1, Ordering::SeqCst) + 1;
    notifier.send(CoreEvent::StatusIfIdle(format!("預載模型 {model_id}...")));

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
                    *cache = Some(LoadedWhisperModel {
                        model_id: model_id.clone(),
                        model_path: model_path.clone(),
                        engine,
                    });
                }

                notifier.send(CoreEvent::DeviceLabel(device));
                notifier.send(CoreEvent::StatusIfIdle(format!("模型已預載: {model_id}")));
            }
            Err(e) => {
                error!("模型預載失敗: {e}");
                notifier.send(CoreEvent::StatusIfIdle(format!("模型預載失敗: {e}")));
            }
        }
    });
}

// ─── 模型選擇 / 下載 ─────────────────────────────────────────────────────────

impl ModelContext {
    /// 選用模型；未下載時（可先詢問）下載後選用並預載。
    pub(crate) fn spawn_select_or_download(
        &self,
        model_id: String,
        whisper_params: WhisperParams,
        confirm_if_missing: bool,
    ) {
        let ctx = self.clone();
        self.tokio.spawn(async move {
            let manager = ModelManager::new(ctx.models_dir.clone());
            let models = manager.list_available().await;

            let Some(info) = models.iter().find(|m| m.id == model_id).cloned() else {
                ctx.notifier
                    .send(CoreEvent::Status(format!("未知模型: {model_id}")));
                return;
            };

            if info.is_downloaded {
                ctx.select_downloaded_model(model_id, whisper_params, models, "模型已切換");
                return;
            }

            if confirm_if_missing || needs_size_confirmation(&info) {
                let display_name = info.display_name.clone();
                let size_mb = info.size.disk_mb;
                let result = rfd::AsyncMessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("模型尚未下載")
                    .set_description(format!(
                        "語音辨識模型 {display_name} 尚未下載。\n\n大小約 {size_mb} MB，是否現在自動下載？\n下載完成後會自動選用並預載。{}",
                        if needs_size_confirmation(&info) {
                            "\n\n提醒：此模型較大，下載與辨識都會比較慢；小型模型（Tiny / Base / Small）通常已足夠。"
                        } else {
                            ""
                        }
                    ))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    .await;

                if result != rfd::MessageDialogResult::Yes {
                    ctx.notifier
                        .send(CoreEvent::Status(format!("已取消下載模型 {display_name}")));
                    return;
                }
            }

            ctx.download_model_and_select(info, whisper_params).await;
        });
    }

    fn select_downloaded_model(
        &self,
        model_id: String,
        whisper_params: WhisperParams,
        models: Vec<ModelInfo>,
        status: &str,
    ) {
        *self.selected_model_id.lock().unwrap() = model_id.clone();

        let display_name = models
            .iter()
            .find(|m| m.id == model_id)
            .map(|m| m.display_name.clone())
            .unwrap_or_else(|| model_id.clone());
        let path = model_path(&self.models_dir, &model_id);

        self.notifier.send(CoreEvent::ModelSelected {
            model_id: model_id.clone(),
            display_name,
            entries: model_entries_from(&models),
            status: status.to_string(),
        });

        spawn_model_preload(
            self.runtime.clone(),
            model_id,
            path,
            whisper_params,
            self.notifier.clone(),
        );
    }

    async fn download_model_and_select(&self, info: ModelInfo, whisper_params: WhisperParams) {
        let model_id = info.id.clone();
        let size_mb = info.size.disk_mb;

        if self
            .runtime
            .download_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            self.notifier
                .send(CoreEvent::Status("已有模型正在下載，請稍候...".into()));
            return;
        }

        self.notifier.send(CoreEvent::DownloadStarted {
            model_id: model_id.clone(),
            size_mb,
        });

        let downloader = ModelDownloader::new(self.models_dir.clone());
        let (tx, mut rx) =
            tokio::sync::mpsc::channel::<crate::models::downloader::DownloadProgress>(64);

        let progress_notifier = self.notifier.clone();
        self.tokio.spawn(async move {
            while let Some(prog) = rx.recv().await {
                progress_notifier.send(CoreEvent::DownloadProgress(download_ratio(
                    prog.downloaded_bytes,
                    prog.total_bytes,
                )));
            }
        });

        match downloader.download(&info, tx).await {
            Ok(_) => {
                info!("模型 {model_id} 下載完成");
                let manager = ModelManager::new(self.models_dir.clone());
                let updated = manager.list_available().await;
                self.select_downloaded_model(
                    model_id,
                    whisper_params,
                    updated,
                    "模型下載完成，已選用",
                );
            }
            Err(e) => {
                error!("模型下載失敗: {e}");
                self.notifier
                    .send(CoreEvent::DownloadFailed(format!("下載失敗: {e}")));
            }
        }
        self.runtime
            .download_in_progress
            .store(false, Ordering::SeqCst);
    }
}

/// 下載進度比例；總大小未知時顯示 0.5。
pub(crate) fn download_ratio(downloaded: u64, total: Option<u64>) -> f32 {
    total
        .map(|t| downloaded as f32 / t as f32)
        .unwrap_or(0.5_f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_ratio_handles_unknown_total() {
        assert_eq!(download_ratio(50, Some(100)), 0.5);
        assert_eq!(download_ratio(100, Some(100)), 1.0);
        assert_eq!(download_ratio(123, None), 0.5);
    }

    fn mi(id: &str, mb: u64, rec: bool, dl: bool) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            display_name: id.into(),
            size: crate::models::registry::ModelSize {
                disk_mb: mb,
                vram_mb: mb,
            },
            speed_multiplier: 1.0,
            quality_stars: 1,
            is_english_only: false,
            sha256: String::new(),
            download_url: String::new(),
            mirror_urls: vec![],
            local_path: None,
            is_downloaded: dl,
            recommended: rec,
        }
    }

    #[test]
    fn recommended_models_listed_first_smallest_first() {
        let models = vec![
            mi("medium", 1500, false, false),
            mi("small", 466, true, false),
            mi("tiny", 75, true, false),
            mi("base", 142, true, false),
        ];
        let ids: Vec<_> = model_entries_from(&models)
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, ["tiny", "base", "small", "medium"]);
    }

    #[test]
    fn smallest_downloaded_ignores_missing_models() {
        let models = vec![
            mi("tiny", 75, true, false),
            mi("small", 466, true, true),
            mi("medium", 1500, false, true),
        ];
        assert_eq!(smallest_downloaded(&models).unwrap().id, "small");
        assert!(smallest_downloaded(&[mi("tiny", 75, true, false)]).is_none());
    }

    #[test]
    fn only_non_recommended_models_need_size_confirmation() {
        assert!(!needs_size_confirmation(&mi("small", 466, true, false)));
        assert!(needs_size_confirmation(&mi("medium", 1500, false, false)));
    }

    #[test]
    fn model_path_uses_ggml_prefix() {
        let p = model_path(Path::new("models"), "small");
        assert!(p.ends_with("ggml-small.bin"));
    }
}
