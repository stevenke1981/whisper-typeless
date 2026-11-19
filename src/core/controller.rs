//! [`Controller`]：把 [`UiCommand`] 與 [`CoreEvent`] 套用到 [`AppViewState`]；
//! [`Notifier`] / [`EventQueue`]：背景執行緒 → 畫面執行緒的事件通道。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use tracing::{error, info};

use super::backend::{Backend, FileTranscriptionRequest, RecordingRequest, RecordingStart};
use super::command::{UiCommand, ViewRequest};
use super::event::{CoreEvent, TrayAction};
use super::state::{AppViewState, TranscriptSegment};
use crate::audio::WAVEFORM_BAR_COUNT;
use crate::settings::config::{AppConfig, TranscriptionEngine};
use crate::settings::secret::Secret;
use crate::whisper::WhisperParams;

// ─── Notifier ────────────────────────────────────────────────────────────────

struct NotifierShared {
    tx: Mutex<mpsc::Sender<CoreEvent>>,
    wake: Box<dyn Fn() + Send + Sync>,
    /// 已呼叫 wake 但畫面尚未 drain。
    wake_pending: AtomicBool,
    /// 佇列中已有一筆 Waveform 佔位。
    waveform_pending: AtomicBool,
    /// 最新的波形值（drain 時填入佔位）。
    waveform_latest: Mutex<Option<Vec<f32>>>,
}

/// 背景工作送事件給畫面用的句柄（`Send + Sync + Clone`）。
///
/// 每次 `send` 會把事件放入佇列；若畫面尚未被喚醒就呼叫一次 wake callback
/// （Slint 版為 Slint 的 `invoke_from_event_loop`）。`CoreEvent::Waveform` 會合併：
/// 佇列中最多一筆，drain 時取最新值。
#[derive(Clone)]
pub struct Notifier {
    shared: Arc<NotifierShared>,
}

/// 畫面執行緒持有的接收端。
pub struct EventQueue {
    rx: mpsc::Receiver<CoreEvent>,
    shared: Arc<NotifierShared>,
}

/// 建立事件通道；`wake` 會在任意執行緒被呼叫，負責讓畫面執行緒稍後呼叫 [`EventQueue::drain`]。
pub fn event_channel(wake: impl Fn() + Send + Sync + 'static) -> (Notifier, EventQueue) {
    let (tx, rx) = mpsc::channel();
    let shared = Arc::new(NotifierShared {
        tx: Mutex::new(tx),
        wake: Box::new(wake),
        wake_pending: AtomicBool::new(false),
        waveform_pending: AtomicBool::new(false),
        waveform_latest: Mutex::new(None),
    });
    (
        Notifier {
            shared: Arc::clone(&shared),
        },
        EventQueue { rx, shared },
    )
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Notifier {
    /// 送出事件（可在任意執行緒呼叫，不會阻塞）。
    pub fn send(&self, event: CoreEvent) {
        let s = &self.shared;
        let event = match event {
            CoreEvent::Waveform(bars) => {
                *lock(&s.waveform_latest) = Some(bars);
                if s.waveform_pending.swap(true, Ordering::AcqRel) {
                    return; // 佇列中已有佔位，drain 時會取到最新值
                }
                CoreEvent::Waveform(Vec::new())
            }
            other => other,
        };
        if lock(&s.tx).send(event).is_err() {
            return; // 畫面已關閉
        }
        if !s.wake_pending.swap(true, Ordering::AcqRel) {
            (s.wake)();
        }
    }
}

impl EventQueue {
    /// 取出目前所有事件（依送出順序）。在畫面執行緒呼叫。
    pub fn drain(&self) -> Vec<CoreEvent> {
        let s = &self.shared;
        // 先清旗標再取：之後到達的事件一定會再觸發一次 wake
        s.wake_pending.store(false, Ordering::Release);
        let mut out = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            match event {
                CoreEvent::Waveform(_) => {
                    s.waveform_pending.store(false, Ordering::Release);
                    if let Some(bars) = lock(&s.waveform_latest).take() {
                        out.push(CoreEvent::Waveform(bars));
                    }
                }
                other => out.push(other),
            }
        }
        out
    }
}

// ─── Controller ──────────────────────────────────────────────────────────────

const SAVED_STATUS: &str = "✓ 已儲存";

/// UI 無關的應用程式控制器。只在畫面執行緒使用。
pub struct Controller {
    state: AppViewState,
    config: AppConfig,
    backend: Box<dyn Backend>,
}

impl Controller {
    /// `config` 為啟動時的設定快照（whisper 進階參數、檔案模式參數取自此處）。
    pub fn new(state: AppViewState, config: AppConfig, backend: Box<dyn Backend>) -> Self {
        Self {
            state,
            config,
            backend,
        }
    }

    pub fn state(&self) -> &AppViewState {
        &self.state
    }

    /// 給畫面層同步「使用者直接在畫面上改的欄位」（例如 Slint 雙向綁定）。
    pub fn state_mut(&mut self) -> &mut AppViewState {
        &mut self.state
    }

    /// 啟動時呼叫一次：預載選用模型；模型不存在時詢問是否下載。
    pub fn start(&mut self) {
        let model_id = self.state.selected_model_id.clone();
        let params = self.whisper_params();
        if !self.backend.preload_model(model_id.clone(), params.clone()) {
            self.backend
                .select_or_download_model(model_id, params, true);
        }
    }

    /// 目前畫面設定套到啟動設定上的 whisper 參數。
    pub fn whisper_params(&self) -> WhisperParams {
        let mut params = self.config.whisper.clone();
        params.use_gpu = self.state.use_gpu;
        params.n_threads = self.state.n_threads;
        params.temperature = self.state.temperature;
        params
    }

    /// 處理使用者操作；回傳需要畫面層執行的動作。
    pub fn handle(&mut self, cmd: UiCommand) -> Vec<ViewRequest> {
        let s = &mut self.state;
        match cmd {
            UiCommand::SetFloatingMode(on) => {
                s.floating_mode = on;
                info!(floating = on, "視窗模式切換");
                return vec![ViewRequest::ApplyWindowGeometry];
            }
            UiCommand::SetFloatingExpanded(on) => {
                s.floating_expanded = on;
                return vec![ViewRequest::ApplyWindowGeometry];
            }
            // 關閉鈕：縮到系統匣（畫面層沒有系統匣時改為關閉視窗），或直接結束。
            UiCommand::RequestClose => {
                if s.close_to_tray {
                    s.window_hidden = true;
                    return vec![ViewRequest::HideWindow];
                }
                s.quit_requested = true;
                return vec![ViewRequest::Quit];
            }
            UiCommand::StartDrag => return vec![ViewRequest::StartDrag],
            UiCommand::ClearErrorLog => {
                self.backend.clear_error_log();
                self.state.error_log.clear();
                self.state.show_error_log = false;
            }
            UiCommand::ToggleRecording => self.toggle_recording(),
            UiCommand::ClearTranscript => {
                s.transcript_segments.clear();
                s.char_count = 0;
            }
            UiCommand::TemplateSelected(id) => s.current_template = id,
            UiCommand::SettingsChanged => {
                let (hk, ptt, on) = (s.hotkey.clone(), s.ptt_hotkey.clone(), s.ptt_mode);
                self.backend.apply_hotkeys(&hk, &ptt, on);
            }
            UiCommand::SaveSettings => self.save_settings(),
            UiCommand::OpenModelManager => self.backend.open_model_folder(),
            UiCommand::ModelSelectOrDownload(id) => {
                let params = self.whisper_params();
                self.backend.select_or_download_model(id, params, false);
            }
            UiCommand::OpenFileDialog => self.backend.pick_audio_file(),
            UiCommand::StartFileTranscription => self.start_file_transcription(),
            UiCommand::FileCopyResult => {
                let text = s.file_result.clone();
                self.backend.copy_text(text);
            }
            UiCommand::FileExportResult(fmt) => {
                let (text, file) = (s.file_result.clone(), s.file_name.clone());
                self.backend.export_text(text, file, fmt);
            }
            UiCommand::TemplateSave {
                original,
                name,
                prompt,
            } => self.backend.save_template(original, name, prompt),
            UiCommand::TemplateDelete(name) => self.backend.delete_template(name),

            UiCommand::SetSettingsOpen(on) => s.settings_open = on,
            UiCommand::SetShowErrorLog(on) => s.show_error_log = on,
            UiCommand::SetDarkMode(on) => s.dark_mode = on,
            UiCommand::SetFileMode(on) => s.file_mode = on,
            UiCommand::SetPrivacyMode(on) => s.privacy_mode = on,
            UiCommand::SetClipboardEnabled(on) => s.clipboard_enabled = on,
            UiCommand::SetInjectEnabled(on) => s.inject_enabled = on,
            UiCommand::SetFileOutputFormat(fmt) => s.file_output_format = fmt,
            UiCommand::UpdateSettings(settings) => s.apply_ui_settings(&settings),

            // WP-A：金鑰加密存入設定資料夾（DPAPI）；結果反映在 openai_key_set / 狀態列。
            UiCommand::SetOpenAiKey(key) => self.store_api_key(Some(key)),
            UiCommand::ClearOpenAiKey => self.store_api_key(None),

            // WP-B：系統匣叫回 / 結束。
            UiCommand::ShowWindow => {
                s.window_hidden = false;
                return vec![ViewRequest::ShowWindow];
            }
            UiCommand::Quit => {
                s.quit_requested = true;
                return vec![ViewRequest::Quit];
            }
        }
        Vec::new()
    }

    /// 套用背景事件。
    pub fn apply(&mut self, event: CoreEvent) {
        let s = &mut self.state;
        match event {
            CoreEvent::Status(text) => s.status_text = text,
            CoreEvent::StatusIfIdle(text) => {
                if !s.is_recording {
                    s.status_text = text;
                }
            }
            CoreEvent::DeviceLabel(label) => s.device_label = label,
            CoreEvent::ErrorLogChanged => s.error_log = self.backend.error_log_snapshot(),
            CoreEvent::SaveStatusExpired => {
                if s.save_status == SAVED_STATUS {
                    s.save_status.clear();
                }
            }

            CoreEvent::HotkeyPressed => self.toggle_recording(),
            CoreEvent::PttKey(is_down) => {
                if is_down != s.is_recording {
                    self.toggle_recording();
                }
            }

            CoreEvent::Level(level) => s.audio_level = level,
            CoreEvent::Waveform(bars) => s.waveform_bars = bars,
            CoreEvent::SpeechStarted => {
                s.current_partial = "聆聽中...".into();
                s.is_processing = true;
            }
            CoreEvent::Silence => s.current_partial.clear(),
            CoreEvent::Partial(text) => s.current_partial = text,
            CoreEvent::PipelineFinished => {
                s.waveform_bars = vec![0.0; WAVEFORM_BAR_COUNT];
                s.is_processing = false;
                s.audio_level = 0.0;
            }
            CoreEvent::ModelLoading => {
                s.status_text = "模型尚未預載完成，正在載入...".into();
                s.is_processing = true;
            }
            CoreEvent::ModelLoadFailed(msg) => {
                s.status_text = msg;
                s.is_processing = false;
            }
            CoreEvent::TranscribeStarted => {
                s.current_partial = "⏳ 轉錄中...".into();
                s.is_processing = true;
            }
            CoreEvent::TranscriptEmpty => {
                s.current_partial.clear();
                s.is_processing = false;
            }
            CoreEvent::TranscriptSegment {
                text,
                timestamp,
                status,
            } => {
                s.transcript_segments
                    .push(TranscriptSegment { text, timestamp });
                s.char_count = s.transcript_char_count();
                s.current_partial.clear();
                s.is_processing = false;
                s.status_text = status;
            }
            CoreEvent::TranscriptionFailed(msg) => {
                s.status_text = msg;
                s.current_partial.clear();
                s.is_processing = false;
            }

            CoreEvent::ModelEntries {
                entries,
                model_name,
            } => {
                s.model_entries = entries;
                if let Some(name) = model_name {
                    s.model_name = name;
                }
            }
            CoreEvent::DownloadStarted { model_id, size_mb } => {
                s.downloading_id = model_id;
                s.download_progress = 0.0;
                s.status_text = format!("下載模型 ({size_mb} MB)...");
            }
            CoreEvent::DownloadProgress(ratio) => s.download_progress = ratio,
            CoreEvent::DownloadFailed(msg) => {
                s.downloading_id.clear();
                s.download_progress = 0.0;
                s.status_text = msg;
            }
            CoreEvent::ModelSelected {
                model_id,
                display_name,
                entries,
                status,
            } => {
                s.downloading_id.clear();
                s.download_progress = 0.0;
                s.selected_model_id = model_id;
                s.model_name = display_name;
                s.status_text = status;
                s.model_entries = entries;
            }

            CoreEvent::FilePicked(path) => {
                s.file_name = path;
                s.file_has_result = false;
                s.file_result.clear();
                s.file_progress = 0.0;
            }
            CoreEvent::FileStarted => {
                s.file_processing = true;
                s.file_progress = 0.05;
            }
            CoreEvent::FileProgress(p) => s.file_progress = p,
            CoreEvent::FileFinished(text) => {
                s.file_progress = 1.0;
                s.file_processing = false;
                s.file_result = text;
                s.file_has_result = true;
            }
            CoreEvent::FileFailed(msg) => {
                s.file_processing = false;
                s.file_result = msg;
                s.file_has_result = true;
            }

            CoreEvent::TemplatesUpdated { entries, status } => {
                s.template_entries = entries;
                if let Some(status) = status {
                    s.status_text = status;
                }
            }

            // WP-B：先只改狀態旗標，畫面層依旗標顯示 / 隱藏 / 結束。
            CoreEvent::Tray(action) => match action {
                TrayAction::ToggleWindow => s.window_hidden = !s.window_hidden,
                TrayAction::ToggleRecording => self.toggle_recording(),
                TrayAction::OpenSettings => {
                    s.window_hidden = false;
                    s.settings_open = true;
                }
                TrayAction::Quit => s.quit_requested = true,
            },

            // WP-A
            CoreEvent::OpenAiKeyState(set) => s.openai_key_set = set,
        }
    }

    /// 儲存 / 刪除 OpenAI 金鑰並更新 `openai_key_set`（WP-A）。
    fn store_api_key(&mut self, key: Option<Secret>) {
        match self.backend.store_api_key(key) {
            Ok(()) => self.state.openai_key_set = self.backend.has_api_key(),
            Err(e) => {
                error!("OpenAI 金鑰儲存失敗: {e}");
                self.state.status_text = format!("金鑰儲存失敗：{e}");
            }
        }
    }

    fn toggle_recording(&mut self) {
        if self.backend.is_recording() {
            // ══ 停止錄音 ══
            self.backend.stop_recording();
            let s = &mut self.state;
            s.is_recording = false;
            s.status_text = "就緒".into();
            s.audio_level = 0.0;
            s.is_processing = false;
            s.current_partial.clear();
            s.waveform_bars = vec![0.0; WAVEFORM_BAR_COUNT];
            return;
        }

        // ══ 開始錄音 ══
        let req = RecordingRequest {
            privacy_mode: self.state.privacy_mode,
            vad_threshold: self.state.vad_threshold,
            vad_timeout_ms: self.state.silence_timeout.max(0.0).round() as u64,
            clipboard_enabled: self.state.clipboard_enabled,
            inject_enabled: self.state.inject_enabled,
            opencc_enabled: self.state.mod_opencc,
            conversion_mode: self.state.conversion_mode.clone(),
            whisper_params: self.whisper_params(),
            // 隱私：未經同意或開啟隱私模式時一律本機轉錄
            engine: if self.state.openai_consent && !self.state.privacy_mode {
                TranscriptionEngine::from_str_lossy(&self.state.engine)
            } else {
                TranscriptionEngine::Local
            },
            openai_model: self.state.openai_model.clone(),
        };
        match self.backend.start_recording(req) {
            RecordingStart::Started => {
                self.state.is_recording = true;
                self.state.status_text = "● 錄音中...".into();
            }
            RecordingStart::ModelMissing { model_id } => {
                self.state.status_text = "模型尚未下載，請確認是否自動下載".into();
                let params = self.whisper_params();
                self.backend
                    .select_or_download_model(model_id, params, true);
            }
            RecordingStart::Failed(msg) => self.state.status_text = msg,
        }
    }

    fn save_settings(&mut self) {
        let snapshot = self.state.ui_settings();
        match self.backend.save_settings(&snapshot) {
            Ok(()) => self.state.save_status = SAVED_STATUS.into(),
            Err(e) => {
                error!("設定儲存失敗: {e}");
                self.state.save_status = "✗ 儲存失敗".into();
            }
        }
    }

    fn start_file_transcription(&mut self) {
        let s = &self.state;
        if s.file_name.is_empty() {
            return;
        }
        let req = FileTranscriptionRequest {
            file_path: s.file_name.clone(),
            model_id: s.selected_model_id.clone(),
            output_format: s.file_output_format.clone(),
            opencc_enabled: s.mod_opencc,
            conversion_mode: s.conversion_mode.clone(),
            // 檔案模式沿用啟動時的 whisper 設定（與舊版行為一致）
            whisper_params: self.config.whisper.clone(),
        };
        self.backend.start_file_transcription(req);
    }
}
