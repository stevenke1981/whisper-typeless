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

`Cargo.toml` 的 `default = []`（CPU 版）；CUDA 需 `--features cuda` 與 CUDA Toolkit。

**在 git worktree（例如 `.claude/worktrees/...`）中建置**：路徑過長會讓 whisper-rs-sys 的 CMake/MSBuild 失敗，請設定短的 `CARGO_TARGET_DIR`（例如 `C:/t/wt1`）。

**無 GUI 的快速驗證**：`whisper-typeless.exe --self-check`（模型清單、範本、設定檔、OpenCC），結束碼 0 表示通過。

## 模組地圖

- `src/main.rs`：入口；安裝 tracing（含錯誤紀錄 layer），解析 CLI，無參數時啟動 UI。
- `src/cli.rs`：`--version`、`--help`、`--config-path`、`--self-check`（手寫解析，無額外相依）。
- `src/lib.rs`：crate 根；`VERSION`、`APP_NAME`、模組宣告，include Slint 產生的模組。
- `src/app.rs`：`AppState`（錄音狀態、目前模板與模型、設定、模組註冊表、啟動警告）。
- `src/audio/`：`capture.rs` 麥克風（cpal）、`resampler.rs` 重採樣到 16 kHz、`vad.rs` 能量式靜音偵測；`mod.rs` 共用音訊常數。
- `src/whisper/`：`engine.rs` 推論、`params.rs` 完整 whisper 參數、`session.rs`。
- `src/models/`：`registry.rs` 模型目錄（內嵌 + 外部覆寫）、`downloader.rs` 串流下載（備用網址；依校驗值長度選 SHA-1/SHA-256）、`manager.rs` 本機模型掃描。
- `src/transcription/`：`pipeline.rs` `TranscriptionPipeline`（`for_live` 供即時錄音）、`opencc.rs`、`postprocess.rs`。
- `src/context_templates/`：`templates.rs` `TemplateRegistry`（`templates/*.toml` 以 `include_str!` 內嵌，可由 `<exe>/templates/` 與 `<config>/whisper-typeless/templates/` 覆寫；以穩定 id 為鍵）、`detector.rs` 加權關鍵字偵測、`custom.rs`。
- `src/output/`：`OutputRouter` 與 `OutputMode`；剪貼簿與鍵盤藏在 `ClipboardSink` / `KeyboardSink` trait 後（測試用 mock）；隱私模式清除剪貼簿在 router 內處理。
- `src/settings/`：`config.rs` `AppConfig`（唯一持久化設定，`config.toml`）、`persistence.rs`（損毀檔備份為 `.bak`、原子寫入）、`migration.rs`（舊 `settings.json` 一次性匯入）、`ui_settings.rs`（僅記憶體中的 UI 快照：`from_config` / `apply_to`）、`mod.rs` 模組開關。
- `src/file_transcription/`：音檔解碼（symphonia）與 TXT / SRT / VTT 匯出。
- `src/ui/`：`mod.rs` 啟動 UI 與 `ui_update!` 巨集、`error_log.rs` WARN/ERROR 環狀緩衝（200 筆）推送到 UI、`window_icon.rs`。
- `src/ui/bridge/`：`mod.rs` callback 接線與設定同步、`window.rs` 視窗模式/置頂/拖曳/錯誤紀錄、`model.rs` 模型選擇與下載、`recording/`（`mod.rs` 錄音控制、`pipeline.rs` 錄音執行緒、`transcribe.rs` 轉錄→pipeline→router、`io.rs` WAV）、`hotkey.rs` 全域快捷鍵、`ptt.rs` 按住說話、`templates.rs`、`file_mode.rs`。
- `ui/appwindow.slint`：主視窗、鍵盤（Esc / Space / E）、完整與浮動視窗切換、錯誤紀錄覆蓋層。
- `ui/components/`：`compact_bar`、`quick_actions`、`context_bar`、`model_picker`、`settings_panel`（含隱私模式）、`file_mode`、`transcript`、`waveform`、`toggle`。
- `ui/styles/tokens.slint`：設計 token。
- `templates/*.toml`：內建情境模板。
- `tests/`：整合測試。
- `scripts/dev.ps1`：`check|test|lint|fmt|build|release|clean|doctor` [-Cuda]；`scripts/setup-env.ps1`（CUDA 環境）、`scripts/download-model.ps1`。
- `.github/workflows/ci.yml`（fmt、clippy、test、release build）與 `release.yml`（push `v*` tag → zip + SHA256SUMS + 自動 release notes）。

## 開發規則

- **測試不得依賴音訊硬體、GPU、網路或模型檔案。** 以純邏輯、假資料或 fixture 測試；需要實機驗證的項目寫在 PR 說明中。
- **提交前 `cargo fmt --all -- --check` 與 `cargo clippy --all-targets -- -D warnings` 必須通過。**
- **使用者可見字串（UI 文字、對話框、狀態列、錯誤訊息）使用 zh-TW。** 程式註解可用繁體中文。
- **不要記錄語音或轉錄內容**（包含 info 層級日誌）；不要新增網路呼叫或遙測。
- 模型網址與校驗值只改 `model-catalog.json`，不要寫進 Rust 或 PowerShell 程式碼。
- 新增 crate 依賴需有明確理由，並一併更新 `Cargo.lock`。
- 不要提交執行期產物：`config.toml`、`settings.json*`、`models/`、錄音 WAV、`target/`、任何含憑證的檔案。
- 文件放在 `docs/`（規格、架構、路線圖）；不要新增臨時報告類 Markdown。

## 發版流程

1. 修改 `Cargo.toml` 的 `version`（並執行一次 `cargo check` 讓 `Cargo.lock` 同步）。
2. 在 `CHANGELOG.md` 新增 `## [X.Y.Z] - YYYY-MM-DD` 區段（Keep a Changelog 格式）。
3. 確認 CI 在 `main` 上通過後合併。
4. 建立並推送 tag：`git tag vX.Y.Z`、`git push origin vX.Y.Z`。
5. `.github/workflows/release.yml` 會建置 `whisper-typeless-vX.Y.Z-windows-x86_64-cpu.zip`、產生 `SHA256SUMS`，並以自動產生的內容建立 Release 說明。

只有在使用者明確要求時才建立或推送 tag。
