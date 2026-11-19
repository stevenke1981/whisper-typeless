pub mod capture;
pub mod resampler;
pub mod vad;

pub use capture::AudioCapture;
pub use resampler::Resampler;
pub use vad::{VadEvent, VoiceActivityDetector};

// ─── 共用音訊常數 ──────────────────────────────────────────────────────────────

/// Whisper 標準取樣率 (Hz)
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Resampler 內部 chunk 大小（每批處理樣本數）
pub const RESAMPLER_CHUNK_SIZE: usize = 1024;

/// VAD 預設靜音超時 (ms)
pub const VAD_DEFAULT_SILENCE_TIMEOUT_MS: u64 = 1500;

/// VAD 預設能量閾值
pub const VAD_DEFAULT_THRESHOLD: f32 = 0.6;

/// 最大段落長度 (秒)
pub const MAX_SEGMENT_SECONDS: u32 = 30;

/// 最大段落樣本數 (30s @ 16kHz)
pub const MAX_SEGMENT_SAMPLES: usize =
    (TARGET_SAMPLE_RATE as usize) * (MAX_SEGMENT_SECONDS as usize);

/// 波形顯示 bar 數量
pub const WAVEFORM_BAR_COUNT: usize = 20;

// ─── 結束 ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AudioSegment {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
}
