//! 機密字串（API 金鑰等）的包裝型別。
//!
//! [`Secret`] 放在 settings 層，讓 core（[`UiCommand::SetOpenAiKey`](crate::core::UiCommand)）
//! 與 Backend 都能使用而不必互相依賴。它刻意**不實作** `Display`、`Serialize`：
//! `Debug` 一律輸出遮蔽後的文字，避免金鑰經由日誌、`{:?}` 或 config.toml 外流。
//! 需要原文時只能明確呼叫 [`Secret::expose`]。
//!
//! [`KeyStore`]：OpenAI 金鑰以 Windows DPAPI（`CryptProtectData`，綁定目前使用者）加密後存放在
//! 使用者設定資料夾的 `openai.key`；未儲存時改讀環境變數 `OPENAI_API_KEY`。

use std::fmt;

/// 不會在 `Debug` 中顯示內容的字串。
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 取得原文。僅在真正要使用金鑰時呼叫（例如加密寫入金鑰檔、放進連線標頭）。
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

// ─── OpenAI 金鑰儲存 ──────────────────────────────────────────────────────────

/// 未儲存金鑰時改讀的環境變數。
pub const OPENAI_KEY_ENV: &str = "OPENAI_API_KEY";

/// DPAPI 額外熵：讓其他程式即使以同一使用者身分呼叫 `CryptUnprotectData` 也需要知道這個值。
#[cfg(windows)]
const DPAPI_ENTROPY: &[u8] = b"whisper-typeless/openai-api-key/v1";

/// OpenAI 金鑰的加密檔案存放區（Windows DPAPI，綁定目前使用者）。
///
/// 預設位置 `%APPDATA%/whisper-typeless/openai.key`；路徑可注入以便測試。
/// 金鑰不寫入 config.toml，也不寫入日誌。
#[derive(Debug, Clone)]
pub struct KeyStore {
    path: std::path::PathBuf,
}

impl KeyStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `%APPDATA%/whisper-typeless/openai.key`。
    pub fn default_location() -> Option<Self> {
        dirs::config_dir().map(|d| Self::new(d.join("whisper-typeless").join("openai.key")))
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// 是否已有儲存的金鑰檔。
    pub fn is_stored(&self) -> bool {
        self.path.is_file()
    }

    /// 讀取並解密；沒有檔案時回傳 `Ok(None)`。
    pub fn load(&self) -> Result<Option<Secret>, String> {
        let data = match std::fs::read(&self.path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("無法讀取金鑰檔：{e}")),
        };
        let plain = dpapi::unprotect(&data)?;
        let text = String::from_utf8(plain).map_err(|_| "金鑰檔內容損毀".to_string())?;
        let key = Secret::new(text.trim());
        Ok((!key.is_empty()).then_some(key))
    }

    /// 加密後寫入（覆寫舊檔）。
    pub fn store(&self, key: &Secret) -> Result<(), String> {
        let trimmed = key.expose().trim();
        if trimmed.is_empty() {
            return Err("金鑰不可為空白".into());
        }
        let data = dpapi::protect(trimmed.as_bytes())?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("無法建立設定資料夾：{e}"))?;
        }
        let tmp = self.path.with_extension("key.tmp");
        std::fs::write(&tmp, &data).map_err(|e| format!("無法寫入金鑰檔：{e}"))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("無法寫入金鑰檔：{e}")
        })
    }

    /// 刪除已儲存的金鑰（不存在時視為成功）。
    pub fn clear(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("無法刪除金鑰檔：{e}")),
        }
    }
}

/// 金鑰解析順序：已儲存的金鑰優先，其次環境變數 [`OPENAI_KEY_ENV`]。
pub fn resolve_key(stored: Option<Secret>, env: Option<String>) -> Option<Secret> {
    stored
        .filter(|k| !k.is_empty())
        .or_else(|| env.map(|v| Secret::new(v.trim())).filter(|k| !k.is_empty()))
}

/// 從存放區（可為 `None`）與環境變數解析 OpenAI 金鑰。讀取失敗只記錄錯誤類型，不含金鑰。
pub fn resolve_openai_key(store: Option<&KeyStore>) -> Option<Secret> {
    let stored = store.and_then(|s| match s.load() {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!("OpenAI 金鑰讀取失敗：{e}");
            None
        }
    });
    resolve_key(stored, std::env::var(OPENAI_KEY_ENV).ok())
}

#[cfg(windows)]
mod dpapi {
    use std::ptr::null_mut;
    use winapi::um::dpapi::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN};
    use winapi::um::winbase::LocalFree;
    use winapi::um::wincrypt::DATA_BLOB;

    fn blob(data: &[u8]) -> DATA_BLOB {
        DATA_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    /// 取出輸出 blob 並以 `LocalFree` 釋放。
    unsafe fn take(out: DATA_BLOB) -> Vec<u8> {
        if out.pbData.is_null() {
            return Vec::new();
        }
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        v
    }

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        let mut input = blob(plain);
        let mut entropy = blob(super::DPAPI_ENTROPY);
        let mut out = DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        // SAFETY: 輸入 blob 指向存活中的切片；輸出由系統配置，take() 負責釋放。
        let ok = unsafe {
            CryptProtectData(
                &mut input,
                std::ptr::null(),
                &mut entropy,
                null_mut(),
                null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(format!(
                "金鑰加密失敗（{}）",
                std::io::Error::last_os_error()
            ));
        }
        Ok(unsafe { take(out) })
    }

    pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>, String> {
        let mut input = blob(cipher);
        let mut entropy = blob(super::DPAPI_ENTROPY);
        let mut out = DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        // SAFETY: 同 protect。
        let ok = unsafe {
            CryptUnprotectData(
                &mut input,
                null_mut(),
                &mut entropy,
                null_mut(),
                null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(
                "金鑰解密失敗（可能是其他使用者或其他電腦建立的金鑰檔），請重新輸入金鑰".into(),
            );
        }
        Ok(unsafe { take(out) })
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(_plain: &[u8]) -> Result<Vec<u8>, String> {
        Err("此平台不支援加密儲存金鑰，請改用環境變數 OPENAI_API_KEY".into())
    }

    pub fn unprotect(_cipher: &[u8]) -> Result<Vec<u8>, String> {
        Err("此平台不支援加密儲存金鑰，請改用環境變數 OPENAI_API_KEY".into())
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

    #[test]
    fn resolve_prefers_stored_then_env() {
        let stored = Some(Secret::new("sk-stored"));
        assert_eq!(
            resolve_key(stored.clone(), Some("sk-env".into()))
                .unwrap()
                .expose(),
            "sk-stored"
        );
        assert_eq!(
            resolve_key(None, Some(" sk-env \n".into()))
                .unwrap()
                .expose(),
            "sk-env"
        );
        assert_eq!(
            resolve_key(Some(Secret::new("")), Some("sk-env".into()))
                .unwrap()
                .expose(),
            "sk-env"
        );
        assert!(resolve_key(None, Some("   ".into())).is_none());
        assert!(resolve_key(None, None).is_none());
    }

    #[test]
    fn missing_key_file_is_none() {
        let dir = crate::settings::test_util::TempDir::new("keystore-missing");
        let store = KeyStore::new(dir.path().join("openai.key"));
        assert!(!store.is_stored());
        assert!(store.load().unwrap().is_none());
        assert!(store.clear().is_ok());
        assert!(store.store(&Secret::new("  ")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_round_trip_in_temp_dir() {
        let dir = crate::settings::test_util::TempDir::new("keystore-dpapi");
        let store = KeyStore::new(dir.path().join("sub").join("openai.key"));
        store.store(&Secret::new(" sk-test-123 \n")).unwrap();
        assert!(store.is_stored());
        // 檔案內容是密文，不含原文
        let raw = std::fs::read(store.path()).unwrap();
        assert!(!raw.windows(11).any(|w| w == b"sk-test-123"));
        assert_eq!(store.load().unwrap().unwrap().expose(), "sk-test-123");
        // 覆寫
        store.store(&Secret::new("sk-second")).unwrap();
        assert_eq!(store.load().unwrap().unwrap().expose(), "sk-second");
        store.clear().unwrap();
        assert!(!store.is_stored());
        assert!(store.load().unwrap().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn corrupted_key_file_is_error() {
        let dir = crate::settings::test_util::TempDir::new("keystore-corrupt");
        let store = KeyStore::new(dir.path().join("openai.key"));
        std::fs::write(store.path(), b"not dpapi").unwrap();
        assert!(store.load().is_err());
    }
}
