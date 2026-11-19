use crate::context_templates::{TemplateId, TemplateRegistry};
use crate::whisper::TranscriptResult;

use super::{
    opencc::{ConversionMode, OpenCCProcessor},
    postprocess::PostProcessor,
    ProcessedText,
};

/// UI 字串（"zh-TW" / "zh-HK" / "zh-CN" / "raw"）→ [`ConversionMode`]，未知值回退 zh-TW。
pub fn conversion_mode_from_str(mode: &str) -> ConversionMode {
    match mode {
        "zh-HK" => ConversionMode::ZhHK,
        "zh-CN" => ConversionMode::ZhCN,
        "raw" => ConversionMode::Raw,
        _ => ConversionMode::ZhTW,
    }
}

pub struct TranscriptionPipeline {
    opencc: OpenCCProcessor,
    template_registry: TemplateRegistry,
}

impl TranscriptionPipeline {
    pub fn new(
        conversion_mode: ConversionMode,
        template_registry: TemplateRegistry,
    ) -> anyhow::Result<Self> {
        let opencc = OpenCCProcessor::new(conversion_mode)?;

        Ok(Self {
            opencc,
            template_registry,
        })
    }

    /// 即時錄音路徑用：依 UI 的 OpenCC 開關與轉換模式建立管線。
    pub fn for_live(opencc_enabled: bool, conversion_mode: &str) -> anyhow::Result<Self> {
        let mode = if opencc_enabled {
            conversion_mode_from_str(conversion_mode)
        } else {
            ConversionMode::Raw
        };
        Self::new(mode, TemplateRegistry::default())
    }

    pub fn process(&self, result: TranscriptResult, template_id: &TemplateId) -> ProcessedText {
        let template = self.template_registry.get(template_id);

        let post_settings = template.map(|t| t.postprocess.clone()).unwrap_or_default();

        let proc = PostProcessor::new(post_settings);
        let after_post = proc.process(&result.text);
        let final_text = self.opencc.convert(&after_post);

        ProcessedText {
            text: final_text,
            raw_text: result.text,
            language: result.language,
        }
    }

    /// 處理純文字：去除首尾空白 →（指定範本時）範本後處理 → OpenCC 轉換。
    ///
    /// `template_id = None` 時不做後處理，與即時錄音路徑既有行為一致。
    pub fn process_text(&self, raw: &str, template_id: Option<&TemplateId>) -> String {
        let trimmed = raw.trim();
        match template_id.and_then(|id| self.template_registry.get(id)) {
            Some(template) => {
                let proc = PostProcessor::new(template.postprocess.clone());
                self.opencc.convert(&proc.process(trimmed))
            }
            None => self.opencc.convert(trimmed),
        }
    }

    pub fn set_conversion_mode(&mut self, mode: ConversionMode) {
        self.opencc.set_mode(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_mode_mapping() {
        assert_eq!(conversion_mode_from_str("zh-TW"), ConversionMode::ZhTW);
        assert_eq!(conversion_mode_from_str("zh-HK"), ConversionMode::ZhHK);
        assert_eq!(conversion_mode_from_str("zh-CN"), ConversionMode::ZhCN);
        assert_eq!(conversion_mode_from_str("raw"), ConversionMode::Raw);
        assert_eq!(conversion_mode_from_str("???"), ConversionMode::ZhTW);
    }

    #[test]
    fn live_pipeline_converts_to_zh_tw_and_trims() {
        let p = TranscriptionPipeline::for_live(true, "zh-TW").unwrap();
        assert_eq!(p.process_text("  软件  ", None), "軟體");
    }

    #[test]
    fn live_pipeline_opencc_disabled_keeps_text() {
        let p = TranscriptionPipeline::for_live(false, "zh-TW").unwrap();
        assert_eq!(p.process_text(" 软件 嗯 ", None), "软件 嗯");
    }

    #[test]
    fn template_postprocess_applied_only_when_requested() {
        let p = TranscriptionPipeline::for_live(false, "raw").unwrap();
        let with = p.process_text("嗯今天開會", Some(&TemplateId::Casual));
        assert!(
            !with.contains('嗯'),
            "fillers removed with template: {with}"
        );
        assert_eq!(p.process_text("嗯今天開會", None), "嗯今天開會");
    }
}
