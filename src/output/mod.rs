pub mod clipboard;
pub mod formatter;
pub mod injector;

pub use clipboard::ClipboardOutput;
pub use formatter::OutputFormatter;
pub use injector::FocusedWindowInjector;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum OutputMode {
    ClipboardOnly,
    InjectToFocused,
    #[default]
    ClipboardAndInject,
    FileAppend(PathBuf),
    None,
}

pub struct OutputRouter {
    clipboard: ClipboardOutput,
    injector: FocusedWindowInjector,
    formatter: OutputFormatter,
    mode: OutputMode,
}

impl OutputRouter {
    pub fn new(mode: OutputMode, formatter: OutputFormatter) -> anyhow::Result<Self> {
        Ok(Self {
            clipboard: ClipboardOutput::new()?,
            injector: FocusedWindowInjector::new(),
            formatter,
            mode,
        })
    }

    pub async fn send(&mut self, text: &str) -> anyhow::Result<()> {
        let formatted = self.formatter.format(text);

        match &self.mode {
            OutputMode::ClipboardOnly => {
                self.clipboard.write(&formatted)?;
            }
            OutputMode::InjectToFocused => {
                self.injector.inject(&formatted).await?;
            }
            OutputMode::ClipboardAndInject => {
                self.clipboard.write(&formatted)?;
                self.injector.inject_via_clipboard().await?;
            }
            OutputMode::FileAppend(path) => {
                let path = path.clone();
                let mut file = tokio::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&path)
                    .await?;
                file.write_all(formatted.as_bytes()).await?;
            }
            OutputMode::None => {}
        }

        Ok(())
    }

    pub fn set_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }
}
