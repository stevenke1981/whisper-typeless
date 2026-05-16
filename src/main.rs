#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| {
                // Set RUST_LOG=whisper_typeless=debug to see VAD rms values
                "whisper_typeless=info,icu_provider=off,icu_segmenter=off,icu4x=off".into()
            }),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("whisper-typeless 啟動中...");

    let app_state = whisper_typeless::app::AppState::new().await?;
    whisper_typeless::ui::run(app_state).await?;

    Ok(())
}
