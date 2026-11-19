//! 機密字串（API 金鑰等）的包裝型別。
//!
//! [`Secret`] 放在 settings 層，讓 core（[`UiCommand::SetOpenAiKey`](crate::core::UiCommand)）
//! 與 Backend 都能使用而不必互相依賴。它刻意**不實作** `Display`、`Serialize`：
//! `Debug` 一律輸出遮蔽後的文字，避免金鑰經由日誌、`{:?}` 或 config.toml 外流。
//! 需要原文時只能明確呼叫 [`Secret::expose`]。

use std::fmt;

/// 不會在 `Debug` 中顯示內容的字串。
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 取得原文。僅在真正要使用金鑰時呼叫（例如寫入憑證存放區）。
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Secret::new("sk-very-secret-123");
        let dbg = format!("{s:?}");
        assert_eq!(dbg, "Secret(***)");
        assert!(!dbg.contains("sk-very"));
        // 包在其他型別裡（例如 UiCommand）也不會外流
        let nested = format!("{:?}", Some(s.clone()));
        assert!(!nested.contains("secret-123"));
        assert_eq!(s.expose(), "sk-very-secret-123");
    }
}
