//! # 與 UI 框架無關的核心（core）
//!
//! 這一層擁有 whisper-typeless 的全部應用程式邏輯與畫面狀態，**不依賴任何 UI 框架**
//! （src/core 不得引用 Slint 套件）。任何畫面層（目前的 Slint、未來的 AcmeUI）都只是
//! 一個薄轉接層：
//!
//! ```text
//!   畫面事件 ──UiCommand──▶ Controller::handle ──▶ Backend（背景工作）
//!                                 │                    │
//!                                 ▼                    │ CoreEvent
//!                           AppViewState ◀── Controller::apply ◀── EventQueue::drain
//!                                 │                                  ▲
//!                                 └──▶ 畫面重繪          Notifier::send + wake ┘
//! ```
//!
//! ## 給畫面層作者（AcmeUI view）的 API
//!
//! 1. **建立事件通道**：`let (notifier, queue) = core::event_channel(wake)`。
//!    `wake: Fn() + Send + Sync` 會在任意背景執行緒被呼叫，必須把
//!    「在畫面執行緒呼叫 `queue.drain()`」排入畫面的事件迴圈（Slint 版用
//!    Slint 的 `invoke_from_event_loop`）。同一時間最多排一次喚醒；`CoreEvent::Waveform`
//!    會自動合併為最新一筆。
//! 2. **建立狀態與控制器**（需在 tokio runtime 內）：
//!    ```ignore
//!    let state = core::services::initial_state(&config, &startup_warnings).await;
//!    let mut backend = core::services::RealBackend::new(
//!        config.clone(), config_store, state.selected_model_id.clone(), notifier);
//!    backend.start_background(&state.hotkey, &state.ptt_hotkey, state.ptt_mode);
//!    let mut controller = core::Controller::new(state, config, Box::new(backend));
//!    controller.start(); // 預載模型 / 詢問下載
//!    ```
//! 3. **使用者操作** → `controller.handle(UiCommand::…)`，回傳 [`ViewRequest`]
//!    （調整視窗尺寸 / 隱藏 / 拖曳）由畫面層自行執行。鍵盤快捷鍵可用
//!    [`keys::key_to_command`] 轉成指令（Esc / Space / E）。
//! 4. **背景更新** → 喚醒後 `for ev in queue.drain() { controller.apply(ev) }`，
//!    再把 `controller.state()`（[`AppViewState`]）畫到螢幕上。建議與上一次畫的
//!    狀態做差異比較，只更新有變的部分。
//! 5. 畫面若允許使用者直接編輯狀態欄位（例如雙向綁定的設定值），在送指令或套用事件前
//!    用 `controller.state_mut()` 寫回，讓核心看到最新值。
//!
//! ## 不可倒退的行為
//!
//! - 全域熱鍵（Ctrl+Shift+W）與 PTT（Ctrl+Alt+L）在背景執行緒以 `CoreEvent::HotkeyPressed`
//!   / `CoreEvent::PttKey` 進入核心；畫面層處理它們時**不得**顯示、啟用或聚焦視窗，
//!   辨識結果要落在使用者原本的焦點視窗。
//! - 隱私模式（`privacy_mode`）：不保留錄音檔、輸出後清除剪貼簿。
//! - 預設本機轉錄：語音與文字不離開本機；只有使用者選擇 OpenAI 引擎且 `openai_consent`
//!   為 true 時，[`RecordingRequest::engine`] 才會是 `OpenAi`（由 `Controller` 把關）。
//!   API 金鑰以 [`crate::settings::Secret`] 傳遞（`Debug` 遮蔽），只交給 `Backend::store_api_key`。
//! - 使用者可見字串一律 zh-TW。
//!
//! ## 模組
//!
//! - [`state`]：[`AppViewState`] 與清單列型別。
//! - [`command`]：[`UiCommand`]、[`ViewRequest`]。
//! - [`event`]：[`CoreEvent`]。
//! - [`controller`]：[`Controller`]、[`Notifier`]、[`EventQueue`]、[`event_channel`]。
//! - [`backend`]：副作用介面 [`Backend`]（測試可用假實作）。
//! - [`keys`]：[`keys::key_to_command`]。
//! - [`services`]：正式版 [`services::RealBackend`] 與錄音 / 模型 / 檔案 / 模板 / 熱鍵服務。

pub mod backend;
pub mod command;
pub mod controller;
pub mod event;
pub mod keys;
pub mod services;
pub mod state;

pub use backend::{Backend, FileTranscriptionRequest, RecordingRequest, RecordingStart};
pub use command::{UiCommand, ViewRequest};
pub use controller::{event_channel, Controller, EventQueue, Notifier};
pub use event::{CoreEvent, TrayAction};
pub use state::{AppViewState, ModelEntryView, TemplateEntryView, TranscriptSegment};

#[cfg(test)]
mod tests;
