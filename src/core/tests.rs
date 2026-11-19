//! 控制器 / 通知器單元測試（假 Backend，不碰音訊、GPU、網路或模型檔）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
use crate::app::RecordingState;
use crate::audio::WAVEFORM_BAR_COUNT;
use crate::settings::config::AppConfig;
use crate::settings::ui_settings::UiSettings;
use crate::whisper::WhisperParams;

// ─── 假 Backend ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Call {
    StartRecording(RecordingRequest),
    StopRecording,
    SelectOrDownload { model_id: String, confirm: bool },
    Preload(String),
    SaveSettings(UiSettings),
    ApplyHotkeys(String, String, bool),
    OpenModelFolder,
    PickFile,
    FileTranscription(FileTranscriptionRequest),
    Copy(String),
    Export(String, String, String),
    SaveTemplate(String, String, String),
    DeleteTemplate(String),
    ClearErrorLog,
}

#[derive(Default)]
struct Shared {
    calls: Vec<Call>,
    recording: bool,
    next_start: Option<RecordingStart>,
    save_error: Option<String>,
    preload_ok: bool,
    error_log: Vec<String>,
}

#[derive(Clone, Default)]
struct FakeBackend(Arc<Mutex<Shared>>);

impl FakeBackend {
    fn calls(&self) -> Vec<Call> {
        self.0.lock().unwrap().calls.clone()
    }
    fn push(&self, c: Call) {
        self.0.lock().unwrap().calls.push(c);
    }
}

impl Backend for FakeBackend {
    fn is_recording(&self) -> bool {
        self.0.lock().unwrap().recording
    }
    fn start_recording(&mut self, req: RecordingRequest) -> RecordingStart {
        self.push(Call::StartRecording(req));
        let mut s = self.0.lock().unwrap();
        let r = s.next_start.take().unwrap_or(RecordingStart::Started);
        s.recording = r == RecordingStart::Started;
        r
    }
    fn stop_recording(&mut self) {
        self.push(Call::StopRecording);
        self.0.lock().unwrap().recording = false;
    }
    fn select_or_download_model(&mut self, model_id: String, _p: WhisperParams, confirm: bool) {
        self.push(Call::SelectOrDownload { model_id, confirm });
    }
    fn preload_model(&mut self, model_id: String, _p: WhisperParams) -> bool {
        self.push(Call::Preload(model_id));
        self.0.lock().unwrap().preload_ok
    }
    fn save_settings(&mut self, settings: &UiSettings) -> Result<(), String> {
        self.push(Call::SaveSettings(settings.clone()));
        match self.0.lock().unwrap().save_error.clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    fn apply_hotkeys(&mut self, hotkey: &str, ptt_hotkey: &str, ptt_enabled: bool) {
        self.push(Call::ApplyHotkeys(
            hotkey.into(),
            ptt_hotkey.into(),
            ptt_enabled,
        ));
    }
    fn open_model_folder(&mut self) {
        self.push(Call::OpenModelFolder);
    }
    fn pick_audio_file(&mut self) {
        self.push(Call::PickFile);
    }
    fn start_file_transcription(&mut self, req: FileTranscriptionRequest) {
        self.push(Call::FileTranscription(req));
    }
    fn copy_text(&mut self, text: String) {
        self.push(Call::Copy(text));
    }
    fn export_text(&mut self, text: String, source_file: String, format: String) {
        self.push(Call::Export(text, source_file, format));
    }
    fn save_template(&mut self, original: String, name: String, prompt: String) {
        self.push(Call::SaveTemplate(original, name, prompt));
    }
    fn delete_template(&mut self, name: String) {
        self.push(Call::DeleteTemplate(name));
    }
    fn clear_error_log(&mut self) {
        self.push(Call::ClearErrorLog);
        self.0.lock().unwrap().error_log.clear();
    }
    fn error_log_snapshot(&self) -> Vec<String> {
        self.0.lock().unwrap().error_log.clone()
    }
}

fn controller() -> (Controller, FakeBackend) {
    let fake = FakeBackend::default();
    let c = Controller::new(
        AppViewState::default(),
        AppConfig::default(),
        Box::new(fake.clone()),
    );
    (c, fake)
}

/// 經過真正的 Notifier → EventQueue 再套用到控制器。
fn pump(c: &mut Controller, queue: &EventQueue) {
    for ev in queue.drain() {
        c.apply(ev);
    }
}

// ─── Notifier ────────────────────────────────────────────────────────────────

#[test]
fn notifier_is_send_sync_clone() {
    fn assert_traits<T: Send + Sync + Clone + 'static>() {}
    assert_traits::<Notifier>();
}

#[test]
fn notifier_wakes_once_per_batch_and_keeps_order() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let w = Arc::clone(&wakes);
    let (n, q) = event_channel(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    n.send(CoreEvent::Status("a".into()));
    n.clone().send(CoreEvent::Status("b".into()));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(
        q.drain(),
        vec![CoreEvent::Status("a".into()), CoreEvent::Status("b".into())]
    );
    n.send(CoreEvent::Silence);
    assert_eq!(wakes.load(Ordering::SeqCst), 2);
    assert_eq!(q.drain(), vec![CoreEvent::Silence]);
    assert!(q.drain().is_empty());
}

#[test]
fn notifier_coalesces_waveform_to_latest() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let w = Arc::clone(&wakes);
    let (n, q) = event_channel(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    n.send(CoreEvent::Waveform(vec![0.1]));
    n.send(CoreEvent::Status("x".into()));
    n.send(CoreEvent::Waveform(vec![0.2]));
    n.send(CoreEvent::Waveform(vec![0.3]));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(
        q.drain(),
        vec![
            CoreEvent::Waveform(vec![0.3]),
            CoreEvent::Status("x".into())
        ]
    );
    // 合併旗標已重設：下一筆波形會再進佇列
    n.send(CoreEvent::Waveform(vec![0.4]));
    assert_eq!(q.drain(), vec![CoreEvent::Waveform(vec![0.4])]);
}

#[test]
fn notifier_survives_dropped_queue_and_other_threads() {
    let (n, q) = event_channel(|| {});
    let n2 = n.clone();
    std::thread::spawn(move || n2.send(CoreEvent::PipelineFinished))
        .join()
        .unwrap();
    assert_eq!(q.drain(), vec![CoreEvent::PipelineFinished]);
    drop(q);
    n.send(CoreEvent::Silence); // 不可 panic
}

// ─── 錄音狀態機 ──────────────────────────────────────────────────────────────

#[test]
fn recording_full_cycle_through_notifier() {
    let (mut c, fake) = controller();
    let (n, q) = event_channel(|| {});
    assert_eq!(c.state().recording_state(), RecordingState::Idle);

    c.handle(UiCommand::ToggleRecording);
    assert!(c.state().is_recording);
    assert_eq!(c.state().status_text, "● 錄音中...");
    assert_eq!(c.state().recording_state(), RecordingState::Recording);

    n.send(CoreEvent::SpeechStarted);
    n.send(CoreEvent::Level(0.7));
    n.send(CoreEvent::Waveform(vec![0.5; WAVEFORM_BAR_COUNT]));
    pump(&mut c, &q);
    assert_eq!(c.state().current_partial, "聆聽中...");
    assert!(c.state().is_processing);
    assert_eq!(c.state().audio_level, 0.7);
    assert_eq!(c.state().waveform_bars, vec![0.5; WAVEFORM_BAR_COUNT]);

    c.handle(UiCommand::ToggleRecording);
    let s = c.state();
    assert!(!s.is_recording);
    assert!(!s.is_processing);
    assert_eq!(s.status_text, "就緒");
    assert_eq!(s.audio_level, 0.0);
    assert_eq!(s.current_partial, "");
    assert_eq!(s.waveform_bars, vec![0.0; WAVEFORM_BAR_COUNT]);

    n.send(CoreEvent::TranscribeStarted);
    pump(&mut c, &q);
    assert_eq!(c.state().recording_state(), RecordingState::Processing);
    assert_eq!(c.state().current_partial, "⏳ 轉錄中...");

    n.send(CoreEvent::TranscriptSegment {
        text: "你好世界".into(),
        timestamp: "12:00:00".into(),
        status: "已轉錄".into(),
    });
    n.send(CoreEvent::PipelineFinished);
    pump(&mut c, &q);
    let s = c.state();
    assert_eq!(s.recording_state(), RecordingState::Idle);
    assert_eq!(s.transcript_segments.len(), 1);
    assert_eq!(s.char_count, 4);
    assert_eq!(s.status_text, "已轉錄");
    assert_eq!(s.current_partial, "");

    let calls = fake.calls();
    assert!(matches!(calls[0], Call::StartRecording(_)));
    assert!(matches!(calls[1], Call::StopRecording));

    c.handle(UiCommand::ClearTranscript);
    assert!(c.state().transcript_segments.is_empty());
    assert_eq!(c.state().char_count, 0);
}

#[test]
fn recording_request_reflects_current_state() {
    let (mut c, fake) = controller();
    {
        let s = c.state_mut();
        s.vad_threshold = 0.3;
        s.silence_timeout = 800.4;
        s.clipboard_enabled = false;
        s.mod_opencc = false;
        s.conversion_mode = "zh-HK".into();
        s.n_threads = 2;
        s.use_gpu = false;
    }
    c.handle(UiCommand::ToggleRecording);
    let Call::StartRecording(req) = &fake.calls()[0] else {
        panic!("expected StartRecording");
    };
    assert_eq!(req.vad_threshold, 0.3);
    assert_eq!(req.vad_timeout_ms, 800);
    assert!(!req.clipboard_enabled);
    assert!(req.inject_enabled);
    assert!(!req.opencc_enabled);
    assert_eq!(req.conversion_mode, "zh-HK");
    assert_eq!(req.whisper_params.n_threads, 2);
    assert!(!req.whisper_params.use_gpu);
}

#[test]
fn missing_model_asks_to_download() {
    let (mut c, fake) = controller();
    fake.0.lock().unwrap().next_start = Some(RecordingStart::ModelMissing {
        model_id: "small".into(),
    });
    c.handle(UiCommand::ToggleRecording);
    assert!(!c.state().is_recording);
    assert_eq!(c.state().status_text, "模型尚未下載，請確認是否自動下載");
    assert!(fake.calls().iter().any(|call| matches!(
        call,
        Call::SelectOrDownload { model_id, confirm: true } if model_id == "small"
    )));
}

#[test]
fn failed_start_shows_message_and_stays_idle() {
    let (mut c, fake) = controller();
    fake.0.lock().unwrap().next_start = Some(RecordingStart::Failed("音訊裝置錯誤: x".into()));
    c.handle(UiCommand::ToggleRecording);
    assert!(!c.state().is_recording);
    assert_eq!(c.state().status_text, "音訊裝置錯誤: x");
}

#[test]
fn ptt_and_hotkey_events_drive_recording() {
    let (mut c, fake) = controller();
    c.apply(CoreEvent::PttKey(true));
    assert!(c.state().is_recording);
    c.apply(CoreEvent::PttKey(true)); // 重複按下：不再切換
    assert!(c.state().is_recording);
    c.apply(CoreEvent::PttKey(false));
    assert!(!c.state().is_recording);
    c.apply(CoreEvent::PttKey(false)); // 已停止：不切換
    c.apply(CoreEvent::HotkeyPressed);
    assert!(c.state().is_recording);
    c.apply(CoreEvent::HotkeyPressed);
    assert!(!c.state().is_recording);
    let starts = fake
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::StartRecording(_)))
        .count();
    assert_eq!(starts, 2);
}

#[test]
fn idle_only_status_is_ignored_while_recording() {
    let (mut c, _fake) = controller();
    c.apply(CoreEvent::StatusIfIdle("預載模型 small...".into()));
    assert_eq!(c.state().status_text, "預載模型 small...");
    c.handle(UiCommand::ToggleRecording);
    c.apply(CoreEvent::StatusIfIdle("模型已預載: small".into()));
    assert_eq!(c.state().status_text, "● 錄音中...");
    c.apply(CoreEvent::DeviceLabel("CUDA".into()));
    assert_eq!(c.state().device_label, "CUDA");
}

#[test]
fn transcription_errors_reset_processing() {
    let (mut c, _fake) = controller();
    c.apply(CoreEvent::ModelLoading);
    assert!(c.state().is_processing);
    c.apply(CoreEvent::ModelLoadFailed("模型載入失敗: x".into()));
    assert!(!c.state().is_processing);
    c.apply(CoreEvent::TranscribeStarted);
    c.apply(CoreEvent::TranscriptionFailed("轉錄錯誤: y".into()));
    assert!(!c.state().is_processing);
    assert_eq!(c.state().status_text, "轉錄錯誤: y");
    c.apply(CoreEvent::TranscribeStarted);
    c.apply(CoreEvent::TranscriptEmpty);
    assert!(!c.state().is_processing);
    assert_eq!(c.state().current_partial, "");
}

// ─── 模型下載 ────────────────────────────────────────────────────────────────

#[test]
fn download_progress_flow() {
    let (mut c, fake) = controller();
    let (n, q) = event_channel(|| {});

    c.handle(UiCommand::ModelSelectOrDownload("base".into()));
    assert!(fake.calls().iter().any(|call| matches!(
        call,
        Call::SelectOrDownload { model_id, confirm: false } if model_id == "base"
    )));

    n.send(CoreEvent::DownloadStarted {
        model_id: "base".into(),
        size_mb: 142,
    });
    pump(&mut c, &q);
    assert_eq!(c.state().downloading_id, "base");
    assert_eq!(c.state().download_progress, 0.0);
    assert_eq!(c.state().status_text, "下載模型 (142 MB)...");

    n.send(CoreEvent::DownloadProgress(0.25));
    n.send(CoreEvent::DownloadProgress(0.75));
    pump(&mut c, &q);
    assert_eq!(c.state().download_progress, 0.75);

    let entries = vec![ModelEntryView {
        id: "base".into(),
        display_name: "Base".into(),
        is_downloaded: true,
        ..ModelEntryView::default()
    }];
    n.send(CoreEvent::ModelSelected {
        model_id: "base".into(),
        display_name: "Base".into(),
        entries: entries.clone(),
        status: "模型下載完成，已選用".into(),
    });
    pump(&mut c, &q);
    let s = c.state();
    assert_eq!(s.downloading_id, "");
    assert_eq!(s.download_progress, 0.0);
    assert_eq!(s.selected_model_id, "base");
    assert_eq!(s.model_name, "Base");
    assert_eq!(s.model_entries, entries);
    assert_eq!(s.status_text, "模型下載完成，已選用");
}

#[test]
fn download_failure_resets_progress() {
    let (mut c, _fake) = controller();
    c.apply(CoreEvent::DownloadStarted {
        model_id: "large".into(),
        size_mb: 3000,
    });
    c.apply(CoreEvent::DownloadProgress(0.5));
    c.apply(CoreEvent::DownloadFailed("下載失敗: 逾時".into()));
    assert_eq!(c.state().downloading_id, "");
    assert_eq!(c.state().download_progress, 0.0);
    assert_eq!(c.state().status_text, "下載失敗: 逾時");
}

#[test]
fn start_preloads_or_asks_to_download() {
    let (mut c, fake) = controller();
    c.start();
    let calls = fake.calls();
    assert!(matches!(&calls[0], Call::Preload(id) if id == "small"));
    assert!(matches!(
        &calls[1],
        Call::SelectOrDownload { confirm: true, .. }
    ));

    let (mut c, fake) = controller();
    fake.0.lock().unwrap().preload_ok = true;
    c.start();
    assert_eq!(fake.calls().len(), 1);
}

// ─── 設定 ────────────────────────────────────────────────────────────────────

#[test]
fn settings_round_trip_through_save() {
    let (mut c, fake) = controller();
    {
        let s = c.state_mut();
        s.hotkey = "Ctrl+Alt+Q".into();
        s.ptt_mode = false;
        s.temperature = 0.4;
        s.inject_enabled = false;
    }
    c.handle(UiCommand::SaveSettings);
    assert_eq!(c.state().save_status, "✓ 已儲存");
    let saved = fake
        .calls()
        .into_iter()
        .find_map(|call| match call {
            Call::SaveSettings(s) => Some(s),
            _ => None,
        })
        .expect("saved");
    assert_eq!(saved, c.state().ui_settings());

    // UiSettings → AppConfig → UiSettings 不失真
    let mut cfg = AppConfig::default();
    saved.apply_to(&mut cfg);
    assert_eq!(UiSettings::from_config(&cfg), saved);

    // 還原到新的狀態
    let (mut c2, _f2) = controller();
    c2.handle(UiCommand::UpdateSettings(Box::new(saved.clone())));
    assert_eq!(c2.state().ui_settings(), saved);

    c.apply(CoreEvent::SaveStatusExpired);
    assert_eq!(c.state().save_status, "");
}

#[test]
fn save_failure_shows_error_status() {
    let (mut c, fake) = controller();
    fake.0.lock().unwrap().save_error = Some("磁碟已滿".into());
    c.handle(UiCommand::SaveSettings);
    assert_eq!(c.state().save_status, "✗ 儲存失敗");
    // 失敗訊息不會被「已儲存」到期事件清掉
    c.apply(CoreEvent::SaveStatusExpired);
    assert_eq!(c.state().save_status, "✗ 儲存失敗");
}

#[test]
fn settings_changed_reapplies_hotkeys() {
    let (mut c, fake) = controller();
    c.state_mut().ptt_hotkey = "Ctrl+Alt+K".into();
    c.handle(UiCommand::SettingsChanged);
    assert!(fake.calls().iter().any(|call| matches!(
        call,
        Call::ApplyHotkeys(hk, ptt, true) if hk == "Ctrl+Shift+W" && ptt == "Ctrl+Alt+K"
    )));
}

#[test]
fn privacy_toggle_flows_to_recording_and_config() {
    let (mut c, fake) = controller();
    assert!(!c.state().privacy_mode);
    c.handle(UiCommand::SetPrivacyMode(true));
    assert!(c.state().privacy_mode);

    c.handle(UiCommand::ToggleRecording);
    let Call::StartRecording(req) = &fake.calls()[0] else {
        panic!("expected StartRecording");
    };
    assert!(req.privacy_mode);

    c.handle(UiCommand::SaveSettings);
    let mut cfg = AppConfig::default();
    c.state().ui_settings().apply_to(&mut cfg);
    assert!(cfg.output.privacy_mode);

    c.handle(UiCommand::SetPrivacyMode(false));
    assert!(!c.state().ui_settings().privacy_mode);
}

// ─── 視窗 / 檔案 / 模板 / 錯誤紀錄 ───────────────────────────────────────────

#[test]
fn window_commands_return_view_requests() {
    let (mut c, _fake) = controller();
    assert_eq!(
        c.handle(UiCommand::SetFloatingMode(true)),
        vec![ViewRequest::ApplyWindowGeometry]
    );
    assert!(c.state().floating_mode);
    assert_eq!(
        c.handle(UiCommand::SetFloatingExpanded(true)),
        vec![ViewRequest::ApplyWindowGeometry]
    );
    assert!(c.state().floating_expanded);
    assert_eq!(
        c.handle(UiCommand::RequestClose),
        vec![ViewRequest::HideWindow]
    );
    assert_eq!(c.handle(UiCommand::StartDrag), vec![ViewRequest::StartDrag]);
    assert!(c.handle(UiCommand::SetSettingsOpen(true)).is_empty());
    assert!(c.state().settings_open);
}

#[test]
fn file_mode_flow() {
    let (mut c, fake) = controller();
    c.handle(UiCommand::StartFileTranscription); // 尚未選檔：不動作
    assert!(fake.calls().is_empty());

    c.handle(UiCommand::OpenFileDialog);
    c.apply(CoreEvent::FilePicked("C:/a/meeting.mp3".into()));
    assert_eq!(c.state().file_name, "C:/a/meeting.mp3");
    assert!(!c.state().file_has_result);

    c.handle(UiCommand::SetFileOutputFormat("srt".into()));
    c.handle(UiCommand::StartFileTranscription);
    let req = fake
        .calls()
        .into_iter()
        .find_map(|call| match call {
            Call::FileTranscription(r) => Some(r),
            _ => None,
        })
        .expect("file request");
    assert_eq!(req.file_path, "C:/a/meeting.mp3");
    assert_eq!(req.model_id, "small");
    assert_eq!(req.output_format, "srt");

    c.apply(CoreEvent::FileStarted);
    assert!(c.state().file_processing);
    c.apply(CoreEvent::FileProgress(0.5));
    assert_eq!(c.state().file_progress, 0.5);
    c.apply(CoreEvent::FileFinished("逐字稿".into()));
    assert!(!c.state().file_processing);
    assert!(c.state().file_has_result);
    assert_eq!(c.state().file_progress, 1.0);

    c.handle(UiCommand::FileCopyResult);
    c.handle(UiCommand::FileExportResult("txt".into()));
    let calls = fake.calls();
    assert!(calls
        .iter()
        .any(|call| matches!(call, Call::Copy(t) if t == "逐字稿")));
    assert!(calls.iter().any(|call| matches!(
        call,
        Call::Export(t, f, fmt) if t == "逐字稿" && f == "C:/a/meeting.mp3" && fmt == "txt"
    )));
    assert!(calls.iter().any(|call| matches!(call, Call::PickFile)));

    c.apply(CoreEvent::FileFailed("解碼失敗: x".into()));
    assert_eq!(c.state().file_result, "解碼失敗: x");
}

#[test]
fn template_commands_and_updates() {
    let (mut c, fake) = controller();
    c.handle(UiCommand::TemplateSelected("meeting".into()));
    assert_eq!(c.state().current_template, "meeting");
    c.handle(UiCommand::TemplateSave {
        original: "".into(),
        name: "新".into(),
        prompt: "p".into(),
    });
    c.handle(UiCommand::TemplateDelete("新".into()));
    let calls = fake.calls();
    assert!(
        matches!(&calls[0], Call::SaveTemplate(o, n, p) if o.is_empty() && n == "新" && p == "p")
    );
    assert!(matches!(&calls[1], Call::DeleteTemplate(n) if n == "新"));

    let entries = vec![TemplateEntryView {
        name: "新".into(),
        prompt: "p".into(),
        is_builtin: false,
    }];
    c.apply(CoreEvent::TemplatesUpdated {
        entries: entries.clone(),
        status: Some("模板已儲存".into()),
    });
    assert_eq!(c.state().template_entries, entries);
    assert_eq!(c.state().status_text, "模板已儲存");
}

#[test]
fn error_log_snapshot_and_clear() {
    let (mut c, fake) = controller();
    fake.0.lock().unwrap().error_log = vec!["錯誤 2".into(), "錯誤 1".into()];
    c.apply(CoreEvent::ErrorLogChanged);
    assert_eq!(c.state().error_log.len(), 2);
    c.handle(UiCommand::SetShowErrorLog(true));
    c.handle(UiCommand::ClearErrorLog);
    assert!(c.state().error_log.is_empty());
    assert!(!c.state().show_error_log);
    assert!(fake
        .calls()
        .iter()
        .any(|call| matches!(call, Call::ClearErrorLog)));
}

#[test]
fn misc_commands_reach_backend() {
    let (mut c, fake) = controller();
    c.handle(UiCommand::OpenModelManager);
    c.handle(UiCommand::SetDarkMode(true));
    c.handle(UiCommand::SetFileMode(true));
    c.handle(UiCommand::SetClipboardEnabled(false));
    c.handle(UiCommand::SetInjectEnabled(false));
    let s = c.state();
    assert!(s.dark_mode && s.file_mode && !s.clipboard_enabled && !s.inject_enabled);
    assert!(matches!(fake.calls()[0], Call::OpenModelFolder));
    c.apply(CoreEvent::ModelEntries {
        entries: Vec::new(),
        model_name: Some("Tiny".into()),
    });
    assert_eq!(c.state().model_name, "Tiny");
    c.apply(CoreEvent::Partial("暫時".into()));
    assert_eq!(c.state().current_partial, "暫時");
    c.apply(CoreEvent::Silence);
    assert_eq!(c.state().current_partial, "");
}

#[test]
fn keys_feed_controller() {
    let (mut c, _fake) = controller();
    let cmd = keys::key_to_command(c.state(), &keys::KeyPress::plain(keys::Key::Space), false)
        .expect("space");
    c.handle(cmd);
    assert!(c.state().is_recording);
}
