use super::AudioSegment;
use std::time::{Duration, Instant};
use tracing::debug;

#[derive(Debug, Clone)]
pub enum VadEvent {
    SpeechStart,
    SpeechContinue { level: f32 },
    Segment(AudioSegment),
    Silence,
}

pub struct VoiceActivityDetector {
    threshold: f32,
    silence_timeout: Duration,
    buffer: Vec<f32>,
    last_speech: Option<Instant>,
    speaking: bool,
    max_segment_samples: usize,
}

impl VoiceActivityDetector {
    pub fn new(threshold: f32, silence_timeout_ms: u64) -> Self {
        Self {
            threshold,
            silence_timeout: Duration::from_millis(silence_timeout_ms),
            buffer: Vec::new(),
            last_speech: None,
            speaking: false,
            // 30 seconds at 16kHz
            max_segment_samples: 16000 * 30,
        }
    }

    pub fn process(&mut self, samples: &[f32]) -> Vec<VadEvent> {
        if samples.is_empty() {
            return Vec::new();
        }
        let rms = Self::rms(samples);
        debug!(
            "VAD rms={:.4} threshold={:.4} speaking={}",
            rms, self.threshold, self.speaking
        );
        let mut events = Vec::new();

        if rms > self.threshold {
            if !self.speaking {
                self.speaking = true;
                events.push(VadEvent::SpeechStart);
            }
            self.last_speech = Some(Instant::now());
            self.buffer.extend_from_slice(samples);
            events.push(VadEvent::SpeechContinue { level: rms });

            if self.buffer.len() >= self.max_segment_samples {
                events.push(self.flush_segment());
            }
        } else if self.speaking {
            self.buffer.extend_from_slice(samples);

            let timed_out = self
                .last_speech
                .map(|t| t.elapsed() > self.silence_timeout)
                .unwrap_or(false);

            if timed_out {
                self.speaking = false;
                events.push(self.flush_segment());
                events.push(VadEvent::Silence);
            }
        }

        events
    }

    fn flush_segment(&mut self) -> VadEvent {
        let samples = std::mem::take(&mut self.buffer);
        let duration_ms = (samples.len() as u64 * 1000) / 16000;
        VadEvent::Segment(AudioSegment {
            samples,
            duration_ms,
        })
    }

    /// Force-flush any buffered audio (call on stop to capture final segment).
    pub fn flush(&mut self) -> Option<AudioSegment> {
        if self.buffer.is_empty() {
            return None;
        }
        let samples = std::mem::take(&mut self.buffer);
        self.speaking = false;
        self.last_speech = None;
        let duration_ms = (samples.len() as u64 * 1000) / 16000;
        Some(AudioSegment {
            samples,
            duration_ms,
        })
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    pub fn set_silence_timeout(&mut self, ms: u64) {
        self.silence_timeout = Duration::from_millis(ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_produces_no_segment() {
        let mut vad = VoiceActivityDetector::new(0.5, 1500);
        let silent = vec![0.01f32; 1600];
        let events = vad.process(&silent);
        assert!(events.is_empty() || events.iter().all(|e| !matches!(e, VadEvent::Segment(_))));
    }

    #[test]
    fn loud_audio_starts_speech() {
        let mut vad = VoiceActivityDetector::new(0.1, 1500);
        let loud = vec![0.8f32; 1600];
        let events = vad.process(&loud);
        assert!(events.iter().any(|e| matches!(e, VadEvent::SpeechStart)));
    }
}
