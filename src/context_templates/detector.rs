use super::{TemplateId, TemplateRegistry};

/// 關鍵字權重：越長越具體（3 字以上 1.5，否則 1.0）。
fn keyword_weight(kw: &str) -> f32 {
    if kw.chars().count() >= 3 {
        1.5
    } else {
        1.0
    }
}

pub struct ContextDetector {
    registry: TemplateRegistry,
}

impl ContextDetector {
    pub fn new(registry: TemplateRegistry) -> Self {
        Self { registry }
    }

    pub fn detect(&self, text: &str) -> TemplateId {
        let text_lower = text.to_lowercase();
        let mut best_id = TemplateId::Casual;
        let mut best_score = 0.0f32;

        for template in self.registry.all() {
            let score: f32 = template
                .keywords
                .iter()
                .filter(|kw| !kw.is_empty() && text_lower.contains(&kw.to_lowercase()))
                .map(|kw| keyword_weight(kw))
                .sum();

            if score > best_score {
                best_score = score;
                best_id = template.id.clone();
            }
        }

        best_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_meeting() {
        let detector = ContextDetector::new(TemplateRegistry::new());
        let text = "今天的會議主要討論預算和季度目標";
        assert_eq!(detector.detect(text), TemplateId::Meeting);
    }

    #[test]
    fn defaults_to_casual() {
        let detector = ContextDetector::new(TemplateRegistry::new());
        let text = "今天天氣不錯";
        assert_eq!(detector.detect(text), TemplateId::Casual);
    }

    #[test]
    fn detector_technical_keywords() {
        let detector = ContextDetector::new(TemplateRegistry::new());
        let text = "這個API有bug，需要重新部署伺服器版本";
        assert_eq!(detector.detect(text), TemplateId::Technical);
    }

    #[test]
    fn detector_multiple_matches_highest_wins() {
        let detector = ContextDetector::new(TemplateRegistry::new());
        // Contains both meeting and technical keywords; technical has more matches
        let text = "會議討論API部署和資料庫版本";
        assert_eq!(detector.detect(text), TemplateId::Technical);
    }

    #[test]
    fn detector_empty_text() {
        let detector = ContextDetector::new(TemplateRegistry::new());
        assert_eq!(detector.detect(""), TemplateId::Casual);
    }

    #[test]
    fn detector_medical_and_legal() {
        let detector = ContextDetector::new(TemplateRegistry::builtin_only());
        assert_eq!(detector.detect("醫師說症狀需要檢查"), TemplateId::Medical);
        assert_eq!(detector.detect("律師提出合約條款"), TemplateId::Legal);
    }

    #[test]
    fn keyword_weight_prefers_specific() {
        assert!(keyword_weight("資料庫") > keyword_weight("會議"));
    }
}
