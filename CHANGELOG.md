# 更新紀錄

本檔案格式參考 [Keep a Changelog](https://keepachangelog.com/zh-TW/1.1.0/)，版本號遵循 [語意化版本](https://semver.org/lang/zh-TW/)。

## [0.3.0] - 2026-11-19

### Added

- 單一 TOML 設定檔（`config.toml`）；首次啟動時自動遷移舊版執行目錄下的 `settings.json`。
- 設定檔無法解析時，先備份為舊檔再以預設值啟動，不直接覆寫。
- 隱私模式開關（設定面板）：不保存錄音 WAV，輸出後清除剪貼簿。
- 即時輸出管線：辨識結果統一經 `OutputRouter` 與 `TranscriptionPipeline` 輸出與處理。
- 錯誤紀錄面板：錯誤事件由 tracing 收集並顯示於介面（⚠ 按鈕與計數）。
- 命令列參數：`--version`、`--help`、`--config-path`、`--self-check`。
- 情境模板改由 TOML 定義；可用 `templates/*.toml` 覆寫內建模板。
- 大量新增單元與整合測試（不依賴音訊、GPU、網路或模型檔案）。
- tag 觸發的 release workflow（`release.yml`）：建置 zip、產生 `SHA256SUMS`，並自動產生 Release 說明。
- 專案文件：雙語 README、AGENTS.md / CLAUDE.md（給 AI 代理）、`docs/` 目錄（規格、架構、路線圖）、MIT 授權檔。

### Changed

- 建置與檢查腳本整併為 `scripts/dev.ps1`（`check`、`test`、`lint`、`build [-Cuda]`）。

### Fixed

- 修正 CI：改為 CPU 版建置（`--no-default-features`），在 `windows-latest` 的 MSVC 工具鏈上執行。

## [0.2.0] - 2026-06-29

### Added

- 程式版本常數（`VERSION`、`APP_NAME`）：視窗標題與啟動日誌顯示版本與目標架構。
- 支援 `.gguf` 模型檔名，與既有的 `ggml-*.bin` 並存。
- 隱私模式的錄音流程：不保存 WAV，並清除剪貼簿。
- GitHub Actions CI（check → clippy → fmt → test → build）。
- 測試由 9 個增加到 50 個（40 個單元測試、10 個整合測試），涵蓋 whisper 參數、後處理、情境模板、模型目錄、VAD 與檔案轉錄。

### Changed

- 抽出共用音訊常數（取樣率、波形條數、最大段落長度等）至 `src/audio/mod.rs`。
- 新增 `ui_update!` 巨集，減少 `invoke_from_event_loop` 的重複樣板。
- 檔案模式由 `bridge.rs` 拆分至 `bridge/file_mode.rs`。
- 統一 VAD 閾值的預設值。
- 模型校驗改用 SHA-256（保留舊欄位名稱作為相容別名）。
- 錄音相關邏輯拆分至 `bridge/recording.rs`、熱鍵拆分至 `bridge/hotkey.rs`、按住說話拆分至 `bridge/ptt.rs`、模板拆分至 `bridge/templates.rs`。
- 預設模型改為 medium。

### Fixed

- 修正模型下載流程與 CUDA 建置腳本。
- PTT 錄音流程修正。
- 隱藏 ICU4X 啟動時的雜訊日誌。
- 清除執行期產物，更新 `.gitignore`。
