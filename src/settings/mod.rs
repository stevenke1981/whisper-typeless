pub mod config;
pub mod persistence;
pub mod ui_settings;

pub use config::AppConfig;
pub use module_registry::ModuleRegistry;

mod module_registry {
    use super::config::AppConfig;
    use std::collections::HashMap;

    pub struct ModuleEntry {
        pub id: &'static str,
        pub display_name: &'static str,
        pub description: &'static str,
        pub enabled: bool,
    }

    pub struct ModuleRegistry {
        modules: HashMap<&'static str, ModuleEntry>,
    }

    const ALL_MODULES: &[(&str, &str, &str)] = &[
        ("vad", "靜音偵測", "自動偵測說話開始/結束"),
        ("context_templates", "情境模板", "智慧選擇轉錄提示詞"),
        ("opencc", "繁簡轉換", "OpenCC 中文字體轉換"),
        ("auto_inject", "自動注入", "結果自動貼入焦點視窗"),
        ("waveform", "波形顯示", "即時音訊波形視覺化"),
        ("history", "歷史記錄", "儲存所有轉錄結果"),
        ("noise_suppress", "降噪", "音訊前處理降噪"),
        ("auto_punctuation", "自動標點", "中文標點符號自動添加"),
        ("speaker_detect", "說話者偵測", "多人場景說話者分離"),
    ];

    impl ModuleRegistry {
        pub fn with_all_modules(config: &AppConfig) -> Self {
            let mut modules = HashMap::new();
            for (id, name, desc) in ALL_MODULES {
                let enabled = config.modules.is_enabled(id);
                modules.insert(
                    *id,
                    ModuleEntry {
                        id,
                        display_name: name,
                        description: desc,
                        enabled,
                    },
                );
            }
            Self { modules }
        }

        pub fn is_enabled(&self, id: &str) -> bool {
            self.modules.get(id).map(|m| m.enabled).unwrap_or(false)
        }

        pub fn set_enabled(&mut self, id: &str, enabled: bool) {
            if let Some(m) = self.modules.get_mut(id) {
                m.enabled = enabled;
            }
        }

        pub fn all(&self) -> impl Iterator<Item = &ModuleEntry> {
            self.modules.values()
        }
    }
}
