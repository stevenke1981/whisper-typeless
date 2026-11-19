use enigo::{Enigo, Key, Keyboard, Settings};
use tracing::debug;

/// 鍵盤注入副作用抽象，讓 [`super::OutputRouter`] 可用 mock 單元測試。
pub trait KeyboardSink: Send {
    /// 直接以模擬鍵入輸出文字（不經剪貼簿）
    fn type_text(&mut self, text: &str) -> anyhow::Result<()>;
    /// 送出貼上快捷鍵（Windows/Linux: Ctrl+V，macOS: Cmd+V）
    fn paste_shortcut(&mut self) -> anyhow::Result<()>;
}

/// 系統鍵盤注入（enigo）。每次操作才建立 `Enigo`，與既有即時錄音路徑一致。
#[derive(Debug, Default, Clone, Copy)]
pub struct FocusedWindowInjector;

impl FocusedWindowInjector {
    pub fn new() -> Self {
        Self
    }
}

impl KeyboardSink for FocusedWindowInjector {
    fn type_text(&mut self, text: &str) -> anyhow::Result<()> {
        let mut enigo = Enigo::new(&Settings::default())?;
        enigo.text(text)?;
        Ok(())
    }

    fn paste_shortcut(&mut self) -> anyhow::Result<()> {
        let mut enigo = Enigo::new(&Settings::default())?;

        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;

        enigo.key(modifier, enigo::Direction::Press)?;
        let click = enigo.key(Key::Unicode('v'), enigo::Direction::Click);
        // 無論 v 是否成功，都要放開修飾鍵，避免卡住 Ctrl
        enigo.key(modifier, enigo::Direction::Release)?;
        click?;

        debug!("已發送貼上快捷鍵");
        Ok(())
    }
}
