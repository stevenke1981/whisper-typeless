//! AcmeUI 外殼 spike（`--acme-shell`）。
//!
//! 取代 Slint 的候選 UI 外殼：無邊框 380x52 緊湊列，可展開為 380x540。
//! 本模組只是 spike，不接上錄音／轉錄；目的是驗證 AcmeUI 的視窗、文字（zh-TW）、
//! 命中測試與跨執行緒喚醒（Waker）能滿足產品需求。
//!
//! # 架構（給 WP3 的 API 導覽）
//!
//! - [`app`]：`Model`（狀態）、`Msg`（訊息）、`update`（狀態轉移，回傳 `Effect`）、
//!   `view`（狀態 → `View` 樹，純函式）、`ShellApp`（`acme_platform::Application` 實作）。
//! - [`paint`]：`View`/`Item` 描述樹、`paint_view`（描述樹 → `Painter` + `HitTestLayer`）、
//!   `route`（`PointerOutcome` → `Action`，即「命中測試 → 訊息」）。
//! - [`text`]：`TextEngine`（`FontSystem` + CJK 設定的 `GlyphAtlas`），把 zh-TW 文字塑形成 `TextPrimitive`。
//! - [`hwnd`]：以 Win32 取得自己的 HWND，並做不搶焦點的縮放／置頂。

pub mod app;
pub mod hwnd;
pub mod paint;
pub mod text;

pub use app::{build_scene, run_shell, Effect, Model, Msg, ShellApp};
pub use paint::{paint_view, route, Action, HitMap, Item, View};
pub use text::{Align, TextEngine};
