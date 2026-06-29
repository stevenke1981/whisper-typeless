pub use crate::context_templates::templates::PostProcessSettings;

pub struct PostProcessor {
    settings: PostProcessSettings,
}

impl PostProcessor {
    pub fn new(settings: PostProcessSettings) -> Self {
        Self { settings }
    }

    pub fn process(&self, text: &str) -> String {
        let mut result = text.to_string();

        result = self.remove_fillers(&result);
        result = self.remove_repeated_phrases(&result);

        if self.settings.add_punctuation {
            result = self.normalize_punctuation(&result);
        }

        result.trim().to_string()
    }

    fn remove_fillers(&self, text: &str) -> String {
        self.settings
            .fillers()
            .iter()
            .fold(text.to_string(), |acc, filler| {
                acc.replace(filler.as_str(), "")
            })
    }

    fn remove_repeated_phrases(&self, text: &str) -> String {
        // 移除連續重複超過 3 次的片段 (whisper 幻覺)
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();

        if len < 6 {
            return text.to_string();
        }

        let mut cleaned = String::new();
        let mut i = 0;

        while i < len {
            let mut found_repeat = false;

            // 嘗試各種片段長度
            'outer: for seg_len in (2..=10).rev() {
                if i + seg_len * 3 > len {
                    continue;
                }

                let seg: String = chars[i..i + seg_len].iter().collect();
                let mut repeat_count = 1;
                let mut j = i + seg_len;

                while j + seg_len <= len {
                    let next: String = chars[j..j + seg_len].iter().collect();
                    if next == seg {
                        repeat_count += 1;
                        j += seg_len;
                    } else {
                        break;
                    }
                }

                if repeat_count >= 3 {
                    cleaned.push_str(&seg);
                    i = j;
                    found_repeat = true;
                    break 'outer;
                }
            }

            if !found_repeat {
                cleaned.push(chars[i]);
                i += 1;
            }
        }

        cleaned
    }

    fn normalize_punctuation(&self, text: &str) -> String {
        text.replace(',', "，")
            .replace('.', "。")
            .replace('!', "！")
            .replace('?', "？")
            .replace(':', "：")
            .replace(';', "；")
    }

    pub fn update_settings(&mut self, settings: PostProcessSettings) {
        self.settings = settings;
    }
}

// 讓 PostProcessSettings 提供 fillers 訪問
impl PostProcessSettings {
    pub fn fillers(&self) -> &[String] {
        &self.remove_fillers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_proc() -> PostProcessor {
        PostProcessor::new(PostProcessSettings::default())
    }

    #[test]
    fn removes_fillers() {
        let proc = default_proc();
        let result = proc.process("嗯這個我覺得啊很不錯");
        assert!(!result.contains("嗯"));
        assert!(!result.contains("啊"));
    }

    #[test]
    fn removes_repeated_phrases() {
        let proc = default_proc();
        let repeated = "謝謝謝謝謝謝謝謝謝謝";
        let result = proc.process(repeated);
        assert!(result.len() < repeated.len());
    }

    #[test]
    fn postprocess_overlapping_repeats() {
        let proc = default_proc();
        let result = proc.process("ABCABCABCABC");
        // Should collapse to at most one "ABC" plus possible leftover
        assert!(result.len() <= 6, "got '{}' (len {})", result, result.len());
    }

    #[test]
    fn postprocess_short_text_no_repeat_check() {
        let proc = default_proc();
        let result = proc.process("Hi");
        assert_eq!(result, "Hi");
    }

    #[test]
    fn postprocess_punctuation_normalize_all() {
        let settings = PostProcessSettings {
            add_punctuation: true,
            ..Default::default()
        };
        let proc = PostProcessor::new(settings);
        let result = proc.process("hello,world.ok!no?");
        assert!(!result.contains(','));
        assert!(!result.contains('.'));
        assert!(result.contains('，'));
        assert!(result.contains('。'));
    }

    #[test]
    fn postprocess_formal_style_noop() {
        let proc = default_proc();
        let result = proc.process("no punctuation here");
        assert_eq!(result, "no punctuation here");
    }

    #[test]
    fn postprocess_repeated_edge_bigrams() {
        let proc = default_proc();
        let result = proc.process("ABABAB");
        assert!(result.len() < 6, "should collapse 'ABABAB' but got '{}'", result);
    }

    #[test]
    fn postprocess_empty_input() {
        let proc = default_proc();
        let result = proc.process("");
        assert_eq!(result, "");
    }
}
