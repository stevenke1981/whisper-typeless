use anyhow::{Context, Result};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::Resampler;
use crate::whisper::TranscriptSegment;

pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
}

pub fn decode_audio_file(path: &Path) -> Result<DecodedAudio> {
    let src = std::fs::File::open(path).with_context(|| format!("無法開啟: {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("不支援的音訊格式")?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .context("找不到音訊軌")?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.context("無法取得取樣率")?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("無法建立解碼器")?;

    let mut raw_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(e.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };

        let spec = *decoded.spec();
        let frames = decoded.frames();
        let mut sample_buf = SampleBuffer::<f32>::new(frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        let channels = spec.channels.count();
        let samples = sample_buf.samples();

        // mix down to mono
        if channels == 1 {
            raw_samples.extend_from_slice(samples);
        } else {
            for chunk in samples.chunks(channels) {
                let mono = chunk.iter().sum::<f32>() / channels as f32;
                raw_samples.push(mono);
            }
        }
    }

    // resample to 16kHz if needed
    let final_samples = if sample_rate != 16000 {
        let mut resampler = Resampler::new(sample_rate)?;
        resampler.process(&raw_samples)?
    } else {
        raw_samples
    };

    let duration_ms = (final_samples.len() as u64 * 1000) / 16000;

    Ok(DecodedAudio {
        samples: final_samples,
        duration_ms,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportFormat {
    Txt,
    Srt,
    Vtt,
}

impl ExportFormat {
    pub fn from_extension(s: &str) -> Self {
        match s {
            "srt" => Self::Srt,
            "vtt" => Self::Vtt,
            _ => Self::Txt,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Srt => "srt",
            Self::Vtt => "vtt",
        }
    }
}

pub fn format_transcript(segments: &[TranscriptSegment], format: ExportFormat) -> String {
    match format {
        ExportFormat::Txt => segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        ExportFormat::Srt => format_srt(segments),
        ExportFormat::Vtt => format_vtt(segments),
    }
}

fn ms_to_srt(ms: i64) -> String {
    let ms = ms.max(0) as u64;
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, millis)
}

fn ms_to_vtt(ms: i64) -> String {
    let ms = ms.max(0) as u64;
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis)
}

fn format_srt(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .enumerate()
        .map(|(i, seg)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                i + 1,
                ms_to_srt(seg.start_ms),
                ms_to_srt(seg.end_ms),
                seg.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_vtt(segments: &[TranscriptSegment]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for seg in segments {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            ms_to_vtt(seg.start_ms),
            ms_to_vtt(seg.end_ms),
            seg.text.trim()
        ));
    }
    out
}
