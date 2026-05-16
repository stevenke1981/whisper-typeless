use arboard::Clipboard;

pub struct ClipboardOutput {
    inner: Clipboard,
}

impl ClipboardOutput {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            inner: Clipboard::new()?,
        })
    }

    pub fn write(&mut self, text: &str) -> anyhow::Result<()> {
        self.inner.set_text(text)?;
        Ok(())
    }

    pub fn read(&mut self) -> anyhow::Result<String> {
        Ok(self.inner.get_text()?)
    }

    pub fn clear(&mut self) -> anyhow::Result<()> {
        self.inner.clear()?;
        Ok(())
    }
}
