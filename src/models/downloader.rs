use futures::StreamExt;
use sha1::Sha1;
use sha2::{Digest, Sha256};
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
                    &model.sha256,
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
        expected_sha256: &str,
        dest: &PathBuf,
        temp: &PathBuf,
        progress_tx: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> anyhow::Result<PathBuf> {
        let response = self.client.get(url).send().await?.error_for_status()?;
        let total = response.content_length();
        let mut file = fs::File::create(temp).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        let mut hasher = ChecksumHasher::for_expected(expected_sha256);
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

        let (algo, computed) = hasher.finalize_hex();
        if !checksum_matches(expected_sha256, &computed) {
            return Err(anyhow::anyhow!(
                "{algo} 校驗失敗: 期望 {expected_sha256}, 得到 {computed}"
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

/// 依 catalog 提供的雜湊長度選擇演算法：40 hex = SHA-1（whisper.cpp 官方公布值），
/// 其餘（含 64 hex）= SHA-256。
enum ChecksumHasher {
    Sha1(Sha1),
    Sha256(Sha256),
}

impl ChecksumHasher {
    fn for_expected(expected: &str) -> Self {
        if expected.trim().len() == 40 {
            Self::Sha1(Sha1::new())
        } else {
            Self::Sha256(Sha256::new())
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha1(h) => h.update(data),
            Self::Sha256(h) => h.update(data),
        }
    }

    fn finalize_hex(self) -> (&'static str, String) {
        match self {
            Self::Sha1(h) => ("SHA-1", format!("{:x}", h.finalize())),
            Self::Sha256(h) => ("SHA-256", format!("{:x}", h.finalize())),
        }
    }
}

/// 空的期望值代表 catalog 未提供雜湊，視為通過。
fn checksum_matches(expected: &str, computed: &str) -> bool {
    let expected = expected.trim();
    expected.is_empty() || computed.eq_ignore_ascii_case(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_of(expected: &str, data: &[u8]) -> (&'static str, String) {
        let mut h = ChecksumHasher::for_expected(expected);
        h.update(data);
        h.finalize_hex()
    }

    #[test]
    fn forty_hex_chars_selects_sha1() {
        let (algo, hex) = hex_of(&"0".repeat(40), b"abc");
        assert_eq!(algo, "SHA-1");
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn sixty_four_hex_chars_selects_sha256() {
        let (algo, hex) = hex_of(&"0".repeat(64), b"abc");
        assert_eq!(algo, "SHA-256");
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn catalog_sha1_values_verify() {
        let (_, hex) = hex_of("A9993E364706816ABA3E25717850C26C9CD0D89D", b"abc");
        assert!(checksum_matches(
            "A9993E364706816ABA3E25717850C26C9CD0D89D",
            &hex
        ));
        assert!(!checksum_matches(
            "bd577a113a864445d4c299885e0cb97d4ba92b5f",
            &hex
        ));
        assert!(checksum_matches("", &hex));
    }
}
