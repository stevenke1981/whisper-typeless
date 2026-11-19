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
    /// SHA-256 hex string (preferred); falls back to `sha1` if empty.
    pub sha256: String,
    pub download_url: String,
    pub mirror_urls: Vec<String>,
    pub local_path: Option<PathBuf>,
    pub is_downloaded: bool,
    /// 建議優先使用的小型模型（catalog 的 `recommended`）。
    pub recommended: bool,
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
    #[serde(alias = "sha1")]
    sha256: String,
    download_url: String,
    #[serde(default)]
    mirror_urls: Vec<String>,
    /// 未標示時（例如舊版 catalog）依大小判斷，見 [`SMALL_MODEL_MAX_MB`]。
    #[serde(default)]
    recommended: Option<bool>,
}

/// 未標示 `recommended` 的模型，磁碟大小不超過此值即視為建議的小型模型（Small 約 466 MB）。
pub const SMALL_MODEL_MAX_MB: u64 = 500;

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
            .map(|entry| {
                let recommended = entry
                    .recommended
                    .unwrap_or(entry.size.disk_mb <= SMALL_MODEL_MAX_MB);
                ModelInfo {
                    id: entry.id,
                    display_name: entry.display_name,
                    size: entry.size,
                    speed_multiplier: entry.speed_multiplier,
                    quality_stars: entry.quality_stars,
                    is_english_only: entry.is_english_only,
                    sha256: entry.sha256,
                    download_url: entry.download_url,
                    mirror_urls: entry.mirror_urls,
                    local_path: None,
                    is_downloaded: false,
                    recommended,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_embedded_catalog() {
        let models = ModelRegistry::all();
        assert!(!models.is_empty(), "should have at least one model");
        assert!(models.len() >= 5, "should have at least 5 models");
    }

    #[test]
    fn registry_entries_have_required_fields() {
        let models = ModelRegistry::all();
        for m in &models {
            assert!(!m.id.is_empty(), "model id must not be empty");
            assert!(!m.download_url.is_empty(), "{} has no download_url", m.id);
            assert!(m.size.disk_mb > 0, "{} disk_mb must be > 0", m.id);
        }
    }

    #[test]
    fn registry_fallback_on_broken_catalog() {
        let result = ModelRegistry::parse_catalog("not valid json");
        assert!(result.is_err());
    }

    const MINI_CATALOG: &str = r#"{"models":[{"id":"tiny","display_name":"Tiny","size":{"disk_mb":75,"vram_mb":390},"speed_multiplier":10.0,"quality_stars":1,"is_english_only":false,"sha1":"abc123","download_url":"https://example.invalid/tiny.bin"}]}"#;

    #[test]
    fn parse_catalog_minimal_entry() {
        let models = ModelRegistry::parse_catalog(MINI_CATALOG).unwrap();
        assert_eq!(models.len(), 1);
        let m = &models[0];
        assert_eq!(m.id, "tiny");
        assert_eq!(m.size.disk_mb, 75);
        assert!(m.mirror_urls.is_empty());
        assert!(!m.is_downloaded);
        assert!(m.local_path.is_none());
    }

    #[test]
    fn recommended_falls_back_to_size_and_embedded_marks_small_models() {
        // 舊版 catalog 沒有 recommended：依大小判斷
        let models = ModelRegistry::parse_catalog(MINI_CATALOG).unwrap();
        assert!(models[0].recommended);
        let big = MINI_CATALOG.replace("\"disk_mb\":75", "\"disk_mb\":1500");
        assert!(!ModelRegistry::parse_catalog(&big).unwrap()[0].recommended);
        let explicit = MINI_CATALOG.replace("\"sha1\"", "\"recommended\":false,\"sha1\"");
        assert!(!ModelRegistry::parse_catalog(&explicit).unwrap()[0].recommended);
        let all = ModelRegistry::parse_catalog(EMBEDDED_MODEL_CATALOG).unwrap();
        let mut rec: Vec<_> = all
            .iter()
            .filter(|m| m.recommended)
            .map(|m| m.id.as_str())
            .collect();
        rec.sort_unstable();
        assert_eq!(rec, ["base", "small", "tiny"]);
    }

    #[test]
    fn parse_catalog_sha1_alias_fills_sha256() {
        let models = ModelRegistry::parse_catalog(MINI_CATALOG).unwrap();
        assert_eq!(models[0].sha256, "abc123");
    }

    #[test]
    fn parse_catalog_missing_field_errors() {
        assert!(ModelRegistry::parse_catalog(r#"{"models":[{"id":"x"}]}"#).is_err());
        assert!(ModelRegistry::parse_catalog("{}").is_err());
    }

    #[test]
    fn parse_catalog_empty_models() {
        assert!(ModelRegistry::parse_catalog(r#"{"models":[]}"#)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn embedded_catalog_ids_unique() {
        let models = ModelRegistry::parse_catalog(EMBEDDED_MODEL_CATALOG).unwrap();
        let mut ids: Vec<_> = models.iter().map(|m| m.id.clone()).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(n, ids.len());
    }

    #[test]
    fn model_id_candidate_filenames() {
        use crate::models::ModelId;
        let names = ModelId::candidate_filenames_for("small");
        assert!(names.contains(&"small.gguf".to_string()));
        assert!(names.contains(&"ggml-small.bin".to_string()));
    }
}
