use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSize {
    pub disk_mb: u64,
    pub vram_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub size: ModelSize,
    pub speed_multiplier: f32,
    pub quality_stars: u8,
    pub is_english_only: bool,
    pub sha1: String,
    pub download_url: String,
    pub mirror_urls: Vec<String>,
    pub local_path: Option<PathBuf>,
    pub is_downloaded: bool,
}

pub struct ModelRegistry;

const EMBEDDED_MODEL_CATALOG: &str = include_str!("../../model-catalog.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelCatalog {
    models: Vec<ModelCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelCatalogEntry {
    id: String,
    display_name: String,
    size: ModelSize,
    speed_multiplier: f32,
    quality_stars: u8,
    is_english_only: bool,
    sha1: String,
    download_url: String,
    #[serde(default)]
    mirror_urls: Vec<String>,
}

impl ModelRegistry {
    pub fn all() -> Vec<ModelInfo> {
        if let Some(models) = Self::load_external_catalog() {
            return models;
        }

        match Self::parse_catalog(EMBEDDED_MODEL_CATALOG) {
            Ok(models) => models,
            Err(err) => {
                warn!("內建模型 catalog 解析失敗: {err}");
                Vec::new()
            }
        }
    }

    fn load_external_catalog() -> Option<Vec<ModelInfo>> {
        for path in Self::catalog_candidate_paths() {
            if !path.is_file() {
                continue;
            }

            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(err) => {
                    warn!("讀取模型 catalog 失敗 {}: {err}", path.display());
                    continue;
                }
            };

            match Self::parse_catalog(&text) {
                Ok(models) => return Some(models),
                Err(err) => {
                    warn!("模型 catalog 解析失敗 {}: {err}", path.display());
                }
            }
        }

        None
    }

    fn catalog_candidate_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.join("model-catalog.json"));
            }
        }

        if let Ok(dir) = std::env::current_dir() {
            let path = dir.join("model-catalog.json");
            if !paths.iter().any(|existing| existing == &path) {
                paths.push(path);
            }
        }

        paths
    }

    fn parse_catalog(source: &str) -> Result<Vec<ModelInfo>, serde_json::Error> {
        let catalog: ModelCatalog = serde_json::from_str(source)?;
        Ok(catalog
            .models
            .into_iter()
            .map(|entry| ModelInfo {
                id: entry.id,
                display_name: entry.display_name,
                size: entry.size,
                speed_multiplier: entry.speed_multiplier,
                quality_stars: entry.quality_stars,
                is_english_only: entry.is_english_only,
                sha1: entry.sha1,
                download_url: entry.download_url,
                mirror_urls: entry.mirror_urls,
                local_path: None,
                is_downloaded: false,
            })
            .collect())
    }
}
