//! AcmeUI 應用程式：擁有核心 [`Controller`]，把平台事件轉成 [`UiCommand`]、把背景
//! [`CoreEvent`](crate::core::CoreEvent) 套用到狀態，並把 [`AppViewState`] 畫成畫面。
//!
//! - 背景事件：`Notifier` 的 wake → acme `Waker::wake` → [`Application::wake`] →
//!   `EventQueue::drain` → `Controller::apply`。
//! - 指標：`ClickDetector` + `HitTestLayer` → [`Action`] → [`Msg`] → `Controller::handle`。
//! - 鍵盤：文字框有焦點時交給 acme-textinput；否則 `core::keys::key_to_command`。
//! - `ViewRequest`：尺寸 / 置頂走 [`hwnd`]（`SWP_NOACTIVATE`）、隱藏 = 關閉視窗、拖曳 = `DragMove`。
//! - 熱鍵 / PTT 只會改核心狀態，**永遠不會**送出 `WindowRequest::Focus` 或啟用視窗。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use acme_core::{
    ClickDetector, CursorHint, HitTestLayer, NodeId, Painter, Point, PointerOutcome, Scene,
    SceneError,
};
use acme_platform::{
    Application, Clipboard, CursorIcon, FrameContext, PlatformEvent, PlatformKey,
    RenderBackendPreference, Waker, WindowConfig, WindowId, WindowRequest,
};

use super::controls::{field_key, ime_commit, ime_preedit, offset_at_x, slider_value, FieldKey};
use super::hwnd;
use super::local::UiLocal;
use super::msg::{FieldId, Msg, ScrollId, SettingEdit, SliderKind};
use super::paint::{paint_view, Action, HitMap, SliderSpec};
use super::text::TextEngine;
use super::theme::Theme;
use super::views::{self, widgets::FONT_SM, window_size, Ctx, BAR_H, BAR_W};
use crate::core::keys::{key_to_command, Key, KeyPress, Modifiers};
use crate::core::{AppViewState, Controller, EventQueue, UiCommand, ViewRequest};

/// 錄音 / 處理中的動畫更新間隔。
pub const ANIMATION_INTERVAL: Duration = Duration::from_millis(33);

/// 背景執行緒用來喚醒畫面的槽（`on_waker` 之前送出的事件會在拿到 Waker 時補喚醒）。
pub type WakerSlot = Arc<Mutex<Option<Waker>>>;

/// 建立給 `core::event_channel` 用的喚醒函式。
pub fn wake_fn(slot: &WakerSlot) -> impl Fn() + Send + Sync + 'static {
    let slot = Arc::clone(slot);
    move || {
        if let Some(waker) = slot.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            waker.wake();
        }
    }
}

pub struct AcmeApp {
    controller: Controller,
    queue: EventQueue,
    waker_slot: WakerSlot,
    ui: UiLocal,
    text: TextEngine,
    hit: HitTestLayer,
    detector: ClickDetector,
    hitmap: HitMap,
    cursor: CursorHint,
    requests: Vec<WindowRequest>,
    geometry_pending: bool,
    scale: f32,
    hwnd: Option<isize>,
    started: Instant,
    /// 按住中的錄音鍵：放開時要送的訊息。
    hold: Option<Msg>,
    /// 拖曳中的滑桿。
    slider: Option<(NodeId, SliderSpec)>,
    ime_area: Option<[f32; 4]>,
    ime_allowed: Option<bool>,
    clipboard: Option<Clipboard>,
    last_recording: bool,
    /// 已畫過至少一個 frame（DPI 縮放已知），之後才能以實體像素調整視窗。
    frame_seen: bool,
    #[cfg(test)]
    sent: Vec<UiCommand>,
    #[cfg(test)]
    last_tree: super::paint::View,
}

impl AcmeApp {
    pub fn new(controller: Controller, queue: EventQueue, waker_slot: WakerSlot) -> Self {
        let last_recording = controller.state().is_recording;
        let mut ui = UiLocal::default();
        ui.sync_fields(controller.state());
        Self {
            controller,
            queue,
            waker_slot,
            ui,
            text: TextEngine::new(),
            hit: HitTestLayer::new(),
            detector: ClickDetector::new(),
            hitmap: HitMap::default(),
            cursor: CursorHint::Default,
            requests: Vec::new(),
            geometry_pending: true,
            scale: 1.0,
            hwnd: None,
            started: Instant::now(),
            hold: None,
            slider: None,
            ime_area: None,
            ime_allowed: None,
            clipboard: None,
            last_recording,
            frame_seen: false,
            #[cfg(test)]
            sent: Vec::new(),
            #[cfg(test)]
            last_tree: super::paint::View::new(),
        }
    }

    pub fn state(&self) -> &AppViewState {
        self.controller.state()
    }

    pub fn local(&self) -> &UiLocal {
        &self.ui
    }

    /// 不開視窗建出一個 frame（測試用；正式執行走 [`Application::frame`]）。
    pub fn build_scene(
        &mut self,
        width: f32,
        height: f32,
        scale: f32,
    ) -> Result<Scene, SceneError> {
        self.scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        self.frame_seen = true;
        self.text.begin_frame();
        self.ui.sync_fields(self.controller.state());
        let theme = Theme::for_mode(self.controller.state().dark_mode);
        let tree = {
            let mut cx = Ctx {
                s: self.controller.state(),
                ui: &self.ui,
                t: theme,
                text: &mut self.text,
                scale: self.scale,
            };
            views::build(&mut cx, width, height)
        };
        self.ime_area = tree.ime_area;
        let mut painter = Painter::with_clear(theme.bg);
        self.hitmap = paint_view(
            &tree,
            &mut self.text,
            &mut painter,
            &mut self.hit,
            self.detector.hovered(),
            self.scale,
        );
        #[cfg(test)]
        {
            self.last_tree = tree;
        }
        painter.finish_checked()
    }

    // ── 指令 ────────────────────────────────────────────────────────────────

    fn send(&mut self, cmd: UiCommand) {
        #[cfg(test)]
        self.sent.push(cmd.clone());
        let requests = self.controller.handle(cmd);
        for request in requests {
            self.perform(request);
        }
        self.after_state_change();
    }

    fn perform(&mut self, request: ViewRequest) {
        match request {
            ViewRequest::ApplyWindowGeometry => self.geometry_pending = true,
            ViewRequest::HideWindow => self.requests.push(WindowRequest::Close),
            ViewRequest::StartDrag => self.start_drag(),
            // WP-B：從系統匣叫回視窗（需 SW_SHOW；目前視窗不會被隱藏，先不處理）。
            ViewRequest::ShowWindow => {}
            // WP-B：改為結束事件迴圈並移除系統匣圖示。
            ViewRequest::Quit => self.requests.push(WindowRequest::Close),
        }
    }

    fn start_drag(&mut self) {
        self.requests.push(WindowRequest::DragMove);
        // 作業系統接手移動迴圈，放開事件不會送達：取消目前的按下狀態。
        let _ = self.detector.cancel();
        self.release_hold();
    }

    fn after_state_change(&mut self) {
        let recording = self.controller.state().is_recording;
        if recording != self.last_recording {
            self.last_recording = recording;
            crate::ui::window_icon::update(recording);
        }
    }

    /// 處理畫面訊息。
    pub fn dispatch(&mut self, msg: Msg) {
        let s = self.controller.state();
        let (floating, expanded, settings_open, recording) = (
            s.floating_mode,
            s.floating_expanded,
            s.settings_open,
            s.is_recording,
        );
        match msg {
            Msg::Core(cmd) => self.send(cmd),
            Msg::ToggleFloating => {
                if !floating {
                    self.send(UiCommand::SetFloatingExpanded(false));
                }
                self.send(UiCommand::SetFloatingMode(!floating));
            }
            Msg::ToggleExpanded => self.send(UiCommand::SetFloatingExpanded(!expanded)),
            Msg::ToggleSettings => {
                if !settings_open && floating && !expanded {
                    self.send(UiCommand::SetFloatingExpanded(true));
                }
                self.send(UiCommand::SetSettingsOpen(!settings_open));
            }
            Msg::TogglePin => {
                self.ui.pinned = !self.ui.pinned;
                self.geometry_pending = true;
            }
            Msg::Minimize => self.requests.push(WindowRequest::Minimize),
            Msg::RecordPress => {
                if !recording {
                    self.send(UiCommand::ToggleRecording);
                }
            }
            Msg::RecordRelease => {
                if recording {
                    self.send(UiCommand::ToggleRecording);
                }
            }
            Msg::Setting(edit) => self.apply_setting(edit, true),
            Msg::TemplateEditSelect(i) => {
                self.blur();
                if let Some(entry) = self.controller.state().template_entries.get(i).cloned() {
                    self.ui.edit_template(i, &entry);
                }
            }
            Msg::TemplateEditNew => {
                self.blur();
                self.ui.new_template();
            }
            Msg::TemplateEditSave => {
                self.blur();
                if let Some(edit) = self.ui.template_edit.clone() {
                    let name = self.ui.fields.template_name.text.trim().to_string();
                    if name.is_empty() {
                        return;
                    }
                    let prompt = self.ui.fields.template_prompt.text.clone();
                    self.send(UiCommand::TemplateSave {
                        original: edit.original.clone(),
                        name: name.clone(),
                        prompt,
                    });
                    if let Some(e) = &mut self.ui.template_edit {
                        e.original = name;
                        e.index = None;
                    }
                }
            }
            Msg::TemplateEditDelete => {
                self.blur();
                if let Some(edit) = self.ui.template_edit.take() {
                    if !edit.original.is_empty() {
                        self.send(UiCommand::TemplateDelete(edit.original));
                    }
                }
            }
        }
    }

    fn apply_setting(&mut self, edit: SettingEdit, reapply: bool) {
        let mut snapshot = self.controller.state().ui_settings();
        edit.apply(&mut snapshot);
        self.send(UiCommand::UpdateSettings(Box::new(snapshot)));
        if reapply && edit.needs_settings_changed() {
            self.send(UiCommand::SettingsChanged);
        }
    }

    // ── 文字框 ──────────────────────────────────────────────────────────────

    fn focus_field(&mut self, id: FieldId, local_x: f32) {
        if self.ui.focus != Some(id) {
            self.blur();
        }
        self.ui.focus = Some(id);
        let style = TextEngine::style(FONT_SM, false);
        let scale = self.scale;
        let field = self.ui.fields.get_mut(id);
        field.focused = true;
        let offset = offset_at_x(&field.text, local_x, self.text.fonts_mut(), &style, scale);
        field.move_cursor_to_offset(offset);
    }

    fn blur(&mut self) {
        let Some(id) = self.ui.focus.take() else {
            return;
        };
        let field = self.ui.fields.get_mut(id);
        field.focused = false;
        field.cancel_preedit();
        if matches!(id, FieldId::Hotkey | FieldId::PttHotkey) {
            // 熱鍵在失去焦點時才重新註冊（避免每打一個字就換一次熱鍵）。
            self.send(UiCommand::SettingsChanged);
        }
    }

    fn on_field_edited(&mut self, id: FieldId) {
        let text = self.ui.fields.get(id).text.clone();
        match id {
            FieldId::Hotkey => self.apply_setting(SettingEdit::Hotkey(text), false),
            FieldId::PttHotkey => self.apply_setting(SettingEdit::PttHotkey(text), false),
            FieldId::TemplateName | FieldId::TemplatePrompt => {}
        }
    }

    // ── 指標 ────────────────────────────────────────────────────────────────

    fn release_hold(&mut self) {
        if let Some(up) = self.hold.take() {
            self.dispatch(up);
        }
    }

    fn slide(&mut self, spec: SliderSpec, x: f32) {
        let value = slider_value(x, spec.track_x, spec.track_w, spec.min, spec.max, spec.step);
        let s = self.controller.state();
        let (edit, current) = match spec.kind {
            SliderKind::VadThreshold => (SettingEdit::VadThreshold(value), s.vad_threshold),
            SliderKind::Temperature => (SettingEdit::Temperature(value), s.temperature),
        };
        if (current - value).abs() > f32::EPSILON {
            self.apply_setting(edit, true);
        }
    }

    fn route(&mut self, outcome: PointerOutcome) {
        match outcome {
            PointerOutcome::Pressed {
                id,
                button: 0,
                point,
                ..
            } => match self.hitmap.action(id).cloned() {
                Some(Action::DragMove) => self.start_drag(),
                Some(Action::Hold { down, up }) => {
                    self.hold = Some(up);
                    self.dispatch(down);
                }
                Some(Action::Slider(spec)) => {
                    self.slider = Some((id, spec));
                    self.slide(spec, point.x.get());
                }
                Some(Action::Field { id, text_x }) => self.focus_field(id, point.x.get() - text_x),
                _ => {}
            },
            PointerOutcome::DragStart {
                id, current_point, ..
            }
            | PointerOutcome::DragMove {
                id, current_point, ..
            } => {
                if let Some((sid, spec)) = self.slider {
                    if sid == id {
                        self.slide(spec, current_point.x.get());
                    }
                }
            }
            PointerOutcome::Released { button: 0, .. } | PointerOutcome::Cancelled { .. } => {
                self.slider = None;
                self.release_hold();
            }
            PointerOutcome::Clicked { id, button: 0, .. } => {
                if let Some(Action::Click(msg)) = self.hitmap.action(id).cloned() {
                    self.dispatch(msg);
                }
            }
            _ => {}
        }
    }

    fn pointer_button(&mut self, pressed: bool, x: f32, y: f32, button: u16) -> bool {
        let point = Point::new(x, y);
        let now = self.started.elapsed().as_secs_f64();
        let outcomes = self
            .detector
            .on_pointer_button(&self.hit, button, pressed, point, now);
        if pressed && button == 0 && self.ui.focus.is_some() {
            let keeps_focus = outcomes.iter().any(|o| match o {
                PointerOutcome::Pressed { id, .. } => matches!(
                    self.hitmap.action(*id),
                    Some(Action::Field { id: f, .. }) if Some(*f) == self.ui.focus
                ),
                _ => false,
            });
            if !keeps_focus {
                self.blur();
            }
        }
        let any = !outcomes.is_empty();
        for outcome in outcomes {
            self.route(outcome);
        }
        if !pressed {
            let _ = self.detector.on_pointer_move(&self.hit, point);
        }
        self.sync_cursor();
        any
    }

    fn scroll(&mut self, x: f32, y: f32, delta_y: f32, lines: bool) -> bool {
        let Some(area) = self.hitmap.scroll_at(Point::new(x, y)) else {
            return false;
        };
        let px = if lines { delta_y * 40.0 } else { delta_y };
        let follow = area.id == ScrollId::Transcript && self.ui.follow_transcript;
        let current = if follow {
            area.max
        } else {
            self.ui.scroll(area.id).min(area.max)
        };
        let next = (current - px).clamp(0.0, area.max);
        self.ui.set_scroll(area.id, next);
        if area.id == ScrollId::Transcript {
            self.ui.follow_transcript = next >= area.max - 0.5;
        }
        (next - current).abs() > f32::EPSILON
    }

    fn sync_cursor(&mut self) {
        let next = self.detector.cursor();
        if next != self.cursor {
            self.cursor = next;
            self.requests
                .push(WindowRequest::SetCursor(map_cursor(next)));
        }
    }

    // ── 鍵盤 ────────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn key(
        &mut self,
        key: PlatformKey,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
        text: Option<&str>,
        character: Option<&str>,
    ) -> bool {
        if let Some(id) = self.ui.focus {
            if self.clipboard.is_none() && ctrl {
                self.clipboard = Clipboard::new().ok();
            }
            let field = self.ui.fields.get_mut(id);
            let outcome = field_key(
                field,
                key,
                ctrl,
                alt,
                shift,
                character,
                text,
                self.clipboard.as_ref(),
            );
            match outcome {
                FieldKey::Handled { edited } => {
                    if edited {
                        self.on_field_edited(id);
                    }
                    return true;
                }
                FieldKey::Submit | FieldKey::Blur => {
                    self.blur();
                    return true;
                }
                FieldKey::Ignored => {}
            }
        }
        let core_key = match key {
            PlatformKey::Escape => Key::Escape,
            PlatformKey::Space => Key::Space,
            _ => match character.or(text).map(|c| {
                let mut chars = c.chars();
                (chars.next(), chars.next())
            }) {
                Some((Some(c), None)) => Key::Char(c),
                _ => Key::Other,
            },
        };
        let press = KeyPress {
            key: core_key,
            modifiers: Modifiers {
                control: ctrl,
                alt,
                meta,
                shift,
            },
        };
        match key_to_command(self.controller.state(), &press, self.ui.text_focused()) {
            Some(cmd) => {
                self.send(cmd);
                true
            }
            None => false,
        }
    }

    // ── 背景事件 ────────────────────────────────────────────────────────────

    /// 取出並套用所有背景事件；有事件時回傳 true（需要重畫）。
    pub fn pump(&mut self) -> bool {
        let events = self.queue.drain();
        if events.is_empty() {
            return false;
        }
        for event in events {
            self.controller.apply(event);
        }
        self.after_state_change();
        true
    }

    fn apply_geometry(&mut self) {
        if !self.geometry_pending || !self.frame_seen {
            return;
        }
        if self.hwnd.is_none() {
            self.hwnd = hwnd::find_own_hwnd(&self.controller.state().app_title);
        }
        let Some(h) = self.hwnd else { return };
        self.geometry_pending = false;
        let s = self.controller.state();
        let (w, ht) = window_size(s);
        hwnd::set_size(
            h,
            hwnd::scaled_px(w, self.scale),
            hwnd::scaled_px(ht, self.scale),
        );
        hwnd::set_topmost(h, s.floating_mode && self.ui.pinned);
    }

    /// 待送出的視窗請求（測試用，不清空）。
    pub fn pending_requests(&self) -> &[WindowRequest] {
        &self.requests
    }
}

fn map_cursor(hint: CursorHint) -> CursorIcon {
    match hint {
        CursorHint::Pointer => CursorIcon::Pointer,
        CursorHint::Text => CursorIcon::Text,
        CursorHint::Grab => CursorIcon::Grab,
        CursorHint::Grabbing => CursorIcon::Grabbing,
        CursorHint::ResizeEW => CursorIcon::ResizeHorizontal,
        CursorHint::ResizeNS => CursorIcon::ResizeVertical,
        CursorHint::NotAllowed => CursorIcon::NotAllowed,
        _ => CursorIcon::Default,
    }
}

impl Application for AcmeApp {
    fn window_config(&self) -> WindowConfig {
        let (w, h) = window_size(self.controller.state());
        WindowConfig {
            title: self.controller.state().app_title.clone(),
            width: w as f64,
            height: h as f64,
            min_width: Some((BAR_W - 20.0) as f64),
            min_height: Some(BAR_H as f64),
            decorations: false,
            undecorated_shadow: true,
            ..WindowConfig::default()
        }
    }

    /// 小視窗不需要 GPU；`ACME_RENDER_BACKEND` 仍可覆寫。
    fn render_backend(&self) -> RenderBackendPreference {
        RenderBackendPreference::Cpu
    }

    fn on_waker(&mut self, waker: Waker) {
        *self.waker_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(waker.clone());
        // 拿到 Waker 前送出的事件已設了「待喚醒」旗標：補喚醒一次讓它們被 drain。
        waker.wake();
    }

    fn wake(&mut self) -> bool {
        self.pump()
    }

    fn background_poll_interval(&self) -> Option<Duration> {
        let s = self.controller.state();
        (s.is_recording || s.is_processing).then_some(ANIMATION_INTERVAL)
    }

    fn background_tick(&mut self) -> bool {
        self.ui.anim = self.ui.anim.wrapping_add(1);
        self.pump();
        true
    }

    fn take_window_requests(&mut self, _window: WindowId) -> Vec<WindowRequest> {
        self.apply_geometry();
        let want_ime = self.ui.focus.is_some();
        if self.ime_allowed != Some(want_ime) {
            self.ime_allowed = Some(want_ime);
            self.requests.push(WindowRequest::SetImeAllowed(want_ime));
        }
        std::mem::take(&mut self.requests)
    }

    fn ime_cursor_area(&self, _window: WindowId) -> Option<[f32; 4]> {
        self.ime_area
    }

    fn event(&mut self, event: PlatformEvent) -> bool {
        match event {
            PlatformEvent::PointerMoved { x, y, .. } => {
                let prev = self.detector.hovered();
                let outcomes = self.detector.on_pointer_move(&self.hit, Point::new(x, y));
                let sliding = self.slider.is_some();
                for outcome in outcomes {
                    self.route(outcome);
                }
                self.sync_cursor();
                sliding || self.detector.hovered() != prev
            }
            PlatformEvent::PointerLeft { .. } | PlatformEvent::CursorLeft { .. } => {
                let outcomes = self.detector.on_pointer_leave();
                self.sync_cursor();
                !outcomes.is_empty()
            }
            PlatformEvent::PointerButton {
                pressed,
                x,
                y,
                button,
                ..
            } => self.pointer_button(pressed, x, y, button),
            PlatformEvent::Scroll {
                delta_y,
                lines,
                x,
                y,
                ..
            } => self.scroll(x, y, delta_y, lines),
            PlatformEvent::Key {
                key,
                pressed: true,
                shift,
                ctrl,
                alt,
                meta,
                text,
                character,
                ..
            } => self.key(
                key,
                shift,
                ctrl,
                alt,
                meta,
                text.as_deref(),
                character.as_deref(),
            ),
            PlatformEvent::ImePreedit { text, cursor, .. } => match self.ui.focus {
                Some(id) => {
                    ime_preedit(self.ui.fields.get_mut(id), &text, cursor);
                    true
                }
                None => false,
            },
            PlatformEvent::ImeCommit { text, .. } => match self.ui.focus {
                Some(id) => {
                    if ime_commit(self.ui.fields.get_mut(id), &text) {
                        self.on_field_edited(id);
                    }
                    true
                }
                None => false,
            },
            PlatformEvent::FocusChanged { gained: false, .. } => {
                self.release_hold();
                true
            }
            PlatformEvent::Resized { .. } | PlatformEvent::FocusChanged { .. } => true,
            _ => false,
        }
    }

    fn on_gpu_recovered(&mut self, _window: WindowId) {
        self.text.clear();
    }

    fn frame(&mut self, context: FrameContext) -> Scene {
        match self.build_scene(
            context.logical_width,
            context.logical_height,
            context.scale_factor,
        ) {
            Ok(scene) => scene,
            Err(error) => {
                tracing::error!("AcmeUI 建立畫面失敗: {error:?}");
                Scene::new()
            }
        }
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
