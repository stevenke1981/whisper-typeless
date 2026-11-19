pub mod clipboard;
pub mod formatter;
pub mod injector;

pub use clipboard::{ClipboardOutput, ClipboardSink};
pub use formatter::{FormatterConfig, OutputFormatter};
pub use injector::{FocusedWindowInjector, KeyboardSink};

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum OutputMode {
    ClipboardOnly,
    InjectToFocused,
    #[default]
    ClipboardAndInject,
    FileAppend(PathBuf),
    None,
}

impl OutputMode {
    /// 由 UI 的「剪貼簿」「注入」兩個開關推導輸出模式
    pub fn from_flags(clipboard: bool, inject: bool) -> Self {
        match (clipboard, inject) {
            (true, true) => Self::ClipboardAndInject,
            (true, false) => Self::ClipboardOnly,
            (false, true) => Self::InjectToFocused,
            (false, false) => Self::None,
        }
    }
}

/// `ClipboardAndInject` 模式下文字如何送進焦點視窗
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InjectMethod {
    /// 寫入剪貼簿後送出 Ctrl/Cmd+V
    #[default]
    Paste,
    /// 寫入剪貼簿，並以模擬鍵入（enigo.text）輸出文字——即時錄音路徑的既有行為
    Type,
}

/// 隱私模式下，用貼上方式輸出後清除剪貼簿前的等待時間，
/// 讓目標程式有時間讀取剪貼簿內容。
const PRIVACY_CLEAR_AFTER_PASTE_MS: u64 = 150;

/// 預設注入前延遲（讓焦點回到目標視窗）
const DEFAULT_INJECT_DELAY_MS: u64 = 50;

/// 將最終文字依 [`OutputMode`] 送往剪貼簿 / 焦點視窗 / 檔案。
///
/// 副作用都藏在 [`ClipboardSink`] / [`KeyboardSink`] 之後，可用 mock 測試。
/// 隱私模式的剪貼簿清除也在此處理。
pub struct OutputRouter {
    clipboard: Box<dyn ClipboardSink>,
    keyboard: Box<dyn KeyboardSink>,
    formatter: OutputFormatter,
    mode: OutputMode,
    inject_method: InjectMethod,
    privacy_mode: bool,
    inject_delay: Duration,
}

impl OutputRouter {
    /// 使用系統剪貼簿（arboard）與鍵盤注入（enigo）
    pub fn new(mode: OutputMode, formatter: OutputFormatter) -> Self {
        Self::with_sinks(
            mode,
            formatter,
            Box::new(ClipboardOutput::new()),
            Box::new(FocusedWindowInjector::new()),
        )
    }

    /// 指定自訂 sink（測試用 mock）
    pub fn with_sinks(
        mode: OutputMode,
        formatter: OutputFormatter,
        clipboard: Box<dyn ClipboardSink>,
        keyboard: Box<dyn KeyboardSink>,
    ) -> Self {
        Self {
            clipboard,
            keyboard,
            formatter,
            mode,
            inject_method: InjectMethod::default(),
            privacy_mode: false,
            inject_delay: Duration::from_millis(DEFAULT_INJECT_DELAY_MS),
        }
    }

    pub fn with_inject_method(mut self, method: InjectMethod) -> Self {
        self.inject_method = method;
        self
    }

    pub fn with_privacy_mode(mut self, privacy_mode: bool) -> Self {
        self.privacy_mode = privacy_mode;
        self
    }

    pub fn with_inject_delay_ms(mut self, delay_ms: u64) -> Self {
        self.inject_delay = Duration::from_millis(delay_ms);
        self
    }

    pub fn mode(&self) -> &OutputMode {
        &self.mode
    }

    pub fn set_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }

    /// 同步送出（會阻塞：注入延遲與鍵盤模擬）。請在背景執行緒呼叫。
    ///
    /// 每個步驟都會嘗試執行（例如剪貼簿失敗仍會嘗試注入），
    /// 回傳第一個遇到的錯誤。
    pub fn send(&mut self, text: &str) -> anyhow::Result<()> {
        let formatted = self.formatter.format(text);
        let mut first_err: Option<anyhow::Error> = None;
        let mut record = |res: anyhow::Result<()>, what: &str| -> bool {
            match res {
                Ok(()) => true,
                Err(e) => {
                    warn!("輸出失敗（{what}）: {e}");
                    first_err.get_or_insert(e);
                    false
                }
            }
        };

        let mut wrote_clipboard = false;
        let mut pasted = false;

        match &self.mode {
            OutputMode::ClipboardOnly => {
                wrote_clipboard = record(self.clipboard.set_text(&formatted), "剪貼簿");
            }
            OutputMode::InjectToFocused => {
                self.wait_inject_delay();
                record(self.keyboard.type_text(&formatted), "鍵入");
            }
            OutputMode::ClipboardAndInject => {
                wrote_clipboard = record(self.clipboard.set_text(&formatted), "剪貼簿");
                self.wait_inject_delay();
                match self.inject_method {
                    // 剪貼簿寫入失敗時改用鍵入，避免貼上舊內容
                    InjectMethod::Paste if wrote_clipboard => {
                        pasted = record(self.keyboard.paste_shortcut(), "貼上");
                    }
                    InjectMethod::Paste | InjectMethod::Type => {
                        record(self.keyboard.type_text(&formatted), "鍵入");
                    }
                }
            }
            OutputMode::FileAppend(path) => {
                let res = std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path)
                    .and_then(|mut f| f.write_all(formatted.as_bytes()))
                    .map_err(anyhow::Error::from);
                record(res, "寫入檔案");
            }
            OutputMode::None => {}
        }

        // 隱私模式：輸出後清除剪貼簿，不留下轉錄文字
        if self.privacy_mode && wrote_clipboard {
            if pasted {
                std::thread::sleep(Duration::from_millis(PRIVACY_CLEAR_AFTER_PASTE_MS));
            }
            record(self.clipboard.clear(), "清除剪貼簿");
        }

        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn wait_inject_delay(&self) {
        if !self.inject_delay.is_zero() {
            std::thread::sleep(self.inject_delay);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq)]
    enum Ev {
        SetClipboard(String),
        ClearClipboard,
        Type(String),
        Paste,
    }

    type Log = Arc<Mutex<Vec<Ev>>>;

    struct MockClipboard {
        log: Log,
        fail_set: bool,
    }

    impl ClipboardSink for MockClipboard {
        fn set_text(&mut self, text: &str) -> anyhow::Result<()> {
            if self.fail_set {
                anyhow::bail!("clipboard busy");
            }
            self.log.lock().unwrap().push(Ev::SetClipboard(text.into()));
            Ok(())
        }
        fn clear(&mut self) -> anyhow::Result<()> {
            self.log.lock().unwrap().push(Ev::ClearClipboard);
            Ok(())
        }
    }

    struct MockKeyboard {
        log: Log,
    }

    impl KeyboardSink for MockKeyboard {
        fn type_text(&mut self, text: &str) -> anyhow::Result<()> {
            self.log.lock().unwrap().push(Ev::Type(text.into()));
            Ok(())
        }
        fn paste_shortcut(&mut self) -> anyhow::Result<()> {
            self.log.lock().unwrap().push(Ev::Paste);
            Ok(())
        }
    }

    fn router_with(mode: OutputMode, fail_set: bool) -> (OutputRouter, Log) {
        let log: Log = Arc::default();
        let router = OutputRouter::with_sinks(
            mode,
            OutputFormatter::passthrough(),
            Box::new(MockClipboard {
                log: log.clone(),
                fail_set,
            }),
            Box::new(MockKeyboard { log: log.clone() }),
        )
        .with_inject_delay_ms(0);
        (router, log)
    }

    fn events(log: &Log) -> Vec<Ev> {
        log.lock().unwrap().clone()
    }

    #[test]
    fn from_flags_maps_all_combinations() {
        assert_eq!(
            OutputMode::from_flags(true, true),
            OutputMode::ClipboardAndInject
        );
        assert_eq!(
            OutputMode::from_flags(true, false),
            OutputMode::ClipboardOnly
        );
        assert_eq!(
            OutputMode::from_flags(false, true),
            OutputMode::InjectToFocused
        );
        assert_eq!(OutputMode::from_flags(false, false), OutputMode::None);
    }

    #[test]
    fn clipboard_only_writes_clipboard() {
        let (mut r, log) = router_with(OutputMode::ClipboardOnly, false);
        r.send("你好").unwrap();
        assert_eq!(events(&log), vec![Ev::SetClipboard("你好".into())]);
    }

    #[test]
    fn inject_to_focused_types_text() {
        let (mut r, log) = router_with(OutputMode::InjectToFocused, false);
        r.send("你好").unwrap();
        assert_eq!(events(&log), vec![Ev::Type("你好".into())]);
    }

    #[test]
    fn clipboard_and_inject_paste_method() {
        let (mut r, log) = router_with(OutputMode::ClipboardAndInject, false);
        r.send("abc").unwrap();
        assert_eq!(
            events(&log),
            vec![Ev::SetClipboard("abc".into()), Ev::Paste]
        );
    }

    #[test]
    fn clipboard_and_inject_type_method_matches_live_path() {
        let (r, log) = router_with(OutputMode::ClipboardAndInject, false);
        let mut r = r.with_inject_method(InjectMethod::Type);
        r.send("abc").unwrap();
        assert_eq!(
            events(&log),
            vec![Ev::SetClipboard("abc".into()), Ev::Type("abc".into())]
        );
    }

    #[test]
    fn paste_falls_back_to_typing_when_clipboard_fails() {
        let (mut r, log) = router_with(OutputMode::ClipboardAndInject, true);
        let res = r.send("abc");
        assert!(res.is_err(), "clipboard error should be reported");
        assert_eq!(events(&log), vec![Ev::Type("abc".into())]);
    }

    #[test]
    fn none_mode_has_no_side_effects() {
        let (r, log) = router_with(OutputMode::None, false);
        let mut r = r.with_privacy_mode(true);
        r.send("abc").unwrap();
        assert!(events(&log).is_empty());
    }

    #[test]
    fn file_append_writes_file() {
        let path = std::env::temp_dir().join(format!(
            "whisper-typeless-router-test-{}.txt",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let (mut r, log) = router_with(OutputMode::FileAppend(path.clone()), false);
        r.send("一").unwrap();
        r.send("二").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(content, "一二");
        assert!(events(&log).is_empty());
    }

    #[test]
    fn formatter_is_applied() {
        let log: Log = Arc::default();
        let mut r = OutputRouter::with_sinks(
            OutputMode::ClipboardOnly,
            OutputFormatter::new(FormatterConfig::default()),
            Box::new(MockClipboard {
                log: log.clone(),
                fail_set: false,
            }),
            Box::new(MockKeyboard { log: log.clone() }),
        );
        r.send("x").unwrap();
        assert_eq!(events(&log), vec![Ev::SetClipboard("x\n".into())]);
    }

    #[test]
    fn privacy_clears_clipboard_after_type() {
        let (r, log) = router_with(OutputMode::ClipboardAndInject, false);
        let mut r = r
            .with_inject_method(InjectMethod::Type)
            .with_privacy_mode(true);
        r.send("秘密").unwrap();
        assert_eq!(
            events(&log),
            vec![
                Ev::SetClipboard("秘密".into()),
                Ev::Type("秘密".into()),
                Ev::ClearClipboard
            ]
        );
    }

    #[test]
    fn privacy_clears_clipboard_after_paste_and_clipboard_only() {
        let (r, log) = router_with(OutputMode::ClipboardAndInject, false);
        let mut r = r.with_privacy_mode(true);
        r.send("a").unwrap();
        assert_eq!(
            events(&log),
            vec![Ev::SetClipboard("a".into()), Ev::Paste, Ev::ClearClipboard]
        );

        let (r, log) = router_with(OutputMode::ClipboardOnly, false);
        let mut r = r.with_privacy_mode(true);
        r.send("b").unwrap();
        assert_eq!(
            events(&log),
            vec![Ev::SetClipboard("b".into()), Ev::ClearClipboard]
        );
    }

    #[test]
    fn privacy_does_not_touch_clipboard_when_only_typing() {
        let (r, log) = router_with(OutputMode::InjectToFocused, false);
        let mut r = r.with_privacy_mode(true);
        r.send("a").unwrap();
        assert_eq!(events(&log), vec![Ev::Type("a".into())]);
    }

    #[test]
    fn no_clear_without_privacy_mode() {
        let (mut r, log) = router_with(OutputMode::ClipboardOnly, false);
        r.send("a").unwrap();
        assert!(!events(&log).contains(&Ev::ClearClipboard));
    }
}
