pub mod capture;
pub mod resampler;
pub mod vad;

pub use capture::AudioCapture;
pub use resampler::Resampler;
pub use vad::{VadEvent, VoiceActivityDetector};

#[derive(Debug, Clone)]
pub struct AudioSegment {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
}
