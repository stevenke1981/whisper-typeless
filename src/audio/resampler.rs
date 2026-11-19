use rubato::{FftFixedIn, Resampler as RubatoResampler};

use crate::audio::{RESAMPLER_CHUNK_SIZE, TARGET_SAMPLE_RATE};

pub struct Resampler {
    inner: Option<FftFixedIn<f32>>,
    // accumulates input samples until we have a full CHUNK_SIZE
    buffer: Vec<f32>,
}

impl Resampler {
    pub fn new(source_rate: u32) -> anyhow::Result<Self> {
        if source_rate == TARGET_SAMPLE_RATE {
            return Ok(Self {
                inner: None,
                buffer: Vec::new(),
            });
        }

        let inner = FftFixedIn::<f32>::new(
            source_rate as usize,
            TARGET_SAMPLE_RATE as usize,
            RESAMPLER_CHUNK_SIZE,
            2,
            1,
        )?;

        Ok(Self {
            inner: Some(inner),
            buffer: Vec::new(),
        })
    }

    /// Accepts any number of samples; returns resampled mono at 16 kHz.
    /// Internally accumulates until a full CHUNK_SIZE is ready.
    pub fn process(&mut self, input: &[f32]) -> anyhow::Result<Vec<f32>> {
        let Some(resampler) = &mut self.inner else {
            return Ok(input.to_vec());
        };

        self.buffer.extend_from_slice(input);
        let mut output = Vec::new();

        while self.buffer.len() >= RESAMPLER_CHUNK_SIZE {
            let chunk: Vec<f32> = self.buffer.drain(..RESAMPLER_CHUNK_SIZE).collect();
            let waves_out = resampler.process(&[chunk], None)?;
            output.extend(waves_out.into_iter().next().unwrap_or_default());
        }

        Ok(output)
    }

    pub fn target_rate() -> u32 {
        TARGET_SAMPLE_RATE
    }

    /// The chunk size used for internal resampling.
    pub fn chunk_size() -> usize {
        RESAMPLER_CHUNK_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rate: u32, n: usize) -> usize {
        let mut r = Resampler::new(rate).unwrap();
        let input = vec![0.1f32; n];
        r.process(&input).unwrap().len()
    }

    #[test]
    fn passthrough_at_target_rate() {
        let mut r = Resampler::new(TARGET_SAMPLE_RATE).unwrap();
        let input: Vec<f32> = (0..777).map(|i| i as f32 / 1000.0).collect();
        assert_eq!(r.process(&input).unwrap(), input);
    }

    #[test]
    fn downsample_48k_ratio_is_one_third() {
        let n = RESAMPLER_CHUNK_SIZE * 20;
        let out = run(48_000, n);
        let expected = n / 3;
        assert!(out.abs_diff(expected) < expected / 5, "out={out}");
    }

    #[test]
    fn downsample_44100_ratio() {
        let n = RESAMPLER_CHUNK_SIZE * 20;
        let out = run(44_100, n);
        let expected = n as f64 * 16_000.0 / 44_100.0;
        assert!((out as f64 - expected).abs() < expected * 0.2, "out={out}");
    }

    #[test]
    fn upsample_8k_doubles() {
        let n = RESAMPLER_CHUNK_SIZE * 10;
        let out = run(8_000, n);
        let expected = n * 2;
        assert!(out.abs_diff(expected) < expected / 5, "out={out}");
    }

    #[test]
    fn short_input_buffers_without_output() {
        assert_eq!(run(48_000, RESAMPLER_CHUNK_SIZE - 1), 0);
    }

    #[test]
    fn chunked_input_accumulates() {
        let mut r = Resampler::new(48_000).unwrap();
        let half = vec![0.0f32; RESAMPLER_CHUNK_SIZE / 2];
        assert!(r.process(&half).unwrap().is_empty());
        assert!(!r.process(&half).unwrap().is_empty());
    }

    #[test]
    fn constants_exposed() {
        assert_eq!(Resampler::target_rate(), 16_000);
        assert_eq!(Resampler::chunk_size(), RESAMPLER_CHUNK_SIZE);
    }
}
