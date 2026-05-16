use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatterConfig {
    pub append_newline: bool,
    pub append_space: bool,
    pub add_timestamp: bool,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            append_newline: true,
            append_space: false,
            add_timestamp: false,
        }
    }
}

pub struct OutputFormatter {
    config: FormatterConfig,
}

impl OutputFormatter {
    pub fn new(config: FormatterConfig) -> Self {
        Self { config }
    }

    pub fn format(&self, text: &str) -> String {
        let mut result = text.to_string();

        if self.config.add_timestamp {
            let now = chrono_timestamp();
            result = format!("[{now}] {result}");
        }

        if self.config.append_newline {
            result.push('\n');
        } else if self.config.append_space {
            result.push(' ');
        }

        result
    }

    pub fn update(&mut self, config: FormatterConfig) {
        self.config = config;
    }
}

fn chrono_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
