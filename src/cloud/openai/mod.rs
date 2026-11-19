//! OpenAI Realtime 轉錄（`wss://api.openai.com/v1/realtime?intent=transcription`）。
//!
//! - [`protocol`]：純函式——`session.update` / 音訊封包編碼、伺服器事件解析、依 commit 順序釋出結果的重排緩衝。
//! - [`session`]：與傳輸層無關的工作階段邏輯（送音訊、commit、收結果、停止時的有限等待）。
//! - [`transport`]：[`Transport`] trait 與真正的 WebSocket 實作（薄轉接層，測試不使用）。
//!
//! 隱私：API 金鑰以 [`Secret`](crate::settings::Secret) 傳入，只放在 `Authorization` 標頭，
//! 不寫入日誌；轉錄文字也不寫入日誌。

pub mod protocol;
pub mod session;
pub mod transport;

pub use protocol::SessionConfig;
pub use session::{CloudSession, SessionOutput};
pub use transport::{Transport, WsTransport};
