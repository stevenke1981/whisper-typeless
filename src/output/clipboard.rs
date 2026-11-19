use arboard::Clipboard;

/// 剪貼簿副作用抽象，讓 [`super::OutputRouter`] 可用 mock 單元測試。
pub trait ClipboardSink: Send {
    fn set_text(&mut self, text: &str) -> anyhow::Result<()>;
    fn clear(&mut self) -> anyhow::Result<()>;
}

/// 系統剪貼簿（arboard）。每次操作才建立 `Clipboard`，
/// 與既有即時錄音路徑行為一致，也避免長期持有剪貼簿控制權。
#[derive(Debug, Default, Clone, Copy)]
pub struct ClipboardOutput;

impl ClipboardOutput {
    pub fn new() -> Self {
        Self
    }

    pub fn write(&mut self, text: &str) -> anyhow::Result<()> {
        Clipboard::new()?.set_text(text)?;
        Ok(())
    }

    pub fn read(&mut self) -> anyhow::Result<String> {
        Ok(Clipboard::new()?.get_text()?)
    }

    pub fn clear(&mut self) -> anyhow::Result<()> {
        Clipboard::new()?.clear()?;
        Ok(())
    }
}

impl ClipboardSink for ClipboardOutput {
    fn set_text(&mut self, text: &str) -> anyhow::Result<()> {
        self.write(text)
    }

    fn clear(&mut self) -> anyhow::Result<()> {
        ClipboardOutput::clear(self)
    }
}
