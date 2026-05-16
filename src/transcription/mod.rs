pub mod opencc;
pub mod pipeline;
pub mod postprocess;

pub use pipeline::TranscriptionPipeline;

#[derive(Debug, Clone)]
pub struct ProcessedText {
    pub text: String,
    pub raw_text: String,
    pub language: String,
}
