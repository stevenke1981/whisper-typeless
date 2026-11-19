# AGENTS.md

給 AI 程式代理（coding agents）的開發指引。人類版概覽見 [README.md](README.md)，規格見 [docs/spec.md](docs/spec.md)，模組關係見 [docs/architecture.md](docs/architecture.md)。

## 專案目的

whisper-typeless：Windows 桌面即時語音轉文字工具。Rust + AcmeUI-Native（預設 UI）+ Slint（本版備援）+ whisper-rs（whisper.cpp 綁定）。辨識結果自動輸入到目前焦點視窗。

## 核心價值（修改時不可倒退）

1. **本機隱私優先**：預設引擎為本機 whisper，語音與文字不離開電腦。唯一的例外是使用者在設定中選擇 OpenAI 引擎並明確同意（`[transcription] openai_consent = true`）後，語音會送往 OpenAI 轉錄；其餘唯一的對外網路行為是下載模型（URL 來自 `model-catalog.json`）。API 金鑰以 Windows DPAPI 加密存在使用者設定資料夾（`%APPDATA%/whisper-typeless/openai.key`），或由環境變數 `OPENAI_API_KEY` 提供；程式內以 `Secret` 型別傳遞（`Debug` 已遮蔽），絕不寫入 config.toml、日誌或錯誤紀錄。隱私模式開啟時一律本機轉錄；檔案模式一律本機。
2. **結果落在焦點視窗**：辨識後寫入剪貼簿並模擬貼上（enigo / SendInput）。
3. **中文優先**：預設 OpenCC `zh-TW`；另有 `zh-HK`、`zh-CN`、`raw`。
4. **情境模板**：內建 `TemplateId`（會議、口語、技術、醫療、法律）與使用者 TOML 覆寫；控制初始提示詞與後處理。
5. **鍵盤優先的浮動 UI**：Liquid Glass 風格；收合 380x52、展開 380x540。
6. **模型目錄**：`model-catalog.json` 是模型清單、下載網址與校驗值的唯一來源；程式碼與腳本都從它讀取。
7. **單一小型 exe**：CUDA 為選用（Cargo feature `cuda`）。

## 架構規則（UI 與核心）

- **畫面層只做兩件事**：讀取 `AppViewState` 把畫面畫出來，以及把使用者操作轉成 `UiCommand` 送進 `Controller::handle`。不要在畫面層直接呼叫錄音、模型、剪貼簿或 Backend。
- **背景執行緒只透過 `Notifier` 通知畫面**：`Notifier::send` 送出 `CoreEvent` 並喚醒畫面執行緒；畫面在自己的執行緒上呼叫 `queue.drain()` → `Controller::apply`。背景執行緒不得直接改動 UI 狀態。
- **熱鍵與按住說話（PTT）路徑不得取得焦點**：處理 `CoreEvent::HotkeyPressed` / `PttKey` 時，不可顯示、啟用或聚焦視窗。AcmeUI 版只送 `SWP_NOACTIVATE` 的尺寸與置頂請求，不送 `WindowRequest::Focus`；辨識結果必須落在使用者原本的焦點視窗。
- **新的 UI 行為先加型別**：需要畫面新增的操作，先加到 `UiCommand`、`ViewRequest` 或 `AppViewState`，再由兩個畫面層各自實作。
- **AcmeUI 相依是固定 git rev**：`Cargo.toml` 的 `acme-core`、`acme-platform`、`acme-text`、`acme-textinput` 與 dev-dependency `acme-render-cpu` 皆指向 `c2e0fd9765d0f25119a240ad1adb378c045df63f`。升級 rev 須刻意為之：四個 crate 一起升、跑完整檢查，並記入 CHANGELOG。不要為了解決其他問題而執行 `cargo update`。
- **保留 `Cargo.lock` 中的 `gpu-allocator 0.28.0` → `windows 0.62.2`**：重新解析可能把 `windows` 降到 0.54，導致 wgpu-hal 編譯失敗。Cargo.lock 變動後，確認此條目仍在。
- AcmeUI-Native 的修改在該上游專案進行，不在本專案內修補。

## 建置、測試、檢查

Windows PowerShell。優先使用腳本：

```powershell
.\scripts\dev.ps1 check        # cargo check --all-targets
.\scripts\dev.ps1 test         # cargo test
.\scripts\dev.ps1 lint         # clippy -D warnings + fmt --check
.\scripts\dev.ps1 build        # release，CPU
.\scripts\dev.ps1 build -Cuda  # release，含 CUDA（需要 CUDA Toolkit）
.\scripts\dev.ps1 build -Vulkan # release，含 Vulkan（需要 Vulkan SDK 與 VULKAN_SDK）
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

## UI 快照（無視窗）

設計 / 實作者可不開視窗檢視 AcmeUI 畫面。測試 `ui_snapshots`（`src/ui_acme/snapshots.rs`，預設 `#[ignore]`）以與正式視窗相同的 view 函式與字型，用 CPU 渲染器把浮動列、即時 / 檔案 / 設定（逐段捲動）/ 模板編輯 / 錯誤紀錄 / 深色等畫面存成 PNG（1.5x 與 1.0x；內建 PNG 編碼器，無新增相依）：

```powershell
$env:SNAPSHOT_DIR = "C:/tmp/snaps"
cargo test --no-default-features --target x86_64-pc-windows-msvc --lib ui_snapshots -- --ignored
```

輸出檔名如 `01_overlay_idle@1.5x.png`，清單在 `index.txt`。

## 模組地圖

- `src/main.rs`：入口；安裝 tracing（含錯誤紀錄 layer），解析 CLI，無參數時啟動預設 GUI（AcmeUI）。
- `src/cli.rs`：`--version`、`--help`、`--config-path`、`--self-check`、`--ui acme|slint`（`--acme-shell` 為 `--ui acme` 的別名）；手寫解析，無額外相依。
- `src/lib.rs`：crate 根；`VERSION`、`APP_NAME`、模組宣告。
- `src/app.rs`：`AppState`（錄音狀態、目前模板與模型、設定、模組註冊表、啟動警告）；兩個畫面層共用的起點。
- `src/core/`：與 UI 框架無關的應用邏輯與畫面狀態。**不得引用 Slint 或 acme-* crate。**
  - `state.rs`：`AppViewState`（畫面唯一讀取的狀態）與清單列型別。
  - `command.rs`：`UiCommand`（畫面 → 核心）、`ViewRequest`（核心 → 畫面：調整尺寸、隱藏、拖曳）。
  - `event.rs`：`CoreEvent`（背景 → 核心）。
  - `controller.rs`：`Controller`（`handle` / `apply` / `state` / `state_mut`）、`Notifier`、`EventQueue`、`event_channel`。
  - `keys.rs`：`key_to_command`（Esc / Space / E）。
  - `backend.rs`：`Backend` trait（副作用介面，測試用假實作）。
  - `services/`：`mod.rs`（`RealBackend`、`initial_state`）；`recording/`（`mod.rs` 控制、`pipeline.rs` 錄音執行緒、`transcribe.rs`、`io.rs`）、`model.rs`、`file_mode.rs`、`hotkey.rs`、`ptt.rs`、`templates.rs`。
  - `tests.rs`：核心單元測試（假 Backend）。
- `src/ui_acme/`：預設畫面層（AcmeUI-Native），只是核心的薄轉接層。
  - `app.rs`：`AcmeApp`（`acme_platform::Application`），擁有 `Controller`，把指標 / 鍵盤 / 輸入法事件轉成 `UiCommand`；`render_backend()` 回傳 `Cpu`（可由環境變數 `ACME_RENDER_BACKEND` 覆寫）。`app_tests.rs` 為對應測試。
  - `views/`：`AppViewState` → 描述樹的純函式（`live.rs` 緊湊 / 即時、`file.rs` 檔案模式、`settings/` 設定（分頁框架與標題列在 `mod.rs`，每頁一檔：`general.rs` 一般、`engine.rs` 辨識、`models.rs` 模型、`output.rs` 輸出、`templates.rs` 模板、`advanced.rs` 進階；`tests.rs` 設定測試）、`overlay.rs` 錯誤紀錄浮層、`onboarding.rs` 缺模型 / 下載中狀態卡、`widgets.rs` 共用元件與 v0.6 tokens；`mod.rs` 另有 `Phase` / `phase` / `cloud_ready` / `needs_model`）。
  - `paint.rs`：描述樹 → `Painter` 指令與 `HitTestLayer` 命中區域（`HitMap`）。
  - `controls.rs`：滑桿 / 步進器數學、acme-textinput 文字框（含輸入法組字）。
  - `theme.rs`：色票（取自 `ui/styles/tokens.slint`）與 `Tone` / `Theme::tone`；`icons.rs`：單色向量圖示（`Icon`、`icon`）；`text.rs`：zh-TW 文字塑形、換行、省略號。
  - `local.rs`：只屬於畫面層的狀態（捲動、焦點、選取）；`msg.rs`：命中區域送出的 `Msg`。
  - `hwnd.rs`：Win32 縮放 / 置頂，一律 `SWP_NOACTIVATE`。
  - `snapshots.rs`：`ui_snapshots` 無視窗快照測試（由 `app.rs` 載入，見上方「UI 快照」）。
- `src/ui/`：Slint 備援轉接層（`--ui slint`，本版保留，下一版移除）。
  - `mod.rs`：`run` 與 `ui_update!` 巨集；`bridge/mod.rs`：callback 接線；`bridge/sync.rs`：設定與狀態同步；`bridge/window.rs`：視窗模式、置頂、拖曳。
  - `error_log.rs`：WARN/ERROR 環狀緩衝（200 筆）；`window_icon.rs`：工作列 / 標題列圖示（依錄音狀態切換）。
  - `ui/appwindow.slint`、`ui/components/`、`ui/styles/tokens.slint` 與根目錄 `build.rs`（slint-build）。
- `src/audio/`：`capture.rs` 麥克風（cpal）、`resampler.rs` 重採樣到 16 kHz、`vad.rs` 能量式靜音偵測；`mod.rs` 共用音訊常數。
- `src/whisper/`：`engine.rs` 推論、`params.rs` 完整 whisper 參數、`session.rs`。
- `src/cloud/openai/`：OpenAI Realtime 轉錄（需使用者同意）。`protocol.rs` 純函式（session.update、PCM16 封包、事件解析、依 commit 順序釋出的重排緩衝）、`session.rs`（`CloudSession`：送音訊、本機 VAD 段落 commit、停止時最多等 10 秒）、`transport.rs`（`Transport` trait 與 tokio-tungstenite WebSocket 薄轉接層）。金鑰存放區 `KeyStore`（DPAPI）在 `src/settings/secret.rs`。
- `src/models/`：`registry.rs` 模型目錄（內嵌 + 外部覆寫）、`downloader.rs` 串流下載（備用網址；依校驗值長度選 SHA-1/SHA-256）、`manager.rs` 本機模型掃描。
- `src/transcription/`：`pipeline.rs` `TranscriptionPipeline`（`for_live` 供即時錄音）、`opencc.rs`、`postprocess.rs`。
- `src/context_templates/`：`templates.rs` `TemplateRegistry`（`templates/*.toml` 以 `include_str!` 內嵌，可由 `<exe>/templates/` 與 `<config>/whisper-typeless/templates/` 覆寫；以穩定 id 為鍵）、`detector.rs` 加權關鍵字偵測、`custom.rs`。
- `src/output/`：`OutputRouter` 與 `OutputMode`；剪貼簿與鍵盤藏在 `ClipboardSink` / `KeyboardSink` trait 後（測試用 mock）；隱私模式清除剪貼簿在 router 內處理。
- `src/settings/`：`config.rs` `AppConfig`（唯一持久化設定，`config.toml`）、`persistence.rs`（損毀檔備份為 `.bak`、原子寫入）、`migration.rs`（舊 `settings.json` 一次性匯入）、`ui_settings.rs`（僅記憶體中的 UI 快照：`from_config` / `apply_to`）、`mod.rs` 模組開關。
- `src/file_transcription/`：音檔解碼（symphonia）與 TXT / SRT / VTT 匯出。
- `templates/*.toml`：內建情境模板。
- `tests/`：整合測試。
- `scripts/dev.ps1`：`check|test|lint|fmt|build|release|clean|doctor` [-Cuda]；`scripts/setup-env.ps1`（CUDA 環境）、`scripts/download-model.ps1`。
- `.github/workflows/ci.yml`（fmt、clippy、test、release build）與 `release.yml`（push `v*` tag → zip + SHA256SUMS + 自動 release notes）。

## 開發規則

- **測試不得依賴音訊硬體、GPU、網路或模型檔案。** 以純邏輯、假資料或 fixture 測試；需要實機驗證的項目寫在 PR 說明中。
- **提交前 `cargo fmt --all -- --check` 與 `cargo clippy --all-targets -- -D warnings` 必須通過。**
- **使用者可見字串（UI 文字、對話框、狀態列、錯誤訊息）使用 zh-TW。** 程式註解可用繁體中文。
- **不要記錄語音、轉錄內容或 API 金鑰**（包含 info 層級日誌）；不要新增遙測，也不要新增模型下載與（經同意的）OpenAI 轉錄以外的網路呼叫。
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
