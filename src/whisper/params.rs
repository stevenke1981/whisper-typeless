use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum DecodingStrategy {
    #[default]
    Greedy,
    BeamSearch,
}

/// 完整 whisper.cpp 參數映射
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperParams {
    // 語言
    pub language: Option<String>,
    pub translate: bool,

    // 情境提示
    pub initial_prompt: Option<String>,

    // 執行緒
    pub n_threads: i32,

    // 解碼
    pub strategy: DecodingStrategy,
    pub beam_size: i32,
    pub best_of: i32,
    pub temperature: f32,
    pub temperature_inc: f32,
    pub patience: f32,

    // Token 限制
    pub max_tokens: i32,
    pub audio_ctx: i32,

    // 時間戳
    pub timestamps: bool,
    pub token_timestamps: bool,
    pub thold_pt: f32,
    pub max_len: i32,
    pub split_on_word: bool,

    // 過濾
    pub suppress_blank: bool,
    pub suppress_non_speech: bool,
    pub entropy_thold: f32,
    pub logprob_thold: f32,
    pub no_speech_thold: f32,
    pub speed_up: bool,

    // GPU
    pub use_gpu: bool,
    pub gpu_device: i32,
}

impl Default for WhisperParams {
    fn default() -> Self {
        Self {
            language: Some("zh".into()),
            translate: false,
            initial_prompt: None,
            n_threads: 4,
            strategy: DecodingStrategy::Greedy,
            beam_size: 5,
            best_of: 5,
            temperature: 0.0,
            temperature_inc: 0.2,
            patience: -1.0,
            max_tokens: 0,
            audio_ctx: 0,
            timestamps: false,
            token_timestamps: false,
            thold_pt: 0.01,
            max_len: 0,
            split_on_word: false,
            suppress_blank: true,
            suppress_non_speech: false,
            entropy_thold: 2.4,
            logprob_thold: -1.0,
            no_speech_thold: 0.3,
            speed_up: false,
            use_gpu: true,
            gpu_device: 0,
        }
    }
}
