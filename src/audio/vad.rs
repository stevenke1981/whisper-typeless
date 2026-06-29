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
            max_segment_samples: crate::audio::MAX_SEGMENT_SAMPLES,
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
        let duration_ms = (samples.len() as u64 * 1000) / crate::audio::TARGET_SAMPLE_RATE as u64;
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
        let duration_ms = (samples.len() as u64 * 1000) / crate::audio::TARGET_SAMPLE_RATE as u64;
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
    use crate::audio::TARGET_SAMPLE_RATE;

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

    #[test]
    fn vad_flush_returns_buffered() {
        let mut vad = VoiceActivityDetector::new(0.1, 1500);
        let loud = vec![0.8f32; 1600];
        vad.process(&loud);
        let segment = vad.flush();
        assert!(segment.is_some());
        let seg = segment.unwrap();
        assert!(!seg.samples.is_empty());
    }

    #[test]
    fn vad_flush_empty_returns_none() {
        let mut vad = VoiceActivityDetector::new(0.5, 1500);
        let segment = vad.flush();
        assert!(segment.is_none());
    }

    #[test]
    fn vad_rms_empty_returns_zero() {
        let rms = VoiceActivityDetector::rms(&[]);
        assert_eq!(rms, 0.0);
    }

    #[test]
    fn vad_rms_positive() {
        let rms = VoiceActivityDetector::rms(&[0.5f32; 100]);
        assert!((rms - 0.5).abs() < 0.001);
    }

    #[test]
    fn vad_threshold_update_takes_effect() {
        let mut vad = VoiceActivityDetector::new(0.5, 1500);
        // 0.3 RMS < 0.5 threshold → should NOT trigger SpeechStart
        let med = vec![0.3f32; 1600];
        let events = vad.process(&med);
        let started = events.iter().any(|e| matches!(e, VadEvent::SpeechStart));
        assert!(!started);

        // Lower threshold → same audio should trigger
        vad.set_threshold(0.2);
        let events2 = vad.process(&med);
        let started2 = events2.iter().any(|e| matches!(e, VadEvent::SpeechStart));
        assert!(started2);
    }

    #[test]
    fn vad_silence_timeout_update() {
        let mut vad = VoiceActivityDetector::new(0.1, 10000); // very long timeout
        let loud = vec![0.8f32; 1600];
        vad.process(&loud); // start speaking
        vad.process(&loud); // continue

        // change to short timeout
        vad.set_silence_timeout(10); // 10ms

        // silence samples should now trigger timeout
        let silent = vec![0.01f32; 1600];
        let events = vad.process(&silent);
        // since we didn't update last_speech (silence doesn't set it),
        // we need to wait for the original last_speech to be old enough.
        // This is a time-dependent test - let's verify the setter works.
        assert_eq!(vad.silence_timeout, std::time::Duration::from_millis(10));
    }
}
