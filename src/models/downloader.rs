use futures::StreamExt;
use sha1::{Digest, Sha1};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use super::registry::ModelInfo;

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
        model: &ModelInfo,
        progress_tx: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> anyhow::Result<PathBuf> {
        fs::create_dir_all(&self.models_dir).await?;

        let model_id = model.id.as_str();
        let dest = self.models_dir.join(format!("ggml-{model_id}.bin"));
        let temp = dest.with_extension("bin.tmp");

        let urls = std::iter::once(model.download_url.as_str())
            .chain(model.mirror_urls.iter().map(String::as_str))
            .filter(|url| !url.trim().is_empty())
            .collect::<Vec<_>>();

        if urls.is_empty() {
            return Err(anyhow::anyhow!("模型 {model_id} 沒有設定下載連結"));
        }

        let mut last_error: Option<anyhow::Error> = None;

        for (idx, url) in urls.iter().enumerate() {
            info!("下載模型: {url}");
            if idx > 0 {
                warn!("改用備用模型來源重試: {url}");
            }

            match self
                .download_from_url(
                    url,
                    model_id,
                    &model.sha1,
                    &dest,
                    &temp,
                    progress_tx.clone(),
                )
                .await
            {
                Ok(path) => return Ok(path),
                Err(e) => {
                    warn!("模型來源下載失敗: {e}");
                    let _ = fs::remove_file(&temp).await;
                    last_error = Some(e);
                }
            }
        }

        match last_error {
            Some(e) => Err(e),
            None => Err(anyhow::anyhow!("沒有可用的模型下載來源")),
        }
    }

    async fn download_from_url(
        &self,
        url: &str,
        model_id: &str,
        expected_sha1: &str,
        dest: &PathBuf,
        temp: &PathBuf,
        progress_tx: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> anyhow::Result<PathBuf> {
        let response = self.client.get(url).send().await?.error_for_status()?;
        let total = response.content_length();
        let mut file = fs::File::create(temp).await?;
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
        if !expected_sha1.is_empty() && !computed.eq_ignore_ascii_case(expected_sha1) {
            return Err(anyhow::anyhow!(
                "SHA1 校驗失敗: 期望 {expected_sha1}, 得到 {computed}"
            ));
        }

        fs::rename(temp, dest).await?;
        info!("模型下載完成: {}", dest.display());

        Ok(dest.clone())
    }

    pub async fn is_downloaded(&self, model_id: &str) -> bool {
        let path = self.models_dir.join(format!("ggml-{model_id}.bin"));
        fs::metadata(&path).await.is_ok()
    }
}
