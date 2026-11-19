# AGENTS.md

給 AI 程式代理（coding agents）的開發指引。人類版概覽見 [README.md](README.md)，規格見 [docs/spec.md](docs/spec.md)，模組關係見 [docs/architecture.md](docs/architecture.md)。

## 專案目的

whisper-typeless：Windows 桌面即時語音轉文字工具。Rust + Slint（UI）+ whisper-rs（whisper.cpp 綁定）。辨識結果自動輸入到目前焦點視窗。

## 核心價值（修改時不可倒退）

1. **本機隱私**：語音與文字不離開電腦。唯一的對外網路行為是下載模型（URL 來自 `model-catalog.json`）。
2. **結果落在焦點視窗**：辨識後寫入剪貼簿並模擬貼上（enigo / SendInput）。
3. **中文優先**：預設 OpenCC `zh-TW`；另有 `zh-HK`、`zh-CN`、`raw`。
4. **情境模板**：內建 `TemplateId`（會議、口語、技術、醫療、法律）與使用者 TOML 覆寫；控制初始提示詞與後處理。
5. **鍵盤優先的浮動 UI**：Liquid Glass 風格；收合 380x52、展開 380x540。
6. **模型目錄**：`model-catalog.json` 是模型清單、下載網址與校驗值的唯一來源；程式碼與腳本都從它讀取。
7. **單一小型 exe**：CUDA 為選用（Cargo feature `cuda`）。

## 建置、測試、檢查

Windows PowerShell。優先使用腳本：

```powershell
.\scripts\dev.ps1 check        # cargo check --all-targets
.\scripts\dev.ps1 test         # cargo test
.\scripts\dev.ps1 lint         # clippy -D warnings + fmt --check
.\scripts\dev.ps1 build        # release，CPU
.\scripts\dev.ps1 build -Cuda  # release，含 CUDA（需要 CUDA Toolkit）
```

等價的 cargo 指令（CI 使用 windows-latest、MSVC 工具鏈、CPU 版）：

```powershell
cargo check --all-targets --no-default-features
cargo test --no-default-features
cargo clippy --all-targets --no-default-features -- -D warnings
cargo fmt --all -- --check
cargo build --release --no-default-features
```

**預設工具鏈是 GNU 的主機**（無法建置 whisper-rs-sys）：必須設定 `LIBCLANG_PATH` 並指定 MSVC target：

```powershell
$env:LIBCLANG_PATH = "<含 libclang.dll 的資料夾>"
cargo test --target x86_64-pc-windows-msvc --no-default-features
cargo clippy --all-targets --target x86_64-pc-windows-msvc --no-default-features -- -D warnings
```

`Cargo.toml` 的 `default = ["cuda"]`，因此不加 `--no-default-features` 會嘗試 CUDA 建置。

## 模組地圖

- `src/main.rs`：執行檔入口；初始化 tracing，建立 `AppState`，啟動 UI。
- `src/lib.rs`：crate 根；`VERSION`、`APP_NAME`、模組宣告，並 include Slint 產生的模組。
- `src/app.rs`：`AppState`（錄音狀態、目前模板與模型、轉錄歷史上限 1000、設定與模組註冊表）。
- `src/audio/`：`capture.rs` 麥克風串流（cpal）、`resampler.rs` 重採樣到 16 kHz、`vad.rs` 能量式靜音偵測；`mod.rs` 放共用音訊常數。
- `src/whisper/`：`engine.rs` 推論引擎、`params.rs` 完整 whisper 參數映射、`session.rs` 推論 session。
- `src/models/`：`registry.rs` 載入內嵌與外部的模型目錄、`downloader.rs` 串流下載（含備用網址與校驗）、`manager.rs` 本機模型掃描（`.gguf` 與 `ggml-*.bin`）。
- `src/transcription/`：`pipeline.rs` 轉錄後處理流水線、`opencc.rs` 繁簡轉換、`postprocess.rs` 贅字與重複移除、標點。
- `src/context_templates/`：`templates.rs` `TemplateId` 與內建模板、`detector.rs` 關鍵字情境偵測、`custom.rs` 使用者模板讀寫。
- `src/output/`：`mod.rs` `OutputRouter` 與 `OutputMode`、`clipboard.rs` 剪貼簿、`injector.rs` 焦點視窗注入、`formatter.rs` 輸出格式化。
- `src/settings/`：`config.rs` `AppConfig`（TOML，設定檔路徑）、`persistence.rs` 讀寫、`ui_settings.rs` 舊版 `settings.json`、`mod.rs` 模組開關註冊表。
- `src/file_transcription/`：音檔解碼（symphonia）與 TXT / SRT / VTT 匯出。
- `src/ui/`：`mod.rs` 執行 UI 與 `ui_update!` 巨集；`bridge.rs` 視窗模式、置頂、拖曳與 callback 接線；`window_icon.rs` 依錄音狀態更新工作列圖示。
- `src/ui/bridge/`：`recording.rs` 錄音與轉錄流程、`hotkey.rs` 全域快捷鍵（Win32）、`ptt.rs` 按住說話（低階鍵盤 hook）、`templates.rs` 模板編輯、`file_mode.rs` 檔案轉錄。
- `ui/appwindow.slint`：主視窗、鍵盤處理（Esc）、完整視窗與浮動視窗切換、錯誤紀錄覆蓋層。
- `ui/components/`：`compact_bar.slint` 收合列、`quick_actions.slint` 快速操作列、`context_bar.slint` 模板列、`model_picker.slint` 模型選擇、`settings_panel.slint` 設定面板、`file_mode.slint` 檔案模式、`transcript.slint` 文字區、`waveform.slint` 波形、`toggle.slint` 開關。
- `ui/styles/tokens.slint`：設計 token（淺色、深色、玻璃材質）。
- `build.rs`：編譯 `ui/appwindow.slint`。
- `tests/`：整合測試（`audio_tests.rs`、`transcription_tests.rs`、`output_tests.rs`）。
- `scripts/`：PowerShell 工具腳本；`model-catalog.json` 與 `MODEL_CATALOG.md` 位於根目錄。
- `.github/workflows/`：CI（check、clippy、fmt、test、build）與 tag 觸發的 release 流程。

## 開發規則

- **測試不得依賴音訊硬體、GPU、網路或模型檔案。** 以純邏輯、假資料或 fixture 測試；需要實機驗證的項目寫在 PR 說明中。
- **提交前 `cargo fmt --all -- --check` 與 `cargo clippy --all-targets -- -D warnings` 必須通過。**
- **使用者可見字串（UI 文字、對話框、狀態列、錯誤訊息）使用 zh-TW。** 程式註解可用繁體中文。
- **不要記錄語音或轉錄內容**（包含 info 層級日誌）；不要新增網路呼叫或遙測。
- 模型網址與校驗值只改 `model-catalog.json`，不要寫進 Rust 或 PowerShell 程式碼。
- 新增 crate 依賴需有明確理由，並一併更新 `Cargo.lock`。
- 不要提交執行期產物：`settings.json`、`models/`、錄音 WAV、`target/`、任何含憑證的檔案。
- 文件放在 `docs/`（規格、架構、路線圖）；不要新增臨時報告類 Markdown。

## 發版流程

1. 修改 `Cargo.toml` 的 `version`（並執行一次 `cargo check` 讓 `Cargo.lock` 同步）。
2. 在 `CHANGELOG.md` 新增 `## [X.Y.Z] - YYYY-MM-DD` 區段（Keep a Changelog 格式）。
3. 確認 CI 在 `main` 上通過後合併。
4. 建立並推送 tag：`git tag vX.Y.Z`、`git push origin vX.Y.Z`。
5. `.github/workflows/release.yml` 會建置 `whisper-typeless-vX.Y.Z-windows-x86_64-cpu.zip`、產生 `SHA256SUMS`，並以自動產生的內容建立 Release 說明。

只有在使用者明確要求時才建立或推送 tag。
