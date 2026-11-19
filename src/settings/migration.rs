//! 舊版 `settings.json` → config.toml 一次性匯入。
//!
//! v0.2 以前 UI 設定寫在工作目錄（或 exe 目錄）的 `settings.json`。啟動時若找到，
//! 將其內容套用到 AppConfig、寫入 config.toml，然後改名為 `settings.json.migrated`，
//! 之後不再讀取。

use super::config::AppConfig;
use super::persistence::ConfigPersistence;
use super::ui_settings::UiSettings;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

pub const LEGACY_FILE_NAME: &str = "settings.json";
pub const MIGRATED_SUFFIX: &str = ".migrated";

/// 舊版 settings.json 可能的位置：工作目錄、exe 目錄（去重）。
pub fn legacy_candidates() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir);
    }
    dedup_paths(dirs.into_iter().map(|d| d.join(LEGACY_FILE_NAME)).collect())
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for p in paths {
        let key = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if !seen.contains(&key) {
            seen.push(key);
            out.push(p);
        }
    }
    out
}

fn migrated_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| LEGACY_FILE_NAME.into());
    name.push(MIGRATED_SUFFIX);
    path.with_file_name(name)
}

/// 匯入結果
#[derive(Debug, Default)]
pub struct MigrationReport {
    /// 實際匯入的檔案
    pub imported_from: Option<PathBuf>,
    /// 已改名為 .migrated 的檔案
    pub renamed: Vec<PathBuf>,
    /// 給使用者看的警告
    pub warnings: Vec<String>,
}

/// 將第一個可解析的舊版 settings.json 套用到 `config`（只改記憶體，不寫檔）。
/// 所有找到的舊檔都視為已處理。回傳 (匯入來源, 找到的所有檔案, 警告)。
fn import_into(
    config: &mut AppConfig,
    candidates: &[PathBuf],
) -> (Option<PathBuf>, Vec<PathBuf>, Vec<String>) {
    let mut imported = None;
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    for path in candidates.iter().filter(|p| p.is_file()) {
        found.push(path.clone());
        if imported.is_some() {
            info!("已匯入其他舊設定，忽略 {}", path.display());
            continue;
        }
        let parsed = std::fs::read_to_string(path)
            .map_err(anyhow::Error::from)
            .and_then(|s| {
                let s = s.strip_prefix('\u{feff}').unwrap_or(&s).to_string();
                serde_json::from_str::<UiSettings>(&s).map_err(anyhow::Error::from)
            });
        match parsed {
            Ok(legacy) => {
                legacy.apply_to(config);
                info!("已匯入舊版設定: {}", path.display());
                imported = Some(path.clone());
            }
            Err(e) => {
                warn!("舊版設定 {} 解析失敗: {e}", path.display());
                warnings.push(format!(
                    "舊版設定檔 {} 無法解析，已略過（{e}）",
                    path.display()
                ));
            }
        }
    }
    (imported, found, warnings)
}

/// 匯入舊版 settings.json → 寫入 config.toml → 舊檔改名為 settings.json.migrated。
///
/// `allow_save` 為 false（例如 config.toml 損毀且備份失敗）時不做任何事，
/// 以免覆寫無法復原的原檔。
pub fn migrate_legacy_settings(
    config: &mut AppConfig,
    persistence: &ConfigPersistence,
    candidates: &[PathBuf],
    allow_save: bool,
) -> MigrationReport {
    let mut report = MigrationReport::default();
    if !candidates.iter().any(|p| p.is_file()) {
        return report;
    }
    if !allow_save {
        report
            .warnings
            .push("偵測到舊版 settings.json，但設定檔無法安全寫入，暫不匯入".into());
        return report;
    }

    let mut updated = config.clone();
    let (imported, found, warnings) = import_into(&mut updated, candidates);
    report.warnings.extend(warnings);

    if imported.is_some() {
        if let Err(e) = persistence.save(&updated) {
            error!("寫入匯入後的設定失敗: {e}");
            report
                .warnings
                .push(format!("舊版設定匯入後寫入 config.toml 失敗：{e}"));
            // 未寫入成功 → 保留舊檔，下次再試；本次仍使用匯入後的值
            *config = updated;
            return report;
        }
        *config = updated;
        report.imported_from = imported;
    }

    // 已處理（成功匯入或確認無法解析）的舊檔一律改名，確保只匯入一次
    for path in found {
        let dest = migrated_path(&path);
        match std::fs::rename(&path, &dest) {
            Ok(()) => {
                info!("舊設定已改名: {} → {}", path.display(), dest.display());
                report.renamed.push(dest);
            }
            Err(e) => {
                error!("舊設定改名失敗 {}: {e}", path.display());
                report.warnings.push(format!(
                    "無法將 {} 改名為 .migrated（{e}），請手動刪除以免重複匯入",
                    path.display()
                ));
            }
        }
    }
    report
}

/// 啟動時的設定：載入 config.toml（損毀時備份並回退）+ 匯入舊版 settings.json。
#[derive(Debug)]
pub struct StartupConfig {
    pub config: AppConfig,
    /// 需在 UI 錯誤紀錄中顯示的警告
    pub warnings: Vec<String>,
}

pub fn load_startup_config(persistence: &ConfigPersistence, legacy: &[PathBuf]) -> StartupConfig {
    let report = persistence.load_or_recover();
    let mut warnings: Vec<String> = report.warning.into_iter().collect();
    let mut config = report.config;
    let migration =
        migrate_legacy_settings(&mut config, persistence, legacy, report.safe_to_overwrite);
    warnings.extend(migration.warnings);
    StartupConfig { config, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::test_util::TempDir;

    const LEGACY_JSON: &str = r#"{
  "vad_threshold": 0.45,
  "silence_timeout_ms": 1800.0,
  "n_threads": 6,
  "use_gpu": false,
  "temperature": 0.2,
  "conversion_mode": "zh-CN",
  "hotkey": "Ctrl+Shift+Q",
  "ptt_hotkey": "F8",
  "ptt_mode": false,
  "mod_vad": true,
  "mod_context": false,
  "mod_opencc": true,
  "mod_inject": true,
  "mod_waveform": false,
  "mod_history": true,
  "append_newline": false,
  "clipboard_enabled": true,
  "inject_enabled": false,
  "selected_model_id": "medium"
}"#;

    #[test]
    fn legacy_settings_imported_once_and_renamed() {
        let dir = TempDir::new("legacy");
        let cfg_path = dir.path().join("cfg").join("config.toml");
        let legacy = dir.path().join("settings.json");
        std::fs::write(&legacy, LEGACY_JSON).unwrap();
        let persistence = ConfigPersistence::new(cfg_path.clone());

        let startup = load_startup_config(&persistence, std::slice::from_ref(&legacy));
        assert!(startup.warnings.is_empty(), "{:?}", startup.warnings);
        let c = &startup.config;
        assert_eq!(c.audio.vad_threshold, 0.45);
        assert_eq!(c.audio.vad_silence_timeout_ms, 1800);
        assert_eq!(c.whisper.n_threads, 6);
        assert!(!c.whisper.use_gpu);
        assert_eq!(c.general.global_hotkey, "Ctrl+Shift+Q");
        assert_eq!(c.general.ptt_hotkey, "F8");
        assert!(!c.general.ptt_mode);
        assert!(!c.modules.context_templates);
        assert!(!c.output.append_newline);
        assert_eq!(c.model.selected_id, "medium");

        // 舊檔已改名，config.toml 已寫入
        assert!(!legacy.exists());
        assert!(dir.path().join("settings.json.migrated").exists());
        let on_disk = persistence.load().unwrap();
        assert_eq!(on_disk.audio.vad_threshold, 0.45);

        // 第二次啟動：沒有舊檔，不再匯入，值來自 config.toml
        let again = load_startup_config(&persistence, std::slice::from_ref(&legacy));
        assert!(again.warnings.is_empty());
        assert_eq!(again.config.audio.vad_threshold, 0.45);
    }

    #[test]
    fn first_valid_candidate_wins_and_all_are_renamed() {
        let dir = TempDir::new("legacy_multi");
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let cwd_file = a.join("settings.json");
        let exe_file = b.join("settings.json");
        std::fs::write(&cwd_file, LEGACY_JSON).unwrap();
        std::fs::write(&exe_file, r#"{"vad_threshold":0.8}"#).unwrap();
        let persistence = ConfigPersistence::new(dir.path().join("config.toml"));

        let startup = load_startup_config(&persistence, &[cwd_file.clone(), exe_file.clone()]);
        assert_eq!(startup.config.audio.vad_threshold, 0.45);
        assert!(!cwd_file.exists() && !exe_file.exists());
        assert!(b.join("settings.json.migrated").exists());
    }

    #[test]
    fn corrupt_legacy_file_is_reported_and_not_retried() {
        let dir = TempDir::new("legacy_bad");
        let legacy = dir.path().join("settings.json");
        std::fs::write(&legacy, "{ not json").unwrap();
        let persistence = ConfigPersistence::new(dir.path().join("config.toml"));

        let startup = load_startup_config(&persistence, std::slice::from_ref(&legacy));
        assert_eq!(startup.warnings.len(), 1);
        assert_eq!(
            startup.config.audio.vad_threshold,
            crate::audio::VAD_DEFAULT_THRESHOLD
        );
        assert!(!legacy.exists());
        assert!(
            !persistence.path().exists(),
            "未匯入任何值時不應建立 config.toml"
        );
    }

    #[test]
    fn no_legacy_file_is_a_no_op() {
        let dir = TempDir::new("legacy_none");
        let persistence = ConfigPersistence::new(dir.path().join("config.toml"));
        let startup = load_startup_config(&persistence, &[dir.path().join("settings.json")]);
        assert!(startup.warnings.is_empty());
        assert!(!persistence.path().exists());
    }

    #[test]
    fn corrupt_config_plus_legacy_keeps_backup_and_imports() {
        let dir = TempDir::new("legacy_corrupt_cfg");
        let cfg_path = dir.path().join("config.toml");
        std::fs::write(&cfg_path, "[[[").unwrap();
        let legacy = dir.path().join("settings.json");
        std::fs::write(&legacy, LEGACY_JSON).unwrap();
        let persistence = ConfigPersistence::new(cfg_path);

        let startup = load_startup_config(&persistence, std::slice::from_ref(&legacy));
        assert_eq!(startup.warnings.len(), 1, "{:?}", startup.warnings);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config.toml.bak")).unwrap(),
            "[[["
        );
        assert_eq!(startup.config.audio.vad_threshold, 0.45);
        assert_eq!(persistence.load().unwrap().audio.vad_threshold, 0.45);
    }

    #[test]
    fn migration_skipped_when_save_not_allowed() {
        let dir = TempDir::new("legacy_blocked");
        let legacy = dir.path().join("settings.json");
        std::fs::write(&legacy, LEGACY_JSON).unwrap();
        let persistence = ConfigPersistence::new(dir.path().join("config.toml"));
        let mut cfg = AppConfig::default();
        let report =
            migrate_legacy_settings(&mut cfg, &persistence, std::slice::from_ref(&legacy), false);
        assert_eq!(report.warnings.len(), 1);
        assert!(legacy.exists());
        assert!(!persistence.path().exists());
    }
}
