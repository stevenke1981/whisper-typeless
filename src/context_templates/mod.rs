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
    /// 穩定的英文 id（用於 registry 鍵與 TOML 檔），不隨語系變動。
    pub fn stable_id(&self) -> String {
        match self {
            Self::Meeting => "meeting".into(),
            Self::Casual => "casual".into(),
            Self::Technical => "technical".into(),
            Self::Medical => "medical".into(),
            Self::Legal => "legal".into(),
            Self::Custom(name) => name.clone(),
        }
    }

    /// 由穩定 id 還原；未知 id 視為自訂模板。
    pub fn from_stable_id(id: &str) -> Self {
        match id {
            "meeting" => Self::Meeting,
            "casual" => Self::Casual,
            "technical" => Self::Technical,
            "medical" => Self::Medical,
            "legal" => Self::Legal,
            other => Self::Custom(other.to_string()),
        }
    }

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
