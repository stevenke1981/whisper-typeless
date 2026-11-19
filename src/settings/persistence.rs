//! config.toml 讀寫。設定檔是唯一的持久化來源；損毀時不得默默覆蓋。

use super::config::AppConfig;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// 載入結果：設定值 + 需要顯示給使用者的警告。
#[derive(Debug)]
pub struct LoadReport {
    pub config: AppConfig,
    /// 啟動警告（zh-TW），UI 可放入錯誤紀錄
    pub warning: Option<String>,
    /// 損毀檔案已成功備份（或無需備份）。為 false 時不應覆寫原檔。
    pub safe_to_overwrite: bool,
}

pub struct ConfigPersistence {
    path: PathBuf,
}

impl ConfigPersistence {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 預設位置：`dirs::config_dir()/whisper-typeless/config.toml`
    pub fn default_location() -> anyhow::Result<Self> {
        Ok(Self::new(AppConfig::config_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 損毀檔備份路徑：`config.toml.bak`
    pub fn backup_path(&self) -> PathBuf {
        let mut name = self
            .path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| "config.toml".into());
        name.push(".bak");
        self.path.with_file_name(name)
    }

    /// 嚴格載入：檔案不存在 → 預設值；解析失敗 → 回傳錯誤（不做任何修復）。
    pub fn load(&self) -> anyhow::Result<AppConfig> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }
        let bytes = std::fs::read(&self.path)?;
        parse_config(&bytes)
    }

    /// 載入並在失敗時復原：
    /// - 檔案不存在 → 預設值，無警告
    /// - 解析失敗 → 複製為 `config.toml.bak`、記錄錯誤、使用預設值並回傳警告
    /// - 讀取失敗（I/O）→ 使用預設值並回傳警告，不覆寫原檔
    pub fn load_or_recover(&self) -> LoadReport {
        if !self.path.exists() {
            return LoadReport {
                config: AppConfig::default(),
                warning: None,
                safe_to_overwrite: true,
            };
        }
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) => {
                error!("無法讀取設定檔 {}: {e}", self.path.display());
                return LoadReport {
                    config: AppConfig::default(),
                    warning: Some(format!(
                        "無法讀取設定檔 {}：{e}。本次使用預設設定。",
                        self.path.display()
                    )),
                    safe_to_overwrite: false,
                };
            }
        };
        match parse_config(&bytes) {
            Ok(config) => {
                info!("已載入設定: {}", self.path.display());
                LoadReport {
                    config,
                    warning: None,
                    safe_to_overwrite: true,
                }
            }
            Err(parse_err) => {
                let backup = self.backup_path();
                error!(
                    "設定檔損毀 {}: {parse_err:#}；備份至 {}",
                    self.path.display(),
                    backup.display()
                );
                let (warning, safe) = match std::fs::copy(&self.path, &backup) {
                    Ok(_) => (
                        format!(
                            "設定檔損毀，已備份至 {}，本次使用預設設定。錯誤：{parse_err:#}",
                            backup.display()
                        ),
                        true,
                    ),
                    Err(copy_err) => {
                        error!("設定檔備份失敗: {copy_err}");
                        (
                            format!(
                                "設定檔損毀且備份失敗（{copy_err}），本次使用預設設定且不會覆寫原檔。錯誤：{parse_err:#}"
                            ),
                            false,
                        )
                    }
                };
                LoadReport {
                    config: AppConfig::default(),
                    warning: Some(warning),
                    safe_to_overwrite: safe,
                }
            }
        }
    }

    /// 寫入 config.toml（先寫暫存檔再取代，避免寫一半造成損毀）。
    pub fn save(&self, config: &AppConfig) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(config)?;
        let mut tmp_name = self
            .path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| "config.toml".into());
        tmp_name.push(".tmp");
        let tmp = self.path.with_file_name(tmp_name);
        std::fs::write(&tmp, content)?;
        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            warn!("設定檔原子取代失敗，改為直接寫入: {e}");
            let _ = std::fs::remove_file(&tmp);
            std::fs::write(&self.path, toml::to_string_pretty(config)?)?;
        }
        info!("設定已儲存: {}", self.path.display());
        Ok(())
    }

    pub fn export(&self, dest: &Path) -> anyhow::Result<()> {
        std::fs::copy(&self.path, dest)?;
        Ok(())
    }

    /// 匯入另一份 config.toml；解析失敗時回傳錯誤且不修改現有設定。
    pub fn import(&self, src: &Path) -> anyhow::Result<AppConfig> {
        let bytes = std::fs::read(src)?;
        let config = parse_config(&bytes)?;
        self.save(&config)?;
        Ok(config)
    }
}

fn parse_config(bytes: &[u8]) -> anyhow::Result<AppConfig> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| anyhow::anyhow!("設定檔不是有效的 UTF-8: {e}"))?;
    // 容忍 UTF-8 BOM（記事本存檔時可能加入）
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    Ok(toml::from_str(text)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::test_util::TempDir;

    #[test]
    fn missing_file_gives_defaults_without_warning() {
        let dir = TempDir::new("missing");
        let p = ConfigPersistence::new(dir.path().join("config.toml"));
        let report = p.load_or_recover();
        assert!(report.warning.is_none());
        assert!(report.safe_to_overwrite);
        assert!(!p.backup_path().exists());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = TempDir::new("roundtrip");
        let p = ConfigPersistence::new(dir.path().join("sub").join("config.toml"));
        let mut cfg = AppConfig::default();
        cfg.audio.vad_threshold = 0.42;
        cfg.output.privacy_mode = true;
        cfg.model.selected_id = "medium".into();
        p.save(&cfg).unwrap();
        let loaded = p.load().unwrap();
        assert_eq!(loaded.audio.vad_threshold, 0.42);
        assert!(loaded.output.privacy_mode);
        assert_eq!(loaded.model.selected_id, "medium");
        let report = p.load_or_recover();
        assert!(report.warning.is_none());
    }

    #[test]
    fn corrupt_toml_is_backed_up_and_reported() {
        let dir = TempDir::new("corrupt");
        let path = dir.path().join("config.toml");
        let garbage = "[audio\nvad_threshold = = 'oops";
        std::fs::write(&path, garbage).unwrap();
        let p = ConfigPersistence::new(path.clone());

        assert!(p.load().is_err(), "嚴格載入必須回報錯誤");

        let report = p.load_or_recover();
        let warning = report.warning.expect("損毀時必須回傳警告");
        assert!(warning.contains("損毀"), "{warning}");
        assert!(report.safe_to_overwrite);
        // 預設值回退
        assert_eq!(
            report.config.audio.vad_threshold,
            crate::audio::VAD_DEFAULT_THRESHOLD
        );
        // 備份內容與原檔一致，原檔未被覆寫
        let bak = dir.path().join("config.toml.bak");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), garbage);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), garbage);
    }

    #[test]
    fn invalid_utf8_is_treated_as_corrupt() {
        let dir = TempDir::new("utf8");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x41]).unwrap();
        let report = ConfigPersistence::new(path).load_or_recover();
        assert!(report.warning.is_some());
        assert!(dir.path().join("config.toml.bak").exists());
    }

    #[test]
    fn partial_config_fills_missing_fields_with_defaults() {
        let dir = TempDir::new("partial");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[output]\nprivacy_mode = true\n").unwrap();
        let cfg = ConfigPersistence::new(path).load().unwrap();
        assert!(cfg.output.privacy_mode);
        assert!(cfg.output.append_newline);
        assert!(cfg.modules.vad);
        assert_eq!(cfg.general.ptt_hotkey, "Ctrl+Alt+L");
    }

    #[test]
    fn import_rejects_corrupt_source() {
        let dir = TempDir::new("import");
        let p = ConfigPersistence::new(dir.path().join("config.toml"));
        let src = dir.path().join("other.toml");
        std::fs::write(&src, "not = [valid").unwrap();
        assert!(p.import(&src).is_err());
        assert!(!p.path().exists());
    }
}
