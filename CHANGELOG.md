# 更新紀錄

本檔案格式參考 [Keep a Changelog](https://keepachangelog.com/zh-TW/1.1.0/)，版本號遵循 [語意化版本](https://semver.org/lang/zh-TW/)。

## [0.5.0] - 2026-11-19

### Added

- OpenAI 即時轉錄（Realtime API，`gpt-live-transcribe` / `gpt-transcribe`）：設定中選「OpenAI 雲端」引擎並勾選同意後，錄音即時串流到 OpenAI，逐句輸出並顯示即時文字；結果同樣經 OpenCC 轉為繁體。隱私模式開啟時一律使用本機。
- 在 App 內設定 OpenAI API 金鑰：以 Windows DPAPI 加密保存於使用者設定目錄（不寫入 `config.toml`、日誌）；未設定時改讀環境變數 `OPENAI_API_KEY`。
- 雲端轉錄失敗且尚未輸出時，若有本機模型則自動改用本機轉錄。
- 系統匣圖示：左鍵顯示／隱藏主視窗，右鍵選單（顯示主視窗、開始／停止錄音、設定、結束）；錄音中圖示變紅；Explorer 重啟後自動重新加入。
- 設定「系統匣」區段：關閉視窗時縮小到系統匣（預設開啟）、最小化時縮小到系統匣、啟動時縮小到系統匣。
- Vulkan GPU 加速（Cargo feature `vulkan`）：Release 同時提供 `-cpu` 與 `-vulkan` 兩種 zip，`SHA256SUMS.txt` 彙整兩者。
- Vulkan 版延遲載入 `vulkan-1.dll` 並於使用前探測；GPU 初始化失敗時自動改用 CPU（裝置標籤「CPU（GPU 初始化失敗）」）。
- `--self-check` 回報 GPU 後端與執行環境狀態。
- 設定新增「GPU 與模型」區段：使用 GPU 開關（下次載入模型生效）與小型模型建議。
- CI 新增 Vulkan 工作；`scripts/dev.ps1` 新增 `-Vulkan` 並檢查 `VULKAN_SDK`。

### Changed

- 模型清單以 `recommended` 標記 tiny / base / small，並置頂顯示（由小到大）。
- 選用的模型遺失時，自動改用已下載的最小模型，不再跳出下載提示。
- Medium 以上的模型下載前一律先確認大小。
- 裝置標籤改為「GPU (Vulkan)」「CPU（此版本未含 GPU）」，取代 CUDA 專用字樣。
- 舊版 `model-catalog.json` 未標示 `recommended` 時，依大小判斷（500 MB 以下視為小型模型）。
- 隱私規則更新：除模型下載外，唯一允許的對外連線是使用者明確選用並同意的 OpenAI 雲端轉錄。

## [0.4.0] - 2026-11-19

### Added

- 預設 GUI 改為 AcmeUI-Native（Rust 原生介面，預設 CPU 軟體渲染）。
- 命令列參數 `--ui acme|slint`；`--acme-shell` 保留為 `--ui acme` 的別名。
- UI 無關的核心（`src/core/`）：`AppViewState`、`UiCommand`、`CoreEvent`、`Controller`；畫面層只透過這些型別與核心溝通。
- 環境變數 `ACME_RENDER_BACKEND`（`cpu`、`gpu`、`auto`）可覆寫 AcmeUI 的渲染後端。

### Changed

- Slint 畫面層精簡為薄轉接層（`src/ui/`），業務邏輯移至 `src/core/`。
- AcmeUI-Native 相依以固定 git rev（`c2e0fd9`）引用。

### Fixed

- 視窗啟動與縮放時會限制在螢幕工作區內，不再被工作列遮住錄音按鈕與狀態列（1080p／150% 縮放）。
- 狀態列文字依寬度精簡與省略，不再超出視窗。
- 工作列圖示改以本程序視窗控制代碼尋找，修正因標題含版本號而找不到視窗的問題。
- 修正 `SHA256SUMS.txt` 以 CRLF 寫入的問題，改為 LF，`sha256sum -c` 可直接驗證。

### Deprecated

- Slint 介面（`--ui slint`）為本版備援，將於下一版移除。

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
- 視窗內快捷鍵：`Space` 開始／停止錄音、`E` 展開／收合浮動視窗。
- 專案文件：雙語 README、AGENTS.md / CLAUDE.md（給 AI 代理）、`docs/` 目錄（規格、架構、路線圖）、MIT 授權檔。

### Changed

- `Cargo.toml` 預設 feature 改為空（CPU 版）；CUDA 需加 `--features cuda`。
- 情境模板以穩定英文 id 為索引，不再依賴顯示名稱。
- 設定儲存改為先寫暫存檔再取代，避免寫入中斷導致設定損毀。
- 建置與檢查腳本整併為 `scripts/dev.ps1`（`check`、`test`、`lint`、`build [-Cuda]`）。

### Fixed

- 修正應用內模型下載一律校驗失敗：依 catalog 校驗值長度自動選用 SHA-1 或 SHA-256。
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
