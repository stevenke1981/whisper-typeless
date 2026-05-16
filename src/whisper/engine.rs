use std::path::Path;
use tracing::{debug, info};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{DecodingStrategy, TranscriptResult, TranscriptSegment, WhisperParams};
use crate::audio::AudioSegment;

pub struct WhisperEngine {
    ctx: WhisperContext,
    pub device_label: String,
}

impl WhisperEngine {
    pub fn load(model_path: &Path, params: &WhisperParams) -> anyhow::Result<Self> {
        info!("載入 Whisper 模型: {}", model_path.display());

        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(params.use_gpu);
        ctx_params.gpu_device(params.gpu_device);

        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("無效路徑"))?,
            ctx_params,
        )
        .map_err(|e| anyhow::anyhow!("模型載入失敗: {e:?}"))?;

        let device_label = if cfg!(feature = "cuda") && params.use_gpu {
            "GPU (CUDA)".to_string()
        } else if cfg!(feature = "metal") && params.use_gpu {
            "GPU (Metal)".to_string()
        } else if params.use_gpu {
            "CPU (需要 cuda 特性)".to_string()
        } else {
            "CPU".to_string()
        };

        info!("模型載入成功，推論裝置: {device_label}");
        Ok(Self { ctx, device_label })
    }

    pub fn transcribe(
        &self,
        segment: &AudioSegment,
        params: &WhisperParams,
    ) -> anyhow::Result<TranscriptResult> {
        debug!("開始轉錄，音訊長度: {}ms", segment.duration_ms);

        let sampling = match params.strategy {
            DecodingStrategy::Greedy => SamplingStrategy::Greedy {
                best_of: params.best_of,
            },
            DecodingStrategy::BeamSearch => SamplingStrategy::BeamSearch {
                beam_size: params.beam_size,
                patience: params.patience,
            },
        };

        let mut fp = FullParams::new(sampling);

        if let Some(lang) = &params.language {
            fp.set_language(Some(lang));
        } else {
            fp.set_language(None);
        }

        fp.set_translate(params.translate);
        fp.set_n_threads(params.n_threads);
        fp.set_temperature(params.temperature);
        fp.set_temperature_inc(params.temperature_inc);
        fp.set_max_tokens(params.max_tokens);
        fp.set_audio_ctx(params.audio_ctx);

        // ICU4X in whisper-rs does not bundle Japanese segmentation data;
        // disable word-level features for "ja" to suppress the runtime error.
        let is_ja = params.language.as_deref() == Some("ja");
        fp.set_token_timestamps(if is_ja {
            false
        } else {
            params.token_timestamps
        });
        fp.set_split_on_word(if is_ja { false } else { params.split_on_word });

        fp.set_thold_pt(params.thold_pt);
        fp.set_max_len(params.max_len);
        fp.set_suppress_blank(params.suppress_blank);
        fp.set_suppress_nst(params.suppress_non_speech);
        fp.set_entropy_thold(params.entropy_thold);
        fp.set_logprob_thold(params.logprob_thold);
        fp.set_no_speech_thold(params.no_speech_thold);

        // Suppress whisper.cpp's internal progress/realtime prints to stderr.
        fp.set_print_realtime(false);
        fp.set_print_progress(false);

        if let Some(prompt) = &params.initial_prompt {
            fp.set_initial_prompt(prompt);
        }

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("無法建立推論狀態: {e:?}"))?;

        state
            .full(fp, &segment.samples)
            .map_err(|e| anyhow::anyhow!("推論失敗: {e:?}"))?;

        let num_segments = state.full_n_segments();

        let mut result_segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let seg = state
                .get_segment(i)
                .ok_or_else(|| anyhow::anyhow!("取得段落失敗: index {i}"))?;
            let text = seg
                .to_str_lossy()
                .map_err(|e| anyhow::anyhow!("取得段落文字失敗: {e:?}"))?
                .into_owned();
            let start_ms = seg.start_timestamp() * 10;
            let end_ms = seg.end_timestamp() * 10;

            full_text.push_str(&text);
            result_segments.push(TranscriptSegment {
                text,
                start_ms,
                end_ms,
            });
        }

        let lang_id = state.full_lang_id_from_state();
        let language = whisper_rs::get_lang_str(lang_id)
            .unwrap_or("zh")
            .to_string();

        debug!("轉錄完成: {} 字", full_text.chars().count());

        Ok(TranscriptResult {
            text: full_text.trim().to_string(),
            language,
            segments: result_segments,
        })
    }
}
