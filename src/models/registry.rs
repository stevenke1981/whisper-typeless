use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    pub sha1: &'static str,
    pub local_path: Option<PathBuf>,
    pub is_downloaded: bool,
}

pub struct ModelRegistry;

const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

impl ModelRegistry {
    pub fn all() -> Vec<ModelInfo> {
        vec![
            Self::entry(
                "tiny",
                "Tiny",
                75,
                125,
                10.0,
                2,
                false,
                "bd577a113a864445d4c299885e0cb97d4ba92b5f",
            ),
            Self::entry(
                "base",
                "Base",
                142,
                210,
                7.0,
                3,
                false,
                "465707469ff3a37a2b9b8d8f89f2f99de7299dac",
            ),
            Self::entry(
                "small",
                "Small",
                466,
                600,
                4.0,
                4,
                false,
                "55356645c2b361a969dfd0ef2c5a50d530afd8d5",
            ),
            Self::entry(
                "medium",
                "Medium",
                1500,
                1700,
                2.0,
                5,
                false,
                "fd9727b6e1217c2f614f9b698455c4ffd82463b4",
            ),
            Self::entry(
                "large-v2",
                "Large v2",
                2900,
                3100,
                1.0,
                5,
                false,
                "0f4c8e34f21cf1a914c59d8b3ce882345ad349d6",
            ),
            Self::entry(
                "large-v3",
                "Large v3",
                2900,
                3100,
                1.0,
                5,
                false,
                "ad82bf6a9043ceed055076d0fd39f5f186ff8062",
            ),
            Self::entry(
                "large-v3-turbo",
                "Large v3 Turbo",
                1600,
                1800,
                2.0,
                5,
                false,
                "4af2b29d7ec73d781377bfd1758ca957d842e1a4",
            ),
        ]
    }

    pub fn download_url(model_id: &str) -> String {
        format!("{HF_BASE}/ggml-{model_id}.bin")
    }

    pub fn modelscope_url(model_id: &str) -> String {
        format!(
            "https://modelscope.cn/models/ggerganov/whisper.cpp/resolve/master/ggml-{model_id}.bin"
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn entry(
        id: &'static str,
        display_name: &'static str,
        disk_mb: u64,
        vram_mb: u64,
        speed: f32,
        quality: u8,
        en_only: bool,
        sha1: &'static str,
    ) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            display_name: display_name.to_string(),
            size: ModelSize { disk_mb, vram_mb },
            speed_multiplier: speed,
            quality_stars: quality,
            is_english_only: en_only,
            sha1,
            local_path: None,
            is_downloaded: false,
        }
    }
}
