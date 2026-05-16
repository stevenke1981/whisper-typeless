use whisper_typeless::transcription::{
    opencc::{ConversionMode, OpenCCProcessor},
    postprocess::{PostProcessSettings, PostProcessor},
};

#[test]
fn opencc_raw_preserves_input() {
    let proc = OpenCCProcessor::new(ConversionMode::Raw).unwrap();
    assert_eq!(proc.convert("測試文字"), "測試文字");
}

#[test]
fn postprocess_removes_fillers() {
    let settings = PostProcessSettings {
        remove_fillers: vec!["嗯".into(), "啊".into()],
        ..Default::default()
    };
    let proc = PostProcessor::new(settings);
    let result = proc.process("嗯這個問題啊很重要");
    assert!(!result.contains("嗯"));
    assert!(!result.contains("啊"));
    assert!(result.contains("這個問題"));
}

#[test]
fn postprocess_normalizes_punctuation() {
    let settings = PostProcessSettings {
        add_punctuation: true,
        ..Default::default()
    };
    let proc = PostProcessor::new(settings);
    let result = proc.process("好的,我明白了.");
    assert!(result.contains("，") || result.contains("。"));
}

#[test]
fn context_detector_picks_meeting() {
    use whisper_typeless::context_templates::{
        detector::ContextDetector, TemplateId, TemplateRegistry,
    };
    let detector = ContextDetector::new(TemplateRegistry::new());
    let text = "今天的會議討論了預算和季度目標";
    assert_eq!(detector.detect(text), TemplateId::Meeting);
}
