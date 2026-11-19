//! 外殼應用程式：狀態（Model）、訊息（Msg）、檢視（view）與 `Application` 實作。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use acme_core::{
    ClickDetector, Color, CursorHint, HitTestLayer, Logical, Painter, Point, Rect, Scene,
    SceneError,
};
use acme_platform::{
    Application, CursorIcon, FrameContext, PlatformEvent, RenderBackendPreference, Waker,
    WindowConfig, WindowId, WindowRequest,
};

use super::hwnd;
use super::paint::{paint_view, route, Action, HitMap, View};
use super::text::{Align, TextEngine};

pub const COMPACT_WIDTH: f32 = 380.0;
pub const COMPACT_HEIGHT: f32 = 52.0;
pub const EXPANDED_HEIGHT: f32 = 540.0;

const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

const BG: Color = rgba(0.10, 0.11, 0.13, 1.0);
const SURFACE: Color = rgba(0.17, 0.18, 0.21, 1.0);
const HOVER: Color = rgba(1.0, 1.0, 1.0, 0.10);
const TEXT: Color = rgba(0.93, 0.94, 0.96, 1.0);
const MUTED: Color = rgba(0.62, 0.65, 0.70, 1.0);
const ACCENT: Color = rgba(0.26, 0.56, 0.96, 1.0);
const RED: Color = rgba(0.92, 0.26, 0.26, 1.0);

/// 應用程式狀態。
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub recording: bool,
    pub expanded: bool,
    pub pinned: bool,
    /// 0.0..=1.0 的音量（spike 中由背景執行緒偽造）。
    pub level: f32,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            recording: false,
            expanded: false,
            pinned: false,
            level: 0.0,
        }
    }
}

/// 使用者意圖。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    ToggleRecord,
    ToggleExpand,
    TogglePin,
}

/// 需要碰視窗的副作用（由 `ShellApp` 在 `take_window_requests` 時執行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    ResizeWindow { expanded: bool },
    SetTopmost(bool),
}

impl Model {
    /// 狀態轉移；回傳需要執行的視窗副作用。
    pub fn update(&mut self, msg: Msg) -> Option<Effect> {
        match msg {
            Msg::ToggleRecord => {
                self.recording = !self.recording;
                None
            }
            Msg::ToggleExpand => {
                self.expanded = !self.expanded;
                Some(Effect::ResizeWindow {
                    expanded: self.expanded,
                })
            }
            Msg::TogglePin => {
                self.pinned = !self.pinned;
                Some(Effect::SetTopmost(self.pinned))
            }
        }
    }

    /// 目前狀態下視窗的邏輯尺寸。
    pub fn window_size(&self) -> (f32, f32) {
        if self.expanded {
            (COMPACT_WIDTH, EXPANDED_HEIGHT)
        } else {
            (COMPACT_WIDTH, COMPACT_HEIGHT)
        }
    }
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect<Logical> {
    Rect::new(x, y, w, h)
}

/// 檢視：狀態 → 描述樹（純函式）。
pub fn view(model: &Model, width: f32, height: f32) -> View<Msg> {
    let mut v = View::new();
    v.drag_area(r(0.0, 0.0, width, height));
    v.rect(r(0.0, 0.0, width, COMPACT_HEIGHT), 0.0, BG);

    // 錄音鍵：外圈 + 內部圖示（待機圓點／錄音方塊）。
    let record = r(9.0, 9.0, 34.0, 34.0);
    let ring = if model.recording { RED } else { SURFACE };
    v.rect(record, 17.0, ring);
    if model.recording {
        v.rect(r(19.0, 19.0, 14.0, 14.0), 3.0, TEXT);
    } else {
        v.rect(r(18.0, 18.0, 16.0, 16.0), 8.0, RED);
    }
    v.button(record, 17.0, Msg::ToggleRecord, HOVER);

    let label = if model.recording {
        "錄音中…"
    } else {
        "按 Ctrl+Alt+L 說話"
    };
    v.text(r(52.0, 6.0, 160.0, 40.0), label, 14.0, TEXT, Align::Start);

    // 音量條。
    let meter = r(216.0, 21.0, 70.0, 10.0);
    v.rect(meter, 5.0, SURFACE);
    let fill = (meter.size.width.get() * model.level.clamp(0.0, 1.0)).max(0.0);
    if fill >= 1.0 {
        v.rect(
            r(216.0, 21.0, fill, 10.0),
            5.0,
            if model.recording { RED } else { ACCENT },
        );
    }

    // 釘選與展開。
    let pin = r(296.0, 9.0, 34.0, 34.0);
    v.rect(pin, 8.0, if model.pinned { ACCENT } else { SURFACE });
    v.text(pin, "釘", 14.0, TEXT, Align::Center);
    v.button(pin, 8.0, Msg::TogglePin, HOVER);

    let expand = r(338.0, 9.0, 34.0, 34.0);
    v.rect(expand, 8.0, SURFACE);
    v.text(
        expand,
        if model.expanded { "收" } else { "展" },
        14.0,
        TEXT,
        Align::Center,
    );
    v.button(expand, 8.0, Msg::ToggleExpand, HOVER);

    if height > COMPACT_HEIGHT + 1.0 {
        let body = r(
            8.0,
            COMPACT_HEIGHT + 4.0,
            width - 16.0,
            height - COMPACT_HEIGHT - 12.0,
        );
        v.rect(body, 8.0, SURFACE);
        v.text(
            r(20.0, COMPACT_HEIGHT + 12.0, width - 40.0, 28.0),
            "最近的辨識結果會顯示在這裡",
            13.0,
            MUTED,
            Align::Start,
        );
    }
    v
}

/// 不開視窗建出一個 frame（測試與 WP3 使用）。
pub fn build_scene(width: f32, height: f32, scale: f32) -> Result<Scene, SceneError> {
    ShellApp::new().build_scene(width, height, scale)
}

/// 偽造音量：第 `tick` 次（每 100ms 一次）的 0.0..=1.0 三角起伏。
pub fn fake_level(tick: u32) -> f32 {
    let phase = (tick % 20) as f32 / 20.0;
    1.0 - (phase * 2.0 - 1.0).abs()
}

pub struct ShellApp {
    model: Model,
    text: TextEngine,
    hit: HitTestLayer,
    detector: ClickDetector,
    hitmap: HitMap<Msg>,
    cursor: CursorHint,
    requests: Vec<WindowRequest>,
    effects: Vec<Effect>,
    scale: f32,
    hwnd: Option<isize>,
    started: Instant,
    level: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
}

impl Default for ShellApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ShellApp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl ShellApp {
    pub fn new() -> Self {
        Self {
            model: Model::default(),
            text: TextEngine::new(),
            hit: HitTestLayer::new(),
            detector: ClickDetector::new(),
            hitmap: HitMap::default(),
            cursor: CursorHint::Default,
            requests: Vec::new(),
            effects: Vec::new(),
            scale: 1.0,
            hwnd: None,
            started: Instant::now(),
            level: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn build_scene(
        &mut self,
        width: f32,
        height: f32,
        scale: f32,
    ) -> Result<Scene, SceneError> {
        self.scale = scale;
        self.text.begin_frame();
        let tree = view(&self.model, width, height);
        let mut painter = Painter::with_clear(BG);
        self.hitmap = paint_view(
            &tree,
            &mut self.text,
            &mut painter,
            &mut self.hit,
            self.detector.hovered(),
            scale,
        );
        painter.finish_checked()
    }

    fn dispatch(&mut self, action: Action<Msg>) {
        match action {
            Action::Message(msg) => self.effects.extend(self.model.update(msg)),
            Action::DragMove => {
                self.requests.push(WindowRequest::DragMove);
                // 作業系統接手移動迴圈，放開事件不會送達：取消目前的按下狀態。
                let _ = self.detector.cancel();
            }
        }
    }

    fn sync_cursor(&mut self) {
        let next = self.detector.cursor();
        if next != self.cursor {
            self.cursor = next;
            self.requests
                .push(WindowRequest::SetCursor(map_cursor(next)));
        }
    }

    /// 取出尚未執行的視窗副作用（測試用）。
    pub fn pending_effects(&self) -> &[Effect] {
        &self.effects
    }

    fn run_effects(&mut self) {
        if self.effects.is_empty() {
            return;
        }
        if self.hwnd.is_none() {
            self.hwnd = hwnd::find_own_hwnd();
            if let Some(h) = self.hwnd {
                hwnd::set_no_activate(h);
            }
        }
        let effects = std::mem::take(&mut self.effects);
        let Some(h) = self.hwnd else { return };
        for effect in effects {
            match effect {
                Effect::ResizeWindow { expanded } => {
                    let (w, ht) = Model {
                        expanded,
                        ..Model::default()
                    }
                    .window_size();
                    hwnd::set_size(
                        h,
                        hwnd::scaled_px(w, self.scale),
                        hwnd::scaled_px(ht, self.scale),
                    );
                }
                Effect::SetTopmost(on) => {
                    hwnd::set_topmost(h, on);
                }
            }
        }
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

impl Application for ShellApp {
    fn window_config(&self) -> WindowConfig {
        WindowConfig {
            title: "whisper-typeless".into(),
            width: COMPACT_WIDTH as f64,
            height: COMPACT_HEIGHT as f64,
            min_width: Some(COMPACT_WIDTH as f64),
            min_height: Some(COMPACT_HEIGHT as f64),
            decorations: false,
            undecorated_shadow: false,
            ..WindowConfig::default()
        }
    }

    /// 小視窗不需要 GPU；`ACME_RENDER_BACKEND` 仍可覆寫。
    fn render_backend(&self) -> RenderBackendPreference {
        RenderBackendPreference::Cpu
    }

    fn on_waker(&mut self, waker: Waker) {
        let level = Arc::clone(&self.level);
        let stop = Arc::clone(&self.stop);
        std::thread::spawn(move || {
            let mut tick = 0u32;
            while !stop.load(Ordering::Relaxed) {
                level.store(fake_level(tick).to_bits(), Ordering::Relaxed);
                waker.wake();
                tick = tick.wrapping_add(1);
                std::thread::sleep(Duration::from_millis(100));
            }
        });
    }

    fn wake(&mut self) -> bool {
        self.model.level = f32::from_bits(self.level.load(Ordering::Relaxed));
        true
    }

    fn take_window_requests(&mut self, _window: WindowId) -> Vec<WindowRequest> {
        self.run_effects();
        std::mem::take(&mut self.requests)
    }

    fn event(&mut self, event: PlatformEvent) -> bool {
        match event {
            PlatformEvent::PointerMoved { x, y, .. } => {
                let prev = self.detector.hovered();
                let outcomes = self.detector.on_pointer_move(&self.hit, Point::new(x, y));
                self.sync_cursor();
                !outcomes.is_empty() || self.detector.hovered() != prev
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
            } => {
                let point = Point::new(x, y);
                let now = self.started.elapsed().as_secs_f64();
                let outcomes = self
                    .detector
                    .on_pointer_button(&self.hit, button, pressed, point, now);
                let actions: Vec<_> = outcomes
                    .iter()
                    .filter_map(|o| route(&self.hitmap, o))
                    .collect();
                let acted = !actions.is_empty();
                for action in actions {
                    self.dispatch(action);
                }
                if !pressed {
                    let _ = self.detector.on_pointer_move(&self.hit, point);
                }
                self.sync_cursor();
                acted || !outcomes.is_empty()
            }
            PlatformEvent::Resized { .. } => true,
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
                tracing::error!("AcmeUI 外殼建立畫面失敗: {error:?}");
                Scene::new()
            }
        }
    }
}

/// `--acme-shell` 進入點：阻塞直到視窗關閉。
pub fn run_shell() -> Result<(), String> {
    acme_platform::run(ShellApp::new()).map_err(|e| format!("AcmeUI 外殼執行失敗: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use acme_render_cpu::CpuRenderer;

    fn pointer(pressed: bool, x: f32, y: f32) -> PlatformEvent {
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

    fn click(app: &mut ShellApp, x: f32, y: f32) {
        let moved = PlatformEvent::PointerMoved {
            window: WindowId(0),
            x,
            y,
        };
        app.event(moved);
        app.event(pointer(true, x, y));
        app.event(pointer(false, x, y));
    }

    #[test]
    fn compact_and_expanded_scenes_validate() {
        for (w, h, s) in [(380.0, 52.0, 1.0), (380.0, 540.0, 1.0), (380.0, 52.0, 1.5)] {
            let scene = build_scene(w, h, s).expect("scene builds");
            assert!(!scene.commands().is_empty());
            scene.validate().expect("scene valid");
        }
    }

    #[test]
    fn cpu_render_produces_non_background_pixels() {
        let mut app = ShellApp::new();
        let scene = app.build_scene(380.0, 52.0, 1.0).unwrap();
        let mut cpu = CpuRenderer::new(380, 52, 1.0).expect("cpu renderer");
        cpu.render_scene(&scene).expect("render");
        let pixels = cpu.pixels();
        assert_eq!(pixels.len(), 380 * 52);
        let bg = pixels[0];
        let differing = pixels.iter().filter(|p| **p != bg).count();
        assert!(differing > 500, "only {differing} non-background pixels");
    }

    #[test]
    fn click_routes_to_messages_and_effects() {
        let mut app = ShellApp::new();
        app.build_scene(380.0, 52.0, 1.0).unwrap();

        click(&mut app, 26.0, 26.0); // 錄音鍵
        assert!(app.model().recording);

        click(&mut app, 313.0, 26.0); // 釘選
        assert!(app.model().pinned);
        assert_eq!(app.pending_effects(), &[Effect::SetTopmost(true)]);

        click(&mut app, 355.0, 26.0); // 展開
        assert!(app.model().expanded);
        assert_eq!(
            app.pending_effects().last(),
            Some(&Effect::ResizeWindow { expanded: true })
        );
    }

    #[test]
    fn press_on_empty_bar_requests_drag_move() {
        let mut app = ShellApp::new();
        app.build_scene(380.0, 52.0, 1.0).unwrap();
        app.event(PlatformEvent::PointerMoved {
            window: WindowId(0),
            x: 120.0,
            y: 26.0,
        });
        app.event(pointer(true, 120.0, 26.0));
        assert!(app
            .requests
            .iter()
            .any(|req| matches!(req, WindowRequest::DragMove)));
        assert!(!app.model().recording);
    }

    #[test]
    fn fake_level_stays_in_range_and_moves() {
        let values: Vec<f32> = (0..40).map(fake_level).collect();
        assert!(values.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(values.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn wake_pulls_level_into_model() {
        let mut app = ShellApp::new();
        app.level.store(0.5f32.to_bits(), Ordering::Relaxed);
        assert!(app.wake());
        assert_eq!(app.model().level, 0.5);
    }

    #[test]
    fn window_config_is_borderless_compact() {
        let cfg = ShellApp::new().window_config();
        assert!(!cfg.decorations);
        assert_eq!((cfg.width, cfg.height), (380.0, 52.0));
    }
}
