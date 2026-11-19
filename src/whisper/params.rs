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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_default_values() {
        let p = WhisperParams::default();
        assert_eq!(p.language, Some("zh".into()));
        assert_eq!(p.n_threads, 4);
        assert_eq!(p.strategy, DecodingStrategy::Greedy);
        assert!(p.use_gpu);
        assert!(p.suppress_blank);
        assert_eq!(p.temperature, 0.0);
        assert_eq!(p.entropy_thold, 2.4);
        assert_eq!(p.no_speech_thold, 0.3);
    }

    #[test]
    fn params_serde_roundtrip() {
        let p = WhisperParams::default();
        let json = serde_json::to_string(&p).unwrap();
        let restored: WhisperParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p.language, restored.language);
        assert_eq!(p.strategy, restored.strategy);
        assert_eq!(p.temperature, restored.temperature);
        assert_eq!(p.n_threads, restored.n_threads);
    }

    #[test]
    fn params_full_roundtrip_non_default() {
        let p = WhisperParams {
            language: None,
            strategy: DecodingStrategy::BeamSearch,
            beam_size: 8,
            initial_prompt: Some("提示".into()),
            temperature: 0.4,
            use_gpu: false,
            ..Default::default()
        };
        let json = serde_json::to_string(&p).unwrap();
        let r: WhisperParams = serde_json::from_str(&json).unwrap();
        assert_eq!(r.language, None);
        assert_eq!(r.strategy, DecodingStrategy::BeamSearch);
        assert_eq!(r.beam_size, 8);
        assert_eq!(r.initial_prompt.as_deref(), Some("提示"));
        assert!(!r.use_gpu);
        assert!((r.temperature - 0.4).abs() < 1e-6);
    }

    #[test]
    fn params_default_misc_values() {
        let p = WhisperParams::default();
        assert!(!p.translate);
        assert!(p.initial_prompt.is_none());
        assert_eq!(p.beam_size, 5);
        assert_eq!(p.best_of, 5);
        assert_eq!(p.gpu_device, 0);
    }

    #[test]
    fn params_missing_field_is_error() {
        assert!(serde_json::from_str::<WhisperParams>("{}").is_err());
    }

    #[test]
    fn decoding_strategy_serde_names() {
        assert_eq!(
            serde_json::to_string(&DecodingStrategy::BeamSearch).unwrap(),
            "\"BeamSearch\""
        );
        let s: DecodingStrategy = serde_json::from_str("\"Greedy\"").unwrap();
        assert_eq!(s, DecodingStrategy::Greedy);
    }

    #[test]
    fn params_strategy_default_is_greedy() {
        assert_eq!(DecodingStrategy::default(), DecodingStrategy::Greedy);
    }
}
