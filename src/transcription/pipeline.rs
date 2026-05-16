use crate::context_templates::{TemplateId, TemplateRegistry};
use crate::whisper::TranscriptResult;

use super::{
    opencc::{ConversionMode, OpenCCProcessor},
    postprocess::PostProcessor,
    ProcessedText,
};

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

    pub fn set_conversion_mode(&mut self, mode: ConversionMode) {
        self.opencc.set_mode(mode);
    }
}
