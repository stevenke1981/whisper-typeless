use super::{TemplateId, TemplateRegistry};

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
        let mut best_score = 0usize;

        for template in self.registry.all() {
            let score = template
                .keywords
                .iter()
                .filter(|kw| text_lower.contains(kw.as_str()))
                .count();

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
}
