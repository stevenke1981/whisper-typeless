//! 命令列介面（手寫解析，不引入額外依賴）。
//!
//! - 無參數：啟動 GUI
//! - `--version` / `-V`、`--help` / `-h` / `/?`
//! - `--config-path`：印出設定檔路徑
//! - `--self-check`：無介面自我檢查（模型 catalog、範本、設定檔、OpenCC），通過 exit 0，否則 exit 1

use std::path::PathBuf;

use crate::context_templates::{TemplateId, TemplateRegistry};
use crate::models::ModelRegistry;
use crate::settings::AppConfig;
use crate::transcription::opencc::{ConversionMode, OpenCCProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    RunGui,
    Version,
    Help,
    ConfigPath,
    SelfCheck,
}

/// 參數解析錯誤（訊息為 zh-TW）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// 解析參數（不含程式名稱，即 `std::env::args().skip(1)`）
pub fn parse_args<I, S>(args: I) -> Result<Command, ParseError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string()).collect();
    match args.as_slice() {
        [] => Ok(Command::RunGui),
        [one] => match one.as_str() {
            "--version" | "-V" => Ok(Command::Version),
            "--help" | "-h" | "/?" => Ok(Command::Help),
            "--config-path" => Ok(Command::ConfigPath),
            "--self-check" => Ok(Command::SelfCheck),
            other => Err(ParseError(format!("未知的參數: {other}"))),
        },
        _ => Err(ParseError(format!(
            "一次只能指定一個參數（收到 {} 個）",
            args.len()
        ))),
    }
}

pub fn version_text() -> String {
    crate::APP_NAME.to_string()
}

pub fn help_text() -> String {
    format!(
        "{name}
本機語音轉文字：說話 → 文字直接輸入目前焦點視窗。

用法:
  whisper-typeless [選項]

選項:
  (無)             啟動圖形介面
  -h, --help       顯示此說明
  -V, --version    顯示版本
  --config-path    印出設定檔路徑
  --self-check     不開視窗執行自我檢查（模型清單、範本、設定檔、OpenCC），
                   全部通過時結束碼為 0，否則為 1",
        name = crate::APP_NAME
    )
}

/// 設定檔路徑（與 `AppConfig::load_or_default` 使用相同位置）
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("whisper-typeless").join("config.toml"))
}

// ─── 自我檢查 ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckItem {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

impl CheckItem {
    fn new(name: &'static str, result: Result<String, String>) -> Self {
        match result {
            Ok(detail) => Self {
                name,
                ok: true,
                detail,
            },
            Err(detail) => Self {
                name,
                ok: false,
                detail,
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelfCheckReport {
    pub items: Vec<CheckItem>,
}

impl SelfCheckReport {
    pub fn all_ok(&self) -> bool {
        self.items.iter().all(|i| i.ok)
    }

    pub fn exit_code(&self) -> i32 {
        if self.all_ok() {
            0
        } else {
            1
        }
    }

    pub fn render(&self) -> String {
        let mut out = format!("{} 自我檢查\n", crate::APP_NAME);
        for item in &self.items {
            let mark = if item.ok { "OK  " } else { "FAIL" };
            out.push_str(&format!("[{mark}] {}: {}\n", item.name, item.detail));
        }
        let passed = self.items.iter().filter(|i| i.ok).count();
        out.push_str(&format!(
            "結果: {passed}/{} 通過{}",
            self.items.len(),
            if self.all_ok() {
                ""
            } else {
                "（有失敗項目）"
            }
        ));
        out
    }
}

const EMBEDDED_MODEL_CATALOG: &str = include_str!("../model-catalog.json");

/// 驗證模型 catalog JSON：至少一個模型、id 不重複、下載網址為 https、checksum 為 40/64 位十六進位。
pub fn check_model_catalog(source: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|e| format!("JSON 解析失敗: {e}"))?;
    let models = value
        .get("models")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "缺少 models 陣列".to_string())?;
    if models.is_empty() {
        return Err("models 為空".into());
    }

    let mut ids = std::collections::HashSet::new();
    for (i, m) in models.iter().enumerate() {
        let id = m
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("第 {i} 個模型缺少 id"))?;
        if !ids.insert(id.to_string()) {
            return Err(format!("模型 id 重複: {id}"));
        }
        let url = m.get("download_url").and_then(|v| v.as_str()).unwrap_or("");
        if !url.starts_with("https://") {
            return Err(format!("{id}: download_url 必須為 https"));
        }
        let sum = m
            .get("sha256")
            .or_else(|| m.get("sha1"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let valid_sum = matches!(sum.len(), 40 | 64) && sum.chars().all(|c| c.is_ascii_hexdigit());
        if !valid_sum {
            return Err(format!("{id}: checksum 格式錯誤"));
        }
    }
    Ok(format!("{} 個模型", models.len()))
}

/// 內建情境範本：五個內建範本皆存在
pub fn check_templates() -> Result<String, String> {
    let registry = TemplateRegistry::new();
    let expected = [
        TemplateId::Meeting,
        TemplateId::Casual,
        TemplateId::Technical,
        TemplateId::Medical,
        TemplateId::Legal,
    ];
    let missing: Vec<&str> = expected
        .iter()
        .filter(|id| registry.get(id).is_none())
        .map(|id| id.display_name())
        .collect();
    if missing.is_empty() {
        Ok(format!("{} 個內建範本", registry.all().len()))
    } else {
        Err(format!("缺少範本: {}", missing.join("、")))
    }
}

/// 解析設定檔內容
pub fn check_config_source(source: &str) -> Result<String, String> {
    toml::from_str::<AppConfig>(source)
        .map(|_| "設定檔解析成功".to_string())
        .map_err(|e| format!("設定檔解析失敗: {e}"))
}

fn check_config_file() -> Result<String, String> {
    let path = config_path().ok_or_else(|| "無法取得設定目錄".to_string())?;
    if !path.exists() {
        return Ok(format!("{}（尚未建立，將使用預設值）", path.display()));
    }
    let source =
        std::fs::read_to_string(&path).map_err(|e| format!("{}: 讀取失敗: {e}", path.display()))?;
    check_config_source(&source).map(|msg| format!("{} — {msg}", path.display()))
}

/// 預設設定可序列化再解析回來
pub fn check_default_config() -> Result<String, String> {
    let text = toml::to_string_pretty(&AppConfig::default())
        .map_err(|e| format!("預設設定序列化失敗: {e}"))?;
    check_config_source(&text).map(|_| "預設設定往返序列化成功".to_string())
}

/// OpenCC 內建字典可載入並轉換（zh-TW 預設）
pub fn check_opencc() -> Result<String, String> {
    let proc = OpenCCProcessor::new(ConversionMode::ZhTW)
        .map_err(|e| format!("OpenCC 初始化失敗: {e}"))?;
    let out = proc.convert("软件");
    if out == "軟體" {
        Ok("zh-TW 轉換正常（软件 → 軟體）".into())
    } else {
        Err(format!("zh-TW 轉換結果異常: {out}"))
    }
}

pub fn run_self_check() -> SelfCheckReport {
    let effective = ModelRegistry::all();
    let effective_result = if effective.is_empty() {
        Err("沒有可用的模型".to_string())
    } else {
        Ok(format!("{} 個模型可用", effective.len()))
    };

    SelfCheckReport {
        items: vec![
            CheckItem::new("內建模型清單", check_model_catalog(EMBEDDED_MODEL_CATALOG)),
            CheckItem::new("實際使用的模型清單", effective_result),
            CheckItem::new("情境範本", check_templates()),
            CheckItem::new("預設設定", check_default_config()),
            CheckItem::new("設定檔", check_config_file()),
            CheckItem::new("OpenCC", check_opencc()),
        ],
    }
}

/// 執行非 GUI 指令，回傳結束碼。`Command::RunGui` 回傳 `None`。
pub fn run_headless(command: Command) -> Option<i32> {
    match command {
        Command::RunGui => None,
        Command::Version => {
            println!("{}", version_text());
            Some(0)
        }
        Command::Help => {
            println!("{}", help_text());
            Some(0)
        }
        Command::ConfigPath => match config_path() {
            Some(path) => {
                println!("{}", path.display());
                Some(0)
            }
            None => {
                eprintln!("無法取得設定目錄");
                Some(1)
            }
        },
        Command::SelfCheck => {
            let report = run_self_check();
            println!("{}", report.render());
            Some(report.exit_code())
        }
    }
}

/// release 版以 `windows_subsystem = "windows"` 編譯、沒有主控台；
/// 從終端機執行時附加到父行程的主控台，讓 println! 看得到。
pub fn attach_parent_console() {
    #[cfg(windows)]
    {
        use winapi::um::wincon::{AttachConsole, ATTACH_PARENT_PROCESS};
        // SAFETY: 單純的 Win32 呼叫；已有主控台（debug 版）或沒有父主控台時回傳 0，忽略即可。
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_runs_gui() {
        assert_eq!(parse_args(Vec::<String>::new()), Ok(Command::RunGui));
    }

    #[test]
    fn recognised_flags() {
        assert_eq!(parse_args(["--version"]), Ok(Command::Version));
        assert_eq!(parse_args(["-V"]), Ok(Command::Version));
        assert_eq!(parse_args(["--help"]), Ok(Command::Help));
        assert_eq!(parse_args(["-h"]), Ok(Command::Help));
        assert_eq!(parse_args(["/?"]), Ok(Command::Help));
        assert_eq!(parse_args(["--config-path"]), Ok(Command::ConfigPath));
        assert_eq!(parse_args(["--self-check"]), Ok(Command::SelfCheck));
    }

    #[test]
    fn unknown_and_extra_args_are_errors() {
        assert!(parse_args(["--bogus"]).unwrap_err().0.contains("--bogus"));
        assert!(parse_args(["-v"]).is_err(), "flags are case-sensitive");
        assert!(parse_args(["--version", "--help"]).is_err());
    }

    #[test]
    fn version_and_help_text() {
        assert!(version_text().contains(crate::VERSION));
        let help = help_text();
        for flag in ["--help", "--version", "--config-path", "--self-check"] {
            assert!(help.contains(flag), "help missing {flag}");
        }
    }

    #[test]
    fn config_path_ends_with_expected_file() {
        if let Some(p) = config_path() {
            assert!(p.ends_with("whisper-typeless/config.toml"));
        }
    }

    #[test]
    fn embedded_catalog_passes() {
        assert!(check_model_catalog(EMBEDDED_MODEL_CATALOG).is_ok());
    }

    #[test]
    fn catalog_errors_detected() {
        assert!(check_model_catalog("nope").is_err());
        assert!(check_model_catalog(r#"{"models":[]}"#).is_err());
        let dup = r#"{"models":[
            {"id":"a","download_url":"https://x/a","sha1":"bd577a113a864445d4c299885e0cb97d4ba92b5f"},
            {"id":"a","download_url":"https://x/b","sha1":"bd577a113a864445d4c299885e0cb97d4ba92b5f"}]}"#;
        assert!(check_model_catalog(dup).unwrap_err().contains("重複"));
        let http = r#"{"models":[{"id":"a","download_url":"http://x","sha1":"bd577a113a864445d4c299885e0cb97d4ba92b5f"}]}"#;
        assert!(check_model_catalog(http).is_err());
        let bad_sum = r#"{"models":[{"id":"a","download_url":"https://x","sha256":"zz"}]}"#;
        assert!(check_model_catalog(bad_sum).is_err());
    }

    #[test]
    fn templates_opencc_and_default_config_pass() {
        assert!(check_templates().is_ok());
        assert!(check_opencc().is_ok());
        assert!(check_default_config().is_ok());
        assert!(check_config_source("this is = = not toml").is_err());
    }

    #[test]
    fn report_rendering_and_exit_code() {
        let mut report = SelfCheckReport {
            items: vec![CheckItem::new("a", Ok("fine".into()))],
        };
        assert_eq!(report.exit_code(), 0);
        report.items.push(CheckItem::new("b", Err("broken".into())));
        assert_eq!(report.exit_code(), 1);
        let text = report.render();
        assert!(text.contains("[OK  ] a: fine"));
        assert!(text.contains("[FAIL] b: broken"));
        assert!(text.contains("1/2"));
    }

    #[test]
    fn run_headless_gui_returns_none() {
        assert_eq!(run_headless(Command::RunGui), None);
    }
}
