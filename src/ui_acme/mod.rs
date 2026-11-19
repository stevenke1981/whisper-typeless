//! AcmeUI-Native 畫面層（預設 GUI；`--ui slint` 可退回 Slint 版）。
//!
//! 只是核心 [`crate::core`] 的薄轉接層：
//!
//! - [`app`]：[`AcmeApp`]（`acme_platform::Application`），擁有 `Controller`，
//!   把指標 / 鍵盤 / 輸入法事件轉成 `UiCommand`，背景 `CoreEvent` 經 `Waker` 喚醒後套用。
//! - [`views`]：`AppViewState` → 描述樹的純函式（緊湊列、完整視窗、設定、檔案模式、浮層）。
//! - [`paint`]：描述樹 → `Painter` 指令 + `HitTestLayer` 命中區域。
//! - [`controls`]：滑桿 / 步進器數學、acme-textinput 文字框（含輸入法組字）。
//! - [`theme`]：取自 `ui/styles/tokens.slint` 的淺色 / 深色色票。
//! - [`text`]：zh-TW 文字塑形、換行、省略號。
//! - [`hwnd`]：Win32 縮放 / 置頂（一律 `SWP_NOACTIVATE`，不搶焦點）、隱藏 / 顯示（系統匣）。

pub mod app;
pub mod controls;
pub mod hwnd;
pub mod local;
pub mod msg;
pub mod paint;
pub mod text;
pub mod theme;
pub mod views;

use std::sync::{Arc, Mutex};

use tokio::sync::RwLock;

pub use app::{AcmeApp, WakerSlot};
pub use msg::Msg;
pub use paint::{paint_view, Action, HitMap, Item, View};
pub use text::{Align, TextEngine};

use crate::app::AppState;
use crate::core::services::{initial_state, RealBackend};
use crate::core::{event_channel, Controller};

/// 啟動 AcmeUI 介面；阻塞直到視窗關閉。需在 tokio runtime 內（主執行緒）呼叫。
pub async fn run(state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    let (config, config_store, startup_warnings) = {
        let s = state.read().await;
        let c = s.config.read().await.clone();
        (c, Arc::clone(&s.config), s.startup_warnings.clone())
    };
    let view = initial_state(&config, &startup_warnings).await;

    let slot: WakerSlot = Arc::new(Mutex::new(None));
    let (notifier, queue) = event_channel(app::wake_fn(&slot));
    let tray = crate::tray::TrayHandle::spawn(notifier.clone());
    let mut backend = RealBackend::new(
        config.clone(),
        config_store,
        view.selected_model_id.clone(),
        notifier,
    );
    backend.start_background(&view.hotkey, &view.ptt_hotkey, view.ptt_mode);
    let mut controller = Controller::new(view, config, Box::new(backend));
    controller.start();

    let app = AcmeApp::new(controller, queue, slot).with_tray(tray);
    acme_platform::run(app).map_err(|e| anyhow::anyhow!("AcmeUI 執行失敗: {e}"))
}
