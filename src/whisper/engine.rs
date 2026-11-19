use std::path::Path;
use tracing::{debug, info, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{DecodingStrategy, TranscriptResult, TranscriptSegment, WhisperParams};
use crate::audio::AudioSegment;

/// 此版本編進了哪個 GPU 後端（沒有則為 `None`）。
pub fn gpu_backend_name() -> Option<&'static str> {
    if cfg!(feature = "cuda") {
        Some("CUDA")
    } else if cfg!(feature = "vulkan") {
        Some("Vulkan")
    } else if cfg!(feature = "metal") {
        Some("Metal")
    } else {
        None
    }
}

/// GPU 執行環境是否可用。Vulkan 版以 LoadLibrary 探測 vulkan-1.dll
/// （該 DLL 為延遲載入，缺少時不探測就呼叫會直接當掉）；其他後端視為可用。
pub fn gpu_runtime_available() -> bool {
    #[cfg(all(feature = "vulkan", windows))]
    {
        use std::sync::OnceLock;
        static PROBE: OnceLock<bool> = OnceLock::new();
        *PROBE.get_or_init(probe_vulkan_dll)
    }
    #[cfg(not(all(feature = "vulkan", windows)))]
    {
        gpu_backend_name().is_some()
    }
}

#[cfg(all(feature = "vulkan", windows))]
fn probe_vulkan_dll() -> bool {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }
    let name: Vec<u16> = "vulkan-1.dll\0".encode_utf16().collect();
    // SAFETY: name 以 NUL 結尾；成功時立即釋放，只是確認 DLL 可載入。
    unsafe {
        let h = LoadLibraryW(name.as_ptr());
        if h.is_null() {
            false
        } else {
            FreeLibrary(h);
            true
        }
    }
}

/// 推論裝置標籤。`gpu_active` 表示實際以 GPU 載入成功。
fn device_label_for(use_gpu: bool, gpu_active: bool, gpu_failed: bool) -> String {
    match (gpu_backend_name(), use_gpu) {
        (_, false) => "CPU".to_string(),
        (Some(name), true) if gpu_active => format!("GPU ({name})"),
        (Some(_), true) if gpu_failed => "CPU（GPU 初始化失敗）".to_string(),
        (Some(name), true) => format!("GPU ({name})"),
        (None, true) => "CPU（此版本未含 GPU）".to_string(),
    }
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    pub device_label: String,
}

impl WhisperEngine {
    pub fn load(model_path: &Path, params: &WhisperParams) -> anyhow::Result<Self> {
        info!("載入 Whisper 模型: {}", model_path.display());

        let path = model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("無效路徑"))?;
        let compiled = gpu_backend_name().is_some();
        let want_gpu = params.use_gpu && compiled;

        let load = |use_gpu: bool| {
            let mut ctx_params = WhisperContextParameters::default();
            ctx_params.use_gpu(use_gpu);
            ctx_params.gpu_device(params.gpu_device);
            WhisperContext::new_with_params(path, ctx_params)
        };

        let mut gpu_failed = false;
        let ctx = if want_gpu && gpu_runtime_available() {
            match load(true) {
                Ok(ctx) => ctx,
                Err(e) => {
                    warn!("GPU 初始化失敗，改用 CPU: {e:?}");
                    gpu_failed = true;
                    load(false).map_err(|e| anyhow::anyhow!("模型載入失敗: {e:?}"))?
                }
            }
        } else if want_gpu {
            warn!("找不到 GPU 執行環境（vulkan-1.dll），改用 CPU");
            gpu_failed = true;
            load(false).map_err(|e| anyhow::anyhow!("模型載入失敗: {e:?}"))?
        } else {
            load(false).map_err(|e| anyhow::anyhow!("模型載入失敗: {e:?}"))?
        };

        let device_label = device_label_for(params.use_gpu, want_gpu && !gpu_failed, gpu_failed);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_cpu_when_gpu_disabled() {
        assert_eq!(device_label_for(false, false, false), "CPU");
    }

    #[test]
    fn label_reflects_build_features() {
        let l = device_label_for(true, true, false);
        match gpu_backend_name() {
            Some(name) => assert_eq!(l, format!("GPU ({name})")),
            None => assert_eq!(l, "CPU（此版本未含 GPU）"),
        }
    }

    #[test]
    fn label_reports_gpu_failure_when_compiled() {
        if gpu_backend_name().is_some() {
            assert_eq!(device_label_for(true, false, true), "CPU（GPU 初始化失敗）");
        }
    }
}
