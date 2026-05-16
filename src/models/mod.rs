pub mod downloader;
pub mod manager;
pub mod registry;

pub use downloader::ModelDownloader;
pub use manager::ModelManager;
pub use registry::{ModelInfo, ModelRegistry, ModelSize};

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

    pub fn filename(&self) -> String {
        format!("ggml-{}.bin", self.as_str())
    }
}
