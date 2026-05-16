pub mod engine;
pub mod params;
pub mod session;

pub use engine::WhisperEngine;
pub use params::{DecodingStrategy, WhisperParams};

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub language: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}
