#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use whisper_typeless::cli;

fn main() -> Result<()> {
    let command = match cli::parse_args(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(e) => {
            cli::attach_parent_console();
            eprintln!("{e}\n\n{}", cli::help_text());
            std::process::exit(2);
        }
    };

    if command == cli::Command::AcmeShell {
        cli::attach_parent_console();
        if let Err(e) = whisper_typeless::ui_acme::run_shell() {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    if command != cli::Command::RunGui {
        cli::attach_parent_console();
        let code = cli::run_headless(command).unwrap_or(0);
        std::process::exit(code);
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_gui())
}

async fn run_gui() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| {
                // Set RUST_LOG=whisper_typeless=debug to see VAD rms values
                "whisper_typeless=info,icu_provider=off,icu_segmenter=off,icu4x=off".into()
            }),
        ))
        .with(tracing_subscriber::fmt::layer())
        // WARN/ERROR → 錯誤紀錄環形緩衝 → UI 錯誤徽章
        .with(whisper_typeless::ui::error_log::layer())
        .init();

    tracing::info!(
        "{} 啟動中 (target: {})",
        whisper_typeless::APP_NAME,
        std::env::consts::ARCH,
    );

    let app_state = whisper_typeless::app::AppState::new().await?;
    whisper_typeless::ui::run(app_state).await?;

    Ok(())
}
