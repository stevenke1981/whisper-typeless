/// 編譯期注入的 crate 版本號（來自 Cargo.toml）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 完整的應用程式顯示名稱（含版本）。
pub const APP_NAME: &str = concat!("whisper-typeless v", env!("CARGO_PKG_VERSION"));

pub mod app;
pub mod audio;
pub mod context_templates;
pub mod file_transcription;
pub mod models;
pub mod output;
pub mod settings;
pub mod transcription;
pub mod ui;
pub mod whisper;

slint::include_modules!();
