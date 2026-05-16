use futures::StreamExt;
use sha1::{Digest, Sha1};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use super::registry::ModelRegistry;

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: f64,
}

pub struct ModelDownloader {
    models_dir: PathBuf,
    client: reqwest::Client,
}

impl ModelDownloader {
    pub fn new(models_dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .build()
            .expect("HTTP 客戶端建立失敗");

        Self { models_dir, client }
    }

    pub async fn download(
        &self,
        model_id: &str,
        expected_sha1: &str,
        progress_tx: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> anyhow::Result<PathBuf> {
        fs::create_dir_all(&self.models_dir).await?;

        let dest = self.models_dir.join(format!("ggml-{model_id}.bin"));
        let temp = dest.with_extension("bin.tmp");

        let url = ModelRegistry::download_url(model_id);
        info!("下載模型: {url}");

        // 主要源：HuggingFace；網路錯誤或 HTTP 非 2xx 時自動切換 ModelScope 備用源
        let response = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                warn!("主要源 HTTP {} 失敗，嘗試 ModelScope 備用源", r.status());
                self.client
                    .get(ModelRegistry::modelscope_url(model_id))
                    .send()
                    .await?
                    .error_for_status()?
            }
            Err(e) => {
                warn!("主要源連線失敗（{}），嘗試 ModelScope 備用源", e);
                self.client
                    .get(ModelRegistry::modelscope_url(model_id))
                    .send()
                    .await?
                    .error_for_status()?
            }
        };

        let total = response.content_length();
        let mut file = fs::File::create(&temp).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        let mut hasher = Sha1::new();
        let start = std::time::Instant::now();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let speed = downloaded as f64 / elapsed;

            let _ = progress_tx.try_send(DownloadProgress {
                model_id: model_id.to_string(),
                downloaded_bytes: downloaded,
                total_bytes: total,
                speed_bps: speed,
            });
        }

        file.flush().await?;
        drop(file);

        let computed = format!("{:x}", hasher.finalize());
        if !expected_sha1.is_empty() && computed != expected_sha1 {
            fs::remove_file(&temp).await?;
            return Err(anyhow::anyhow!(
                "SHA1 校驗失敗: 期望 {expected_sha1}, 得到 {computed}"
            ));
        }

        fs::rename(&temp, &dest).await?;
        info!("模型下載完成: {}", dest.display());

        Ok(dest)
    }

    pub async fn is_downloaded(&self, model_id: &str) -> bool {
        let path = self.models_dir.join(format!("ggml-{model_id}.bin"));
        fs::metadata(&path).await.is_ok()
    }
}
