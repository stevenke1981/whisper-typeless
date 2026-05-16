use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct AudioCapture {
    device: Device,
    config: StreamConfig,
}

impl AudioCapture {
    pub fn new(device_name: Option<&str>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) => host
                .input_devices()?
                .find(|d| d.name().ok().as_deref() == Some(name))
                .ok_or_else(|| anyhow::anyhow!("找不到音訊裝置: {name}"))?,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("無預設輸入裝置"))?,
        };

        info!("使用音訊裝置: {}", device.name()?);

        let config = device.default_input_config()?;
        let stream_config = StreamConfig {
            channels: config.channels(),
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        info!(
            "音訊設定: {}ch @ {}Hz",
            stream_config.channels, stream_config.sample_rate.0
        );

        Ok(Self {
            device,
            config: stream_config,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }

    pub fn start(
        &self,
        tx: mpsc::Sender<Vec<f32>>,
        level_tx: mpsc::Sender<f32>,
    ) -> anyhow::Result<Stream> {
        let channels = self.config.channels as usize;
        let stream = self.device.build_input_stream(
            &self.config,
            move |data: &[f32], _| {
                // mix down to mono by averaging all channels
                let mono: Vec<f32> = if channels == 1 {
                    data.to_vec()
                } else {
                    data.chunks_exact(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect()
                };
                let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt();
                let _ = level_tx.try_send(rms);
                let _ = tx.try_send(mono);
            },
            |err| error!("音訊串流錯誤: {err}"),
            None,
        )?;

        stream.play()?;
        Ok(stream)
    }

    pub fn list_devices() -> anyhow::Result<Vec<String>> {
        let host = cpal::default_host();
        Ok(host
            .input_devices()?
            .filter_map(|d| d.name().ok())
            .collect())
    }
}
