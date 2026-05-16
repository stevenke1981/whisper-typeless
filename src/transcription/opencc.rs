use serde::{Deserialize, Serialize};

use ferrous_opencc::{config::BuiltinConfig, OpenCC};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum ConversionMode {
    #[serde(rename = "zh-TW")]
    #[default]
    ZhTW,
    #[serde(rename = "zh-HK")]
    ZhHK,
    #[serde(rename = "zh-CN")]
    ZhCN,
    #[serde(rename = "raw")]
    Raw,
}

pub struct OpenCCProcessor {
    converter: Option<OpenCC>,
}

impl OpenCCProcessor {
    pub fn new(mode: ConversionMode) -> anyhow::Result<Self> {
        let converter = Self::converter_for_mode(&mode)?;
        Ok(Self { converter })
    }

    pub fn convert(&self, text: &str) -> String {
        match &self.converter {
            Some(converter) => converter.convert(text),
            None => text.to_string(),
        }
    }

    fn converter_for_mode(mode: &ConversionMode) -> anyhow::Result<Option<OpenCC>> {
        let config = match mode {
            ConversionMode::Raw => return Ok(None),
            ConversionMode::ZhTW => BuiltinConfig::S2twp,
            ConversionMode::ZhHK => BuiltinConfig::S2hk,
            ConversionMode::ZhCN => BuiltinConfig::Tw2sp,
        };
        Ok(Some(OpenCC::from_config(config)?))
    }

    pub fn set_mode(&mut self, mode: ConversionMode) {
        self.converter = Self::converter_for_mode(&mode)
            .map_err(|e| tracing::error!("OpenCC 模式切換失敗: {e}"))
            .ok()
            .flatten();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_mode_unchanged() {
        let proc = OpenCCProcessor::new(ConversionMode::Raw).unwrap();
        assert_eq!(proc.convert("軟體"), "軟體");
    }

    #[test]
    fn simp_to_trad_tw() {
        let proc = OpenCCProcessor::new(ConversionMode::ZhTW).unwrap();
        let result = proc.convert("这是一个软件程序，设置里面还有用户数据。");
        assert!(result.contains("這是一個軟體程式"));
        assert!(result.contains("設定"));
        assert!(result.contains("使用者資料"));
    }
}
