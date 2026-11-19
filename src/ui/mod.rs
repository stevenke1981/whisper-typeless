mod bridge;
pub mod error_log;
pub mod window_icon;

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::app::AppState;

pub async fn run(state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    bridge::launch(state).await
}

/// 簡化 slint::invoke_from_event_loop + Weak::upgrade 樣板。
///
/// 用法：
/// ```ignore
/// ui_update!(ui_weak, |ui| {
///     ui.set_status_text("就緒".into());
/// });
/// ```
#[macro_export]
macro_rules! ui_update {
    ($weak:expr, $block:expr) => {
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = $weak.upgrade() {
                $block(ui);
            }
        });
    };
}
