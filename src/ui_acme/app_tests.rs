//! AcmeApp 的合成事件測試（不開視窗、不碰音訊 / GPU / 網路 / 模型檔）。

use super::*;
use crate::core::backend::{Backend, FileTranscriptionRequest, RecordingRequest, RecordingStart};
use crate::core::{
    event_channel, CoreEvent, ModelEntryView, Notifier, TemplateEntryView, TranscriptSegment,
};
use crate::settings::config::AppConfig;
use crate::settings::ui_settings::UiSettings;
use crate::ui_acme::paint::Action;
use crate::whisper::WhisperParams;
use acme_render_cpu::CpuRenderer;

#[derive(Default)]
pub(super) struct FakeBackend {
    recording: bool,
}

impl Backend for FakeBackend {
    fn is_recording(&self) -> bool {
        self.recording
    }
    fn start_recording(&mut self, _req: RecordingRequest) -> RecordingStart {
        self.recording = true;
        RecordingStart::Started
    }
    fn stop_recording(&mut self) {
        self.recording = false;
    }
    fn select_or_download_model(&mut self, _id: String, _p: WhisperParams, _confirm: bool) {}
    fn preload_model(&mut self, _id: String, _p: WhisperParams) -> bool {
        true
    }
    fn save_settings(&mut self, _settings: &UiSettings) -> Result<(), String> {
        Ok(())
    }
    fn apply_hotkeys(&mut self, _hotkey: &str, _ptt: &str, _on: bool) {}
    fn open_model_folder(&mut self) {}
    fn pick_audio_file(&mut self) {}
    fn start_file_transcription(&mut self, _req: FileTranscriptionRequest) {}
    fn copy_text(&mut self, _text: String) {}
    fn export_text(&mut self, _text: String, _source: String, _format: String) {}
    fn save_template(&mut self, _original: String, _name: String, _prompt: String) {}
    fn delete_template(&mut self, _name: String) {}
    fn clear_error_log(&mut self) {}
    fn store_api_key(&mut self, _key: Option<crate::settings::Secret>) -> Result<(), String> {
        Ok(())
    }
    fn has_api_key(&self) -> bool {
        false
    }
    fn error_log_snapshot(&self) -> Vec<String> {
        vec!["模型下載失敗：連線逾時".into()]
    }
}

pub(super) fn app_with(state: AppViewState) -> (AcmeApp, Notifier) {
    let (notifier, queue) = event_channel(|| {});
    let controller = Controller::new(state, AppConfig::default(), Box::<FakeBackend>::default());
    let slot: WakerSlot = Arc::new(Mutex::new(None));
    (AcmeApp::new(controller, queue, slot), notifier)
}

fn app() -> AcmeApp {
    app_with(AppViewState::default()).0
}

pub(super) fn models() -> Vec<ModelEntryView> {
    vec![
        ModelEntryView {
            id: "small".into(),
            display_name: "Small (466 MB)".into(),
            is_downloaded: true,
            size_mb: 466,
            quality: 3,
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin".into(),
        },
        ModelEntryView {
            id: "base".into(),
            display_name: "Base".into(),
            is_downloaded: true,
            size_mb: 142,
            quality: 2,
            download_url: "https://example.invalid/base.bin".into(),
        },
        ModelEntryView {
            id: "large-v3".into(),
            display_name: "Large v3".into(),
            is_downloaded: false,
            size_mb: 3100,
            quality: 5,
            download_url: "https://example.invalid/large.bin".into(),
        },
    ]
}

pub(super) fn templates() -> Vec<TemplateEntryView> {
    vec![
        TemplateEntryView {
            name: "會議".into(),
            prompt: "以下是會議紀錄。".into(),
            is_builtin: true,
        },
        TemplateEntryView {
            name: "我的模板".into(),
            prompt: "自訂提示".into(),
            is_builtin: false,
        },
    ]
}

fn rich_state() -> AppViewState {
    AppViewState {
        model_entries: models(),
        template_entries: templates(),
        transcript_segments: (0..12)
            .map(|i| TranscriptSegment {
                text: format!("第 {i} 段：今天的會議討論了繁體中文語音辨識的自動換行與捲動行為。"),
                timestamp: format!("10:{i:02}"),
            })
            .collect(),
        current_partial: "正在聆聽…".into(),
        waveform_bars: (0..20).map(|i| i as f32 / 20.0).collect(),
        error_log: vec!["錯誤一".into(), "很長的錯誤訊息".repeat(10)],
        file_name: "C:\\錄音\\訪談.mp3".into(),
        file_has_result: true,
        file_result: "檔案轉錄結果。".repeat(40),
        downloading_id: "large-v3".into(),
        download_progress: 0.42,
        ..AppViewState::default()
    }
}

/// 先畫一次（建立命中區域），再在找到的區域中心做一次完整點擊。
fn click(app: &mut AcmeApp, w: f32, h: f32, pred: impl Fn(&Action) -> bool) {
    app.build_scene(w, h, 1.0).expect("scene");
    let mut p = app.last_tree.find_hit(&pred).expect("hit region not found");
    // 在捲動區下方看不到的：先捲到可見
    if p.y.get() > h - 4.0 {
        let need = p.y.get() - h / 2.0;
        app.ui.set_scroll(ScrollId::Settings, need);
        app.build_scene(w, h, 1.0).expect("scene");
        p = app
            .last_tree
            .find_hit(&pred)
            .expect("hit region after scroll");
    }
    let (x, y) = (p.x.get(), p.y.get());
    assert!(y >= 0.0 && y <= h, "hit center {y} outside window {h}");
    app.event(PlatformEvent::PointerMoved {
        window: WindowId(0),
        x,
        y,
    });
    app.event(button(true, x, y));
    app.event(button(false, x, y));
}

fn button(pressed: bool, x: f32, y: f32) -> PlatformEvent {
    PlatformEvent::PointerButton {
        window: WindowId(0),
        pressed,
        x,
        y,
        button: 0,
        pointer: 1,
        shift: false,
        ctrl: false,
        alt: false,
    }
}

fn is_click(msg: Msg) -> impl Fn(&Action) -> bool {
    move |a| *a == Action::Click(msg.clone())
}

fn key(key: PlatformKey, text: Option<&str>) -> PlatformEvent {
    PlatformEvent::Key {
        window: WindowId(0),
        key,
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
        repeat: false,
        text: text.map(str::to_string),
        character: text.map(str::to_string),
    }
}

// ─── 每個畫面狀態的 Scene 都要通過 validate（1.0 / 1.5 倍） ────────────────────

#[test]
fn every_view_state_validates_at_both_scales() {
    let base = rich_state();
    let variants: Vec<(&str, AppViewState, (f32, f32))> = vec![
        ("full idle", AppViewState::default(), (560.0, 680.0)),
        ("full rich", base.clone(), (560.0, 680.0)),
        (
            "full recording",
            AppViewState {
                is_recording: true,
                is_processing: true,
                ..base.clone()
            },
            (560.0, 680.0),
        ),
        (
            "full file",
            AppViewState {
                file_mode: true,
                ..base.clone()
            },
            (560.0, 680.0),
        ),
        (
            "full file processing",
            AppViewState {
                file_mode: true,
                file_processing: true,
                file_has_result: false,
                file_progress: 0.3,
                ..base.clone()
            },
            (560.0, 680.0),
        ),
        (
            "full settings",
            AppViewState {
                settings_open: true,
                save_status: "✓ 已儲存".into(),
                ..base.clone()
            },
            (560.0, 680.0),
        ),
        (
            "full error log",
            AppViewState {
                show_error_log: true,
                ..base.clone()
            },
            (560.0, 680.0),
        ),
        (
            "full dark",
            AppViewState {
                dark_mode: true,
                mod_context: false,
                mod_waveform: false,
                ..base.clone()
            },
            (560.0, 680.0),
        ),
        (
            "floating collapsed",
            AppViewState {
                floating_mode: true,
                ..base.clone()
            },
            (380.0, 52.0),
        ),
        (
            "floating collapsed recording",
            AppViewState {
                floating_mode: true,
                is_recording: true,
                current_partial: String::new(),
                ..AppViewState::default()
            },
            (380.0, 52.0),
        ),
        (
            "floating expanded",
            AppViewState {
                floating_mode: true,
                floating_expanded: true,
                ..base.clone()
            },
            (380.0, 540.0),
        ),
        (
            "floating expanded file",
            AppViewState {
                floating_mode: true,
                floating_expanded: true,
                file_mode: true,
                ..base.clone()
            },
            (380.0, 540.0),
        ),
        (
            "floating settings dark",
            AppViewState {
                floating_mode: true,
                floating_expanded: true,
                settings_open: true,
                dark_mode: true,
                ..base.clone()
            },
            (380.0, 540.0),
        ),
        (
            "floating error log collapsed",
            AppViewState {
                floating_mode: true,
                show_error_log: true,
                ..base.clone()
            },
            (380.0, 52.0),
        ),
    ];
    for (name, state, (w, h)) in variants {
        for scale in [1.0, 1.5] {
            let (mut app, _n) = app_with(state.clone());
            if state.settings_open {
                app.ui.edit_template(1, &templates()[1]);
                app.ui.focus = Some(FieldId::TemplatePrompt);
                app.ui.fields.template_prompt.focused = true;
                ime_preedit(&mut app.ui.fields.template_prompt, "ㄏㄠˇ", None);
            }
            let scene = app
                .build_scene(w, h, scale)
                .unwrap_or_else(|e| panic!("{name} @{scale}: {e:?}"));
            assert!(!scene.commands().is_empty(), "{name} empty");
            scene
                .validate()
                .unwrap_or_else(|e| panic!("{name} @{scale} invalid: {e:?}"));
            assert!(!app.hitmap.is_empty(), "{name} has no hit regions");
        }
    }
}

#[test]
fn cpu_render_produces_non_background_pixels() {
    let mut app = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    })
    .0;
    let scene = app.build_scene(380.0, 52.0, 1.0).unwrap();
    let mut cpu = CpuRenderer::new(380, 52, 1.0).expect("cpu renderer");
    cpu.render_scene(&scene).expect("render");
    let pixels = cpu.pixels();
    assert_eq!(pixels.len(), 380 * 52);
    let bg = pixels[0];
    let differing = pixels.iter().filter(|p| **p != bg).count();
    assert!(differing > 500, "only {differing} non-background pixels");
}

// ─── 主要按鈕 → UiCommand ─────────────────────────────────────────────────────

#[test]
fn record_button_ptt_press_and_release_toggle_recording() {
    let mut app = app();
    assert!(app.state().ptt_mode);
    click(&mut app, 560.0, 680.0, |a| matches!(a, Action::Hold { .. }));
    // 按下開始、放開停止
    assert_eq!(
        app.sent,
        vec![UiCommand::ToggleRecording, UiCommand::ToggleRecording]
    );
    assert!(!app.state().is_recording);
}

#[test]
fn record_button_click_toggles_when_not_ptt() {
    let mut app = app_with(AppViewState {
        ptt_mode: false,
        floating_mode: true,
        ..AppViewState::default()
    })
    .0;
    click(
        &mut app,
        380.0,
        52.0,
        is_click(Msg::Core(UiCommand::ToggleRecording)),
    );
    assert_eq!(app.sent, vec![UiCommand::ToggleRecording]);
    assert!(app.state().is_recording);
    assert!(app.background_poll_interval().is_some());
}

#[test]
fn expand_and_pin_in_compact_bar() {
    let mut app = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    })
    .0;
    app.geometry_pending = false;
    click(&mut app, 380.0, 52.0, is_click(Msg::ToggleExpanded));
    assert_eq!(app.sent, vec![UiCommand::SetFloatingExpanded(true)]);
    assert!(app.state().floating_expanded);
    assert!(app.geometry_pending, "expand must resize the window");

    app.geometry_pending = false;
    assert!(app.local().pinned);
    click(&mut app, 380.0, 540.0, is_click(Msg::TogglePin));
    assert!(!app.local().pinned);
    assert!(app.geometry_pending, "pin must re-apply topmost");
    assert_eq!(app.sent.len(), 1, "pin is local-only");
}

#[test]
fn settings_open_auto_expands_floating_bar() {
    let mut app = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    })
    .0;
    click(&mut app, 380.0, 52.0, is_click(Msg::ToggleSettings));
    assert_eq!(
        app.sent,
        vec![
            UiCommand::SetFloatingExpanded(true),
            UiCommand::SetSettingsOpen(true)
        ]
    );
    // 完整模式：只開設定
    let mut full = super::tests::app();
    click(&mut full, 560.0, 680.0, is_click(Msg::ToggleSettings));
    assert_eq!(full.sent, vec![UiCommand::SetSettingsOpen(true)]);
    assert!(full.state().settings_open);
}

#[test]
fn privacy_toggle_in_settings() {
    let mut app = app_with(AppViewState {
        settings_open: true,
        ..AppViewState::default()
    })
    .0;
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::SetPrivacyMode(true))),
    );
    assert_eq!(
        app.sent,
        vec![UiCommand::SetPrivacyMode(true), UiCommand::SaveSettings]
    );
    assert!(app.state().privacy_mode);
}

#[test]
fn template_select_from_context_bar() {
    let mut app = app();
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::TemplateSelected("meeting".into()))),
    );
    assert_eq!(
        app.sent,
        vec![UiCommand::TemplateSelected("meeting".into())]
    );
    assert_eq!(app.state().current_template, "meeting");
}

#[test]
fn model_select_in_settings_and_disabled_while_downloading() {
    let mut app = app_with(AppViewState {
        settings_open: true,
        model_entries: models(),
        ..AppViewState::default()
    })
    .0;
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::ModelSelectOrDownload("base".into()))),
    );
    assert_eq!(
        app.sent,
        vec![UiCommand::ModelSelectOrDownload("base".into())]
    );

    let mut busy = app_with(AppViewState {
        settings_open: true,
        model_entries: models(),
        downloading_id: "large-v3".into(),
        ..AppViewState::default()
    })
    .0;
    busy.build_scene(560.0, 680.0, 1.0).unwrap();
    assert!(busy
        .last_tree
        .find_hit(|a| matches!(
            a,
            Action::Click(Msg::Core(UiCommand::ModelSelectOrDownload(_)))
        ))
        .is_none());
}

#[test]
fn toolbar_buttons_map_to_commands() {
    let mut app = app();
    click(&mut app, 560.0, 680.0, is_click(Msg::ToggleFloating));
    assert_eq!(
        app.sent,
        vec![
            UiCommand::SetFloatingExpanded(false),
            UiCommand::SetFloatingMode(true)
        ]
    );
    // 浮動 → 完整
    app.sent.clear();
    click(&mut app, 380.0, 52.0, is_click(Msg::ToggleFloating));
    assert_eq!(app.sent, vec![UiCommand::SetFloatingMode(false)]);
    // 錯誤徽章、分頁、關閉
    app.sent.clear();
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::SetShowErrorLog(true))),
    );
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::ClearErrorLog)),
    );
    assert_eq!(
        app.sent,
        vec![UiCommand::SetShowErrorLog(true), UiCommand::ClearErrorLog]
    );
    app.sent.clear();
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::SetFileMode(true))),
    );
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::OpenFileDialog)),
    );
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Core(UiCommand::RequestClose)),
    );
    assert_eq!(
        app.sent,
        vec![
            UiCommand::SetFileMode(true),
            UiCommand::OpenFileDialog,
            UiCommand::RequestClose
        ]
    );
    assert!(app.pending_requests().contains(&WindowRequest::Close));
}

#[test]
fn dragging_the_bar_requests_drag_move() {
    let mut app = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    })
    .0;
    app.build_scene(380.0, 52.0, 1.0).unwrap();
    // 文字區（無按鈕）
    app.event(PlatformEvent::PointerMoved {
        window: WindowId(0),
        x: 180.0,
        y: 26.0,
    });
    app.event(button(true, 180.0, 26.0));
    assert!(app.pending_requests().contains(&WindowRequest::DragMove));
    assert!(app.sent.is_empty());
}

// ─── 滑桿 / 步進器 ───────────────────────────────────────────────────────────

#[test]
fn slider_press_and_drag_update_settings() {
    let mut app = app_with(AppViewState {
        settings_open: true,
        ..AppViewState::default()
    })
    .0;
    app.build_scene(560.0, 680.0, 1.0).unwrap();
    let spec = app
        .last_tree
        .items
        .iter()
        .find_map(|item| match item {
            crate::ui_acme::paint::Item::Hit {
                action: Action::Slider(spec),
                ..
            } if spec.kind == SliderKind::VadThreshold => Some(*spec),
            _ => None,
        })
        .expect("vad slider");
    let center = app
        .last_tree
        .find_hit(|a| matches!(a, Action::Slider(s) if s.kind == SliderKind::VadThreshold))
        .unwrap();
    let y = center.y.get();
    let x0 = spec.track_x;
    app.event(PlatformEvent::PointerMoved {
        window: WindowId(0),
        x: x0,
        y,
    });
    app.event(button(true, x0, y));
    assert_eq!(app.state().vad_threshold, 0.1);
    let end = spec.track_x + spec.track_w + 30.0;
    app.event(PlatformEvent::PointerMoved {
        window: WindowId(0),
        x: end,
        y,
    });
    assert_eq!(app.state().vad_threshold, 0.9);
    assert!(app.sent.len() >= 2);
    // 拖曳中只改狀態，不寫檔
    assert!(app
        .sent
        .iter()
        .all(|c| matches!(c, UiCommand::UpdateSettings(_))));
    app.event(button(false, end, y));
    assert!(app.slider.is_none());
    // 放開後寫檔一次
    assert_eq!(app.sent.last(), Some(&UiCommand::SaveSettings));
    let saves = app
        .sent
        .iter()
        .filter(|c| **c == UiCommand::SaveSettings)
        .count();
    assert_eq!(saves, 1);
}

#[test]
fn stepper_changes_threads_and_reapplies() {
    let mut app = app_with(AppViewState {
        settings_open: true,
        ..AppViewState::default()
    })
    .0;
    click(
        &mut app,
        560.0,
        680.0,
        is_click(Msg::Setting(SettingEdit::Threads(5))),
    );
    assert_eq!(app.state().n_threads, 5);
    assert!(matches!(app.sent[0], UiCommand::UpdateSettings(_)));
    assert_eq!(app.sent[1], UiCommand::SettingsChanged);
    assert_eq!(app.sent[2], UiCommand::SaveSettings);
    assert_eq!(app.sent.len(), 3);
}

// ─── 文字框 + 輸入法 ─────────────────────────────────────────────────────────

#[test]
fn hotkey_field_typing_ime_and_blur() {
    let mut app = app_with(AppViewState {
        settings_open: true,
        ..AppViewState::default()
    })
    .0;
    click(&mut app, 560.0, 680.0, |a| {
        matches!(
            a,
            Action::Field {
                id: FieldId::Hotkey,
                ..
            }
        )
    });
    assert_eq!(app.local().focus, Some(FieldId::Hotkey));
    let reqs = app.take_window_requests(WindowId(0));
    assert!(reqs.contains(&WindowRequest::SetImeAllowed(true)));
    assert!(!reqs.contains(&WindowRequest::Focus));

    // Space 在文字框裡是文字，不會開始錄音
    app.ui.fields.hotkey.select_all();
    app.event(key(PlatformKey::Other, Some("F")));
    app.event(key(PlatformKey::Other, Some("9")));
    assert_eq!(app.state().hotkey, "F9");
    app.event(key(PlatformKey::Space, Some(" ")));
    assert!(!app.state().is_recording);
    assert!(!app.sent.contains(&UiCommand::ToggleRecording));
    app.event(key(PlatformKey::Backspace, None));
    assert_eq!(app.state().hotkey, "F9");

    // 輸入法組字 → 確定
    app.event(PlatformEvent::ImePreedit {
        window: WindowId(0),
        text: "ㄅ".into(),
        cursor: Some((0, 3)),
    });
    app.build_scene(560.0, 680.0, 1.0).unwrap();
    assert!(app.ime_cursor_area(WindowId(0)).is_some());
    assert_eq!(app.local().fields.hotkey.preedit, "ㄅ");
    assert_eq!(app.state().hotkey, "F9", "preedit is not committed text");
    app.event(PlatformEvent::ImeCommit {
        window: WindowId(0),
        text: "鍵".into(),
    });
    assert_eq!(app.state().hotkey, "F9鍵");
    assert!(!app.sent.contains(&UiCommand::SettingsChanged));

    assert!(!app.sent.contains(&UiCommand::SaveSettings), "打字中不寫檔");

    // Enter 送出：失去焦點、重新套用熱鍵並寫檔
    app.event(key(PlatformKey::Enter, None));
    assert_eq!(app.local().focus, None);
    let n = app.sent.len();
    assert_eq!(
        app.sent[n - 2..],
        [UiCommand::SettingsChanged, UiCommand::SaveSettings]
    );
    app.build_scene(560.0, 680.0, 1.0).unwrap();
    assert!(app.ime_cursor_area(WindowId(0)).is_none());
    let reqs = app.take_window_requests(WindowId(0));
    assert!(reqs.contains(&WindowRequest::SetImeAllowed(false)));
}

#[test]
fn template_editor_new_save_delete() {
    let mut app = app_with(AppViewState {
        settings_open: true,
        template_entries: templates(),
        ..AppViewState::default()
    })
    .0;
    click(&mut app, 560.0, 680.0, is_click(Msg::TemplateEditNew));
    assert_eq!(app.local().fields.template_name.text, "新模板");
    click(&mut app, 560.0, 680.0, |a| {
        matches!(
            a,
            Action::Field {
                id: FieldId::TemplatePrompt,
                ..
            }
        )
    });
    app.event(PlatformEvent::ImeCommit {
        window: WindowId(0),
        text: "請用正式語氣".into(),
    });
    click(&mut app, 560.0, 680.0, is_click(Msg::TemplateEditSave));
    assert_eq!(
        app.sent,
        vec![UiCommand::TemplateSave {
            original: String::new(),
            name: "新模板".into(),
            prompt: "請用正式語氣".into(),
        }]
    );
    app.sent.clear();
    click(&mut app, 560.0, 680.0, is_click(Msg::TemplateEditSelect(1)));
    click(&mut app, 560.0, 680.0, is_click(Msg::TemplateEditDelete));
    assert_eq!(app.sent, vec![UiCommand::TemplateDelete("我的模板".into())]);
    assert!(app.local().template_edit.is_none());

    // 內建模板不能刪除
    click(&mut app, 560.0, 680.0, is_click(Msg::TemplateEditSelect(0)));
    app.build_scene(560.0, 680.0, 1.0).unwrap();
    assert!(app
        .last_tree
        .find_hit(|a| *a == Action::Click(Msg::TemplateEditDelete))
        .is_none());
}

// ─── 鍵盤快捷鍵 ──────────────────────────────────────────────────────────────

#[test]
fn window_shortcuts_go_through_key_to_command() {
    let mut app = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    })
    .0;
    app.build_scene(380.0, 52.0, 1.0).unwrap();
    app.event(key(PlatformKey::Space, Some(" ")));
    assert!(app.state().is_recording);
    app.event(key(PlatformKey::Other, Some("e")));
    assert!(app.state().floating_expanded);
    app.event(key(PlatformKey::Escape, None));
    assert!(!app.state().floating_expanded);
    assert_eq!(
        app.sent,
        vec![
            UiCommand::ToggleRecording,
            UiCommand::SetFloatingExpanded(true),
            UiCommand::SetFloatingExpanded(false)
        ]
    );
}

// ─── 背景事件 / 焦點 / 閒置 ─────────────────────────────────────────────────

#[test]
fn hotkey_and_ptt_never_request_focus() {
    let (mut app, notifier) = app_with(AppViewState::default());
    let _ = app.take_window_requests(WindowId(0));
    app.geometry_pending = false; // 啟動時的一次性套用（測試中找不到 HWND）
    notifier.send(CoreEvent::HotkeyPressed);
    assert!(app.wake());
    assert!(app.state().is_recording);
    notifier.send(CoreEvent::PttKey(false));
    notifier.send(CoreEvent::PttKey(true));
    assert!(app.wake());
    assert!(app.state().is_recording);
    notifier.send(CoreEvent::PttKey(false));
    app.wake();
    assert!(!app.state().is_recording);
    let reqs = app.take_window_requests(WindowId(0));
    assert!(
        !reqs
            .iter()
            .any(|r| matches!(r, WindowRequest::Focus | WindowRequest::SetImeAllowed(true))),
        "hotkey path must not touch focus: {reqs:?}"
    );
    assert!(
        !app.geometry_pending,
        "hotkey path must not move the window"
    );
    assert!(app.sent.is_empty(), "hotkeys go through apply, not handle");
}

#[test]
fn wake_drains_waveform_and_transcript_events() {
    let (mut app, notifier) = app_with(AppViewState::default());
    assert!(!app.wake(), "nothing pending");
    notifier.send(CoreEvent::Waveform(vec![0.5; 20]));
    notifier.send(CoreEvent::Waveform(vec![0.7; 20]));
    notifier.send(CoreEvent::TranscriptSegment {
        text: "你好".into(),
        timestamp: "10:00".into(),
        status: "就緒".into(),
    });
    assert!(app.wake());
    assert_eq!(app.state().waveform_bars, vec![0.7; 20]);
    assert_eq!(app.state().transcript_segments.len(), 1);
    assert_eq!(app.state().char_count, 2);
}

#[test]
fn idle_poll_interval_is_none() {
    let mut app = app();
    assert_eq!(app.background_poll_interval(), None);
    app.send(UiCommand::ToggleRecording);
    assert_eq!(app.background_poll_interval(), Some(ANIMATION_INTERVAL));
    app.send(UiCommand::ToggleRecording);
    assert_eq!(app.background_poll_interval(), None);
}

#[test]
fn transcript_scroll_unfollows_and_refollows() {
    let mut app = app_with(rich_state()).0;
    app.build_scene(560.0, 680.0, 1.0).unwrap();
    let area = app
        .hitmap
        .scrolls
        .iter()
        .find(|a| a.id == ScrollId::Transcript)
        .copied()
        .expect("transcript scroll area");
    assert!(area.max > 0.0, "rich transcript should overflow");
    let (x, y) = (
        area.rect.origin.x.get() + 20.0,
        area.rect.origin.y.get() + 20.0,
    );
    let wheel = |dy: f32| PlatformEvent::Scroll {
        window: WindowId(0),
        delta_x: 0.0,
        delta_y: dy,
        lines: true,
        x,
        y,
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };
    assert!(app.event(wheel(2.0)));
    assert!(!app.local().follow_transcript);
    assert!((app.local().scroll(ScrollId::Transcript) - (area.max - 80.0)).abs() < 0.01);
    app.event(wheel(-1000.0));
    assert!(app.local().follow_transcript);
}

#[test]
fn window_config_follows_mode_and_is_borderless() {
    let cfg = app().window_config();
    assert!(!cfg.decorations);
    assert_eq!((cfg.width, cfg.height), (560.0, 620.0));
    let floating = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    })
    .0
    .window_config();
    assert_eq!((floating.width, floating.height), (380.0, 52.0));
    assert!(floating.title.contains("whisper-typeless"));
}

// ── 系統匣 / 視窗隱藏 ────────────────────────────────────────────────────────

#[test]
fn close_without_tray_still_closes_window() {
    let mut app = app();
    app.dispatch(Msg::Core(UiCommand::RequestClose));
    assert!(app.pending_requests().contains(&WindowRequest::Close));
    assert!(app.quitting);
}

#[test]
fn close_with_tray_hides_instead_of_closing() {
    let mut app = app();
    app.tray_ok = true;
    app.dispatch(Msg::Core(UiCommand::RequestClose));
    assert!(app.state().window_hidden);
    assert!(!app.pending_requests().contains(&WindowRequest::Close));
    assert!(!app.quitting);
}

#[test]
fn close_requested_vetoed_only_with_tray_and_setting() {
    let w = WindowId(1);
    let mut app = app();
    // 無系統匣：放行並標記結束
    assert!(app.close_requested(w));
    assert!(app.state().quit_requested);

    let mut app = self::app();
    app.tray_ok = true;
    assert!(!app.close_requested(w), "Alt+F4 應被否決並縮到系統匣");
    assert!(app.state().window_hidden);

    let mut app = self::app();
    app.tray_ok = true;
    app.controller.state_mut().close_to_tray = false;
    assert!(app.close_requested(w));

    // 已在結束流程時不再否決
    let mut app = self::app();
    app.tray_ok = true;
    app.request_quit();
    assert!(app.close_requested(w));
}

#[test]
fn quit_request_closes_once() {
    let mut app = app();
    app.dispatch(Msg::Core(UiCommand::Quit));
    app.dispatch(Msg::Core(UiCommand::Quit));
    let closes = app
        .pending_requests()
        .iter()
        .filter(|r| **r == WindowRequest::Close)
        .count();
    assert_eq!(closes, 1);
}

#[test]
fn tray_quit_event_closes_window() {
    let (mut app, notifier) = app_with(AppViewState::default());
    notifier.send(CoreEvent::Tray(crate::core::TrayAction::Quit));
    assert!(app.pump());
    assert!(app.pending_requests().contains(&WindowRequest::Close));
}

#[test]
fn minimize_hides_only_when_enabled_and_tray_present() {
    let mut app = app();
    app.dispatch(Msg::Minimize);
    assert!(app.pending_requests().contains(&WindowRequest::Minimize));
    assert!(!app.state().window_hidden);

    let mut app = self::app();
    app.tray_ok = true;
    app.controller.state_mut().minimize_to_tray = true;
    app.dispatch(Msg::Minimize);
    assert!(app.state().window_hidden);
    assert!(!app.pending_requests().contains(&WindowRequest::Minimize));
}

#[test]
fn hotkey_events_do_not_change_window_visibility() {
    let (mut app, notifier) = app_with(AppViewState::default());
    app.tray_ok = true;
    app.controller.state_mut().window_hidden = true;
    app.shown = false;
    notifier.send(CoreEvent::HotkeyPressed);
    assert!(app.pump());
    assert!(app.state().window_hidden);
    assert!(!app.shown);
    assert!(!app.pending_requests().contains(&WindowRequest::Focus));
}

#[test]
fn tray_show_marks_window_visible_again() {
    let (mut app, notifier) = app_with(AppViewState::default());
    app.controller.state_mut().window_hidden = true;
    app.shown = false;
    notifier.send(CoreEvent::Tray(crate::core::TrayAction::ToggleWindow));
    app.pump();
    assert!(!app.state().window_hidden);
}

// ─── v0.6 契約：設定分頁、錯誤紀錄 ─────────────────────────────────────────────

#[test]
fn toggle_error_log_expands_collapsed_bar_first() {
    let (mut app, _n) = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    });
    app.dispatch(Msg::ToggleErrorLog);
    assert_eq!(
        app.sent,
        vec![
            UiCommand::SetFloatingExpanded(true),
            UiCommand::SetShowErrorLog(true)
        ]
    );
    assert!(app.state().floating_expanded && app.state().show_error_log);
    app.sent.clear();
    app.dispatch(Msg::ToggleErrorLog);
    assert_eq!(app.sent, vec![UiCommand::SetShowErrorLog(false)]);
    assert!(app.state().floating_expanded, "關閉錯誤紀錄不收合");
}

#[test]
fn open_settings_selects_tab_and_resets_scroll() {
    let (mut app, _n) = app_with(AppViewState {
        floating_mode: true,
        ..AppViewState::default()
    });
    app.ui.set_scroll(ScrollId::Settings, 300.0);
    app.dispatch(Msg::OpenSettings(SettingsTab::Models));
    assert_eq!(
        app.sent,
        vec![
            UiCommand::SetFloatingExpanded(true),
            UiCommand::SetSettingsOpen(true)
        ]
    );
    assert_eq!(app.local().settings_tab, SettingsTab::Models);
    assert_eq!(app.local().scroll(ScrollId::Settings), 0.0);

    // 已開啟：只換分頁，不重送開啟指令。
    app.sent.clear();
    app.ui.set_scroll(ScrollId::Settings, 120.0);
    app.dispatch(Msg::SettingsTab(SettingsTab::Advanced));
    assert!(app.sent.is_empty());
    assert_eq!(app.local().settings_tab, SettingsTab::Advanced);
    assert_eq!(app.local().scroll(ScrollId::Settings), 0.0);
    app.dispatch(Msg::OpenSettings(SettingsTab::Engine));
    assert!(app.sent.is_empty());
    assert_eq!(app.local().settings_tab, SettingsTab::Engine);
}

#[test]
fn switching_tab_blurs_hotkey_field_and_reapplies() {
    let mut app = app();
    app.ui.focus = Some(FieldId::Hotkey);
    app.dispatch(Msg::SettingsTab(SettingsTab::Output));
    assert!(app.local().focus.is_none());
    assert_eq!(app.sent, vec![UiCommand::SettingsChanged]);
}

#[test]
fn new_setting_edits_reach_core_state() {
    let mut app = app();
    app.dispatch(Msg::Setting(SettingEdit::CloseToTray(false)));
    app.dispatch(Msg::Setting(SettingEdit::StartMinimized(true)));
    app.dispatch(Msg::Setting(SettingEdit::AppendNewline(false)));
    let s = app.state();
    assert!(!s.close_to_tray && s.start_minimized && !s.append_newline);
    assert!(!app.sent.contains(&UiCommand::SettingsChanged));
}

// ─── WP-1b：即改即存、視窗快捷鍵、快捷鍵錄入 ─────────────────────────────────

fn ctrl_key(k: PlatformKey, ch: &str, shift: bool) -> PlatformEvent {
    PlatformEvent::Key {
        window: WindowId(0),
        key: k,
        pressed: true,
        shift,
        ctrl: true,
        alt: false,
        meta: false,
        repeat: false,
        text: None,
        character: Some(ch.to_string()),
    }
}

#[test]
fn toggles_and_direct_setting_commands_save_immediately() {
    let mut app = app();
    app.dispatch(Msg::Setting(SettingEdit::PttMode(true)));
    assert_eq!(app.sent.last(), Some(&UiCommand::SaveSettings));
    assert_eq!(
        app.sent.len(),
        3,
        "UpdateSettings, SettingsChanged, SaveSettings"
    );

    for cmd in [
        UiCommand::SetDarkMode(true),
        UiCommand::SetClipboardEnabled(false),
        UiCommand::SetInjectEnabled(false),
        UiCommand::SetPrivacyMode(true),
    ] {
        app.sent.clear();
        app.dispatch(Msg::Core(cmd.clone()));
        assert_eq!(app.sent, vec![cmd, UiCommand::SaveSettings]);
    }
    let snap = app.state().ui_settings();
    assert!(snap.dark_mode && snap.privacy_mode);
    assert!(!snap.clipboard_enabled && !snap.inject_enabled);

    // 舊畫面直接送整份快照（系統匣開關）也會寫檔
    app.sent.clear();
    let mut snap = app.state().ui_settings();
    snap.close_to_tray = !snap.close_to_tray;
    app.dispatch(Msg::Core(UiCommand::UpdateSettings(Box::new(snap))));
    assert_eq!(app.sent.last(), Some(&UiCommand::SaveSettings));

    // 非設定指令不寫檔
    app.sent.clear();
    app.dispatch(Msg::Core(UiCommand::SetFileMode(true)));
    app.dispatch(Msg::Core(UiCommand::ClearTranscript));
    assert!(!app.sent.contains(&UiCommand::SaveSettings));
}

#[test]
fn dark_mode_survives_save_and_reload() {
    let mut app = app();
    app.dispatch(Msg::Core(UiCommand::SetDarkMode(true)));
    let saved = app.state().ui_settings();
    let mut config = AppConfig::default();
    saved.apply_to(&mut config);
    assert_eq!(config.general.theme, "dark");
    let loaded = UiSettings::from_config(&config);
    assert!(loaded.dark_mode);
    let mut fresh = self::app();
    fresh.dispatch(Msg::Core(UiCommand::UpdateSettings(Box::new(loaded))));
    assert!(fresh.state().dark_mode);
}

#[test]
fn hotkey_blur_without_edit_does_not_save() {
    let mut app = app();
    app.ui.focus = Some(FieldId::PttHotkey);
    app.dispatch(Msg::SettingsTab(SettingsTab::Output));
    assert_eq!(app.sent, vec![UiCommand::SettingsChanged]);
}

#[test]
fn ctrl_comma_toggles_settings_and_resets_tab() {
    let mut app = app();
    app.ui.settings_tab = SettingsTab::Advanced;
    app.ui.set_scroll(ScrollId::Settings, 200.0);
    assert!(app.event(ctrl_key(PlatformKey::Other, ",", false)));
    assert!(app.state().settings_open);
    assert_eq!(
        app.local().settings_tab,
        SettingsTab::General,
        "開啟回到一般"
    );
    assert_eq!(app.local().scroll(ScrollId::Settings), 0.0);
    app.event(ctrl_key(PlatformKey::Other, ",", false));
    assert!(!app.state().settings_open);
    assert!(!app.pending_requests().contains(&WindowRequest::Focus));
}

#[test]
fn gear_reopens_settings_on_general_tab() {
    let mut app = app();
    app.dispatch(Msg::OpenSettings(SettingsTab::Models));
    app.dispatch(Msg::ToggleSettings);
    assert!(!app.state().settings_open);
    assert_eq!(
        app.local().settings_tab,
        SettingsTab::Models,
        "關閉不改分頁"
    );
    app.dispatch(Msg::ToggleSettings);
    assert!(app.state().settings_open);
    assert_eq!(app.local().settings_tab, SettingsTab::General);
}

#[test]
fn ctrl_digits_switch_mode_or_settings_tab() {
    let mut app = app();
    app.event(ctrl_key(PlatformKey::Other, "2", false));
    assert!(app.state().file_mode);
    app.event(ctrl_key(PlatformKey::Other, "1", false));
    assert!(!app.state().file_mode);
    assert!(!app.event(ctrl_key(PlatformKey::Other, "5", false)));

    app.dispatch(Msg::ToggleSettings);
    for (i, tab) in SettingsTab::ALL.into_iter().enumerate() {
        let digit = (i + 1).to_string();
        app.event(ctrl_key(PlatformKey::Other, &digit, false));
        assert_eq!(app.local().settings_tab, tab);
    }
    assert!(!app.state().file_mode, "設定開啟時 Ctrl+2 只切分頁");
}

#[test]
fn window_shortcuts_ignored_while_text_field_focused() {
    let mut app = app();
    app.ui.focus = Some(FieldId::TemplateName);
    app.ui.fields.template_name.focused = true;
    app.event(ctrl_key(PlatformKey::Other, ",", false));
    app.event(ctrl_key(PlatformKey::Other, "2", false));
    assert!(!app.state().settings_open);
    assert!(!app.state().file_mode);
}

#[test]
fn hotkey_field_captures_pressed_combo() {
    let mut app = app_with(AppViewState {
        settings_open: true,
        ..AppViewState::default()
    })
    .0;
    app.ui.focus = Some(FieldId::Hotkey);
    app.ui.fields.hotkey.focused = true;
    app.event(ctrl_key(PlatformKey::Other, "K", true));
    assert_eq!(app.state().hotkey, "Ctrl+Shift+K");
    assert_eq!(app.local().fields.hotkey.text, "Ctrl+Shift+K");
    assert!(
        !app.sent.contains(&UiCommand::SettingsChanged),
        "失焦才重新註冊"
    );
    // 單純 Ctrl+A 仍是全選，不錄入
    app.event(ctrl_key(PlatformKey::Other, "a", false));
    assert_eq!(app.state().hotkey, "Ctrl+Shift+K");
    // 單獨的 F 鍵也能錄入；設定開啟中的 Ctrl+2 被錄成熱鍵而不是切分頁
    app.event(key(PlatformKey::Function(9), None));
    assert_eq!(app.state().hotkey, "F9");
    app.event(ctrl_key(PlatformKey::Other, "2", false));
    assert_eq!(app.state().hotkey, "Ctrl+2");
    assert_eq!(app.local().settings_tab, SettingsTab::General);
    app.event(key(PlatformKey::Enter, None));
    assert_eq!(app.sent.last(), Some(&UiCommand::SaveSettings));
}
