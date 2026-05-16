use enigo::{Enigo, Key, Keyboard, Settings};
use tracing::debug;

pub struct FocusedWindowInjector {
    inject_delay_ms: u64,
}

impl FocusedWindowInjector {
    pub fn new() -> Self {
        Self {
            inject_delay_ms: 50,
        }
    }

    pub fn with_delay(delay_ms: u64) -> Self {
        Self {
            inject_delay_ms: delay_ms,
        }
    }

    /// 直接輸入文字（不走剪貼簿）
    pub async fn inject(&self, text: &str) -> anyhow::Result<()> {
        let text = text.to_string();
        let delay = self.inject_delay_ms;

        tokio::task::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            let mut enigo = Enigo::new(&Settings::default())?;
            enigo.text(&text)?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    /// 使用剪貼簿 + Ctrl/Cmd+V 貼上
    pub async fn inject_via_clipboard(&self) -> anyhow::Result<()> {
        let delay = self.inject_delay_ms;

        tokio::task::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay));

            let mut enigo = Enigo::new(&Settings::default())?;

            #[cfg(target_os = "macos")]
            {
                enigo.key(Key::Meta, enigo::Direction::Press)?;
                enigo.key(Key::Unicode('v'), enigo::Direction::Click)?;
                enigo.key(Key::Meta, enigo::Direction::Release)?;
            }

            #[cfg(not(target_os = "macos"))]
            {
                enigo.key(Key::Control, enigo::Direction::Press)?;
                enigo.key(Key::Unicode('v'), enigo::Direction::Click)?;
                enigo.key(Key::Control, enigo::Direction::Release)?;
            }

            debug!("已發送貼上快捷鍵");
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    pub fn set_delay(&mut self, delay_ms: u64) {
        self.inject_delay_ms = delay_ms;
    }
}

impl Default for FocusedWindowInjector {
    fn default() -> Self {
        Self::new()
    }
}
