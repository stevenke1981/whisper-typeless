pub mod custom;
pub mod detector;
pub mod templates;

pub use detector::ContextDetector;
pub use templates::{ContextTemplate, TemplateRegistry};

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TemplateId {
    Meeting,
    Casual,
    Technical,
    Medical,
    Legal,
    Custom(String),
}

impl TemplateId {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Meeting => "會議記錄",
            Self::Casual => "口語對話",
            Self::Technical => "技術討論",
            Self::Medical => "醫療紀錄",
            Self::Legal => "法律文書",
            Self::Custom(name) => name.as_str(),
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Meeting => "🏢",
            Self::Casual => "💬",
            Self::Technical => "💻",
            Self::Medical => "🏥",
            Self::Legal => "⚖",
            Self::Custom(_) => "📝",
        }
    }
}
