pub mod downloader;
pub mod manager;
pub mod registry;

pub use downloader::ModelDownloader;
pub use manager::ModelManager;
pub use registry::{ModelInfo, ModelRegistry, ModelSize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModelId {
    Tiny,
    TinyEn,
    Base,
    BaseEn,
    Small,
    SmallEn,
    Medium,
    MediumEn,
    LargeV2,
    LargeV3,
    LargeV3Turbo,
}

impl ModelId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::TinyEn => "tiny.en",
            Self::Base => "base",
            Self::BaseEn => "base.en",
            Self::Small => "small",
            Self::SmallEn => "small.en",
            Self::Medium => "medium",
            Self::MediumEn => "medium.en",
            Self::LargeV2 => "large-v2",
            Self::LargeV3 => "large-v3",
            Self::LargeV3Turbo => "large-v3-turbo",
        }
    }

    /// Primary filename (ggml-format .bin) for download
    pub fn filename(&self) -> String {
        format!("ggml-{}.bin", self.as_str())
    }

    /// Alternative filenames to search on disk (newer gguf format first, then legacy ggml)
    pub fn candidate_filenames(&self) -> Vec<String> {
        Self::candidate_filenames_for(self.as_str())
    }

    /// Look up candidate filenames from a string model id.
    pub fn candidate_filenames_for(id: &str) -> Vec<String> {
        vec![
            format!("{id}.gguf"),
            format!("ggml-{id}.bin"),
        ]
    }

    /// Search `dir` for any filename that matches a model id, returning the first match.
    pub fn find_model_in_dir(dir: &Path, model_id: &str) -> Option<std::path::PathBuf> {
        for name in Self::candidate_filenames_for(model_id) {
            let path = dir.join(&name);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }
}
