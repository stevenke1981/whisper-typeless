use rubato::{FftFixedIn, Resampler as RubatoResampler};

const TARGET_SAMPLE_RATE: u32 = 16000;
const CHUNK_SIZE: usize = 1024;

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
            CHUNK_SIZE,
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

        while self.buffer.len() >= CHUNK_SIZE {
            let chunk: Vec<f32> = self.buffer.drain(..CHUNK_SIZE).collect();
            let waves_out = resampler.process(&[chunk], None)?;
            output.extend(waves_out.into_iter().next().unwrap_or_default());
        }

        Ok(output)
    }

    pub fn target_rate() -> u32 {
        TARGET_SAMPLE_RATE
    }
}
