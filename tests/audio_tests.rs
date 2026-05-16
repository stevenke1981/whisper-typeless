use whisper_typeless::audio::{VadEvent, VoiceActivityDetector};

#[test]
fn vad_silent_produces_no_segment() {
    let mut vad = VoiceActivityDetector::new(0.5, 1500);
    let silent = vec![0.01f32; 1600];
    let events = vad.process(&silent);
    let has_segment = events.iter().any(|e| matches!(e, VadEvent::Segment(_)));
    assert!(!has_segment);
}

#[test]
fn vad_loud_starts_speaking() {
    let mut vad = VoiceActivityDetector::new(0.1, 1500);
    let loud = vec![0.9f32; 1600];
    let events = vad.process(&loud);
    let started = events.iter().any(|e| matches!(e, VadEvent::SpeechStart));
    assert!(started);
}

#[test]
fn vad_accumulates_audio_during_speech() {
    let mut vad = VoiceActivityDetector::new(0.1, 10000); // 長超時
    let loud = vec![0.9f32; 1600];
    for _ in 0..5 {
        vad.process(&loud);
    }
    // 沒有超過最大長度，不應產生 Segment
    let events = vad.process(&loud);
    let has_segment = events.iter().any(|e| matches!(e, VadEvent::Segment(_)));
    assert!(!has_segment);
}
