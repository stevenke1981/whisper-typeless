mod bridge;
pub mod window_icon;

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::app::AppState;

pub async fn run(state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    bridge::launch(state).await
}
