use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::TemplateId;
use crate::whisper::WhisperParams;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostProcessSettings {
    pub add_punctuation: bool,
    pub formal_style: bool,
    pub remove_fillers: Vec<String>,
    pub preserve_english_terms: bool,
}

impl Default for PostProcessSettings {
    fn default() -> Self {
        Self {
            add_punctuation: true,
            formal_style: false,
            remove_fillers: vec!["嗯".into(), "啊".into(), "那個".into(), "就是說".into()],
            preserve_english_terms: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTemplate {
    pub id: TemplateId,
    pub initial_prompt: String,
    pub language_override: Option<String>,
    pub temperature_override: Option<f32>,
    pub postprocess: PostProcessSettings,
    pub keywords: Vec<String>,
}

impl ContextTemplate {
    pub fn apply_to_params(&self, params: &mut WhisperParams) {
        if !self.initial_prompt.is_empty() {
            params.initial_prompt = Some(self.initial_prompt.clone());
        }
        if let Some(lang) = &self.language_override {
            params.language = Some(lang.clone());
        }
        if let Some(temp) = self.temperature_override {
            params.temperature = temp;
        }
    }
}

pub struct TemplateRegistry {
    templates: HashMap<String, ContextTemplate>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            templates: HashMap::new(),
        };
        reg.load_builtins();
        reg
    }

    fn load_builtins(&mut self) {
        let builtins = [
            Self::meeting_template(),
            Self::casual_template(),
            Self::technical_template(),
            Self::medical_template(),
            Self::legal_template(),
        ];
        for t in builtins {
            self.templates.insert(t.id.display_name().to_string(), t);
        }
    }

    fn meeting_template() -> ContextTemplate {
        ContextTemplate {
            id: TemplateId::Meeting,
            initial_prompt:
                "這是一段商業會議的記錄，包含專業術語、人名和公司名稱。請準確轉錄所有內容。".into(),
            language_override: Some("zh".into()),
            temperature_override: Some(0.0),
            postprocess: PostProcessSettings {
                formal_style: true,
                remove_fillers: vec!["嗯".into(), "啊".into(), "那個".into()],
                ..Default::default()
            },
            keywords: vec![
                "議程".into(),
                "會議".into(),
                "決議".into(),
                "報告".into(),
                "預算".into(),
                "季度".into(),
                "目標".into(),
            ],
        }
    }

    fn casual_template() -> ContextTemplate {
        ContextTemplate {
            id: TemplateId::Casual,
            initial_prompt: "這是一段日常對話。".into(),
            language_override: Some("zh".into()),
            temperature_override: Some(0.2),
            postprocess: PostProcessSettings::default(),
            keywords: vec![],
        }
    }

    fn technical_template() -> ContextTemplate {
        ContextTemplate {
            id: TemplateId::Technical,
            initial_prompt: "This is a technical discussion about software engineering, including programming terms, API names, and technical concepts. 程式碼、函式名稱、技術術語請保留英文原文。".into(),
            language_override: Some("zh".into()),
            temperature_override: Some(0.0),
            postprocess: PostProcessSettings {
                preserve_english_terms: true,
                ..Default::default()
            },
            keywords: vec![
                "程式".into(), "函式".into(), "API".into(), "bug".into(),
                "資料庫".into(), "伺服器".into(), "版本".into(), "部署".into(),
            ],
        }
    }

    fn medical_template() -> ContextTemplate {
        ContextTemplate {
            id: TemplateId::Medical,
            initial_prompt: "這是醫療診療的對話，包含醫學術語、藥品名稱和診斷名稱。".into(),
            language_override: Some("zh".into()),
            temperature_override: Some(0.0),
            postprocess: PostProcessSettings {
                formal_style: true,
                ..Default::default()
            },
            keywords: vec![
                "症狀".into(),
                "診斷".into(),
                "治療".into(),
                "藥物".into(),
                "手術".into(),
                "檢查".into(),
                "病患".into(),
                "醫師".into(),
            ],
        }
    }

    fn legal_template() -> ContextTemplate {
        ContextTemplate {
            id: TemplateId::Legal,
            initial_prompt: "這是法律相關的對話，包含法律術語和條文引用。".into(),
            language_override: Some("zh".into()),
            temperature_override: Some(0.0),
            postprocess: PostProcessSettings {
                formal_style: true,
                ..Default::default()
            },
            keywords: vec![
                "合約".into(),
                "條款".into(),
                "法律".into(),
                "訴訟".into(),
                "判決".into(),
                "法院".into(),
                "律師".into(),
                "被告".into(),
            ],
        }
    }

    pub fn get(&self, id: &TemplateId) -> Option<&ContextTemplate> {
        let key = id.display_name();
        self.templates.get(key)
    }

    pub fn get_casual(&self) -> &ContextTemplate {
        self.get(&TemplateId::Casual)
            .expect("casual template must exist")
    }

    pub fn all(&self) -> Vec<&ContextTemplate> {
        let order = [
            TemplateId::Meeting,
            TemplateId::Casual,
            TemplateId::Technical,
            TemplateId::Medical,
            TemplateId::Legal,
        ];
        order.iter().filter_map(|id| self.get(id)).collect()
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}
