# whisper-typeless

`whisper-typeless` 是一個 Windows 桌面語音轉文字工具：按住快捷鍵說話，辨識結果自動輸入到目前焦點所在的視窗。語音辨識使用 whisper.cpp 在你的電腦上離線完成。

> English summary: [jump to English](#english)

**文件：** [使用規格](docs/spec.md) · [架構說明](docs/architecture.md) · [路線圖](docs/roadmap.md) · [模型目錄](MODEL_CATALOG.md) · [更新紀錄](CHANGELOG.md) · [AI 代理指引](AGENTS.md) · [授權](LICENSE)

---

## 繁體中文

### 這是什麼

whisper-typeless 把你的語音即時轉成文字，並貼到你正在輸入的地方（編輯器、聊天視窗、瀏覽器輸入框等）。

- **本機運作**：語音與文字不會離開你的電腦。唯一的對外連線是下載模型檔案。
- **中文優先**：預設將 Whisper 的簡體輸出轉為台灣繁體（zh-TW）。
- **單一執行檔**：主程式是一個 exe，模型檔另外放置；CUDA 加速為選用。

### 介面

- 預設介面為 **AcmeUI-Native**（Rust 原生 GUI）：小型浮動視窗，預設以 CPU 軟體渲染（小視窗不需要 GPU）。
- 環境變數 `ACME_RENDER_BACKEND` 可覆寫渲染後端：`cpu`、`gpu`、`auto`（不分大小寫）。未設定或留空時使用預設的 `cpu`；設成無法辨識的值時會記錄警告並改用 `auto`。
- 本版保留舊的 **Slint** 介面作為備援，以 `whisper-typeless.exe --ui slint` 啟動。Slint 介面將於下一版移除。

### 核心功能

- **離線辨識**：使用 Whisper 模型（whisper.cpp），可在設定中選擇模型大小。
- **自動輸入焦點視窗**：結果寫入剪貼簿，並可自動貼入目前視窗（可改為只複製或只貼上）。
- **全域快捷鍵與按住說話**：不需切換視窗即可開始／停止錄音。
- **情境模板**：會議記錄、口語對話、技術討論、醫療紀錄、法律文書五種內建模板，各自提供辨識提示詞與後處理設定；可用 TOML 自訂。
- **即時波形與錯誤紀錄**：浮動視窗顯示音量波形，錯誤集中在紀錄面板。
- **隱私模式**：不保留錄音檔，輸出後清除剪貼簿。
- **檔案轉錄**：轉錄 WAV、MP3、FLAC、OGG、M4A 等音檔，並匯出 TXT、SRT、VTT。

### 下載與安裝

1. 到 [GitHub Releases](https://github.com/stevenke1981/whisper-typeless/releases) 下載 `whisper-typeless-vX.Y.Z-windows-x86_64-cpu.zip`。
2. 解壓縮到任一資料夾。
3. 若啟動時出現缺少 `VCRUNTIME140.dll` 或 `MSVCP140.dll` 的錯誤，請安裝 [Microsoft Visual C++ Redistributable（x64）](https://aka.ms/vs/17/release/vc_redist.x64.exe)。
4. 執行 `whisper-typeless.exe`。

系統需求：Windows 10/11（64 位元）、一個麥克風。Release 提供兩種 zip：`-cpu`（不需要任何 GPU 執行時）與 `-vulkan`（Vulkan GPU 加速，支援 NVIDIA / AMD / Intel 顯示卡，需要顯示卡驅動內建的 Vulkan 執行環境）。建議優先使用 Tiny / Base / Small 小型模型。需要 NVIDIA CUDA 加速時，請從原始碼建置（見下方）。

### 首次啟動與模型下載

辨識需要一個 Whisper 模型。若目前選擇的模型尚未下載，應用程式會跳出確認視窗，詢問是否自動下載（會顯示大小）。下載完成後會自動選用並預先載入。

- 模型清單定義在 `model-catalog.json`，說明見 [MODEL_CATALOG.md](MODEL_CATALOG.md)。
- 模型儲存在執行檔旁的 `models\` 資料夾。
- 模型越大越準確，但越慢、占用越多記憶體與 VRAM。中文語音建議至少使用 `small`。

### 快捷鍵

| 操作 | 預設按鍵 | 說明 |
|------|----------|------|
| 開始／停止錄音（切換） | `Ctrl+Shift+W` | 全域快捷鍵，僅 Windows；可在設定中修改 |
| 按住說話（PTT） | `Ctrl+Alt+L` | 按住開始錄音，放開停止並送出辨識 |
| 收合 ／ 關閉 | `Esc` | 依序關閉設定面板、錯誤紀錄，或收合展開的浮動視窗 |
| 開始／停止錄音 | `Space` | 視窗取得焦點且未在輸入框打字時 |
| 展開／收合浮動視窗 | `E` | 僅浮動視窗模式 |

### 輸出模式

在設定的「輸出」區塊選擇：

- 只寫入剪貼簿
- 只注入焦點視窗（自動 Ctrl+V）
- 剪貼簿與注入焦點視窗（預設）

注入是模擬按鍵貼上，因此無法送入以系統管理員身分執行的視窗，除非本程式也以系統管理員身分執行。

### 隱私模式

在設定的「輸出」區塊開啟「隱私模式」，或在設定檔中寫入：

```toml
[output]
privacy_mode = true
```

開啟後：

- 不會把錄音儲存為 WAV 檔案。
- 輸出完成後清除剪貼簿內容。

辨識文字仍會顯示在視窗中，可使用清除按鈕清空。隱私模式開啟時一律使用本機轉錄，不會使用 OpenAI 雲端轉錄。

### OpenAI 即時轉錄（選用）

預設只用本機 whisper，語音不離開電腦。若想改用 OpenAI 的即時轉錄（Realtime API）：

1. 開啟設定的「雲端轉錄」區段，引擎選「OpenAI 雲端」。
2. 勾選同意：錄音期間麥克風音訊會即時傳送到 OpenAI。
3. 輸入 API 金鑰後按「儲存」。金鑰以 Windows DPAPI 加密存在 `%APPDATA%\whisper-typeless\openai.key`，不會寫入 `config.toml` 或日誌；也可改用環境變數 `OPENAI_API_KEY`。
4. 模型可選 `gpt-live-transcribe`（預設，邊說邊出字）或 `gpt-transcribe`。

錄音時會即時顯示文字，每段話結束（靜音偵測）就輸出一句，並同樣轉為繁體中文。雲端轉錄失敗而且還沒有輸出時，若已下載本機模型，會自動改用本機轉錄。使用 OpenAI 會依其價格計費。

### 系統匣

程式會在系統匣顯示圖示（錄音中變紅）。左鍵點一下顯示／隱藏主視窗；右鍵選單有「顯示主視窗」「開始／停止錄音」「設定」「結束」。預設按下關閉鈕會縮小到系統匣而不是結束，要結束請用右鍵選單的「結束」。可在設定的「系統匣」區段調整：關閉時縮小、最小化時縮小、啟動時縮小到系統匣。

### 情境模板

內建模板：會議記錄（`meeting`）、口語對話（`casual`）、技術討論（`technical`）、醫療紀錄（`medical`）、法律文書（`legal`）。每個模板會提供初始提示詞（幫助 Whisper 辨識專有名詞）與後處理規則（例如移除贅字、保留英文術語）。

自訂模板是 TOML 檔案，放在下列任一位置（同 id 者以後者覆寫內建）：

- 執行檔旁的 `templates\` 資料夾
- 設定目錄下的 `%APPDATA%\whisper-typeless\templates\`

範例：

```toml
[template]
id = "meeting"
name_zh = "會議記錄"
name_en = "Meeting"
icon = "🏢"

[whisper]
initial_prompt = "這是一段商業會議的記錄，包含專業術語、人名和公司名稱。"
language = "zh"
temperature = 0.0

[postprocess]
formal_style = true
remove_fillers = ["嗯", "啊", "那個"]
```

### 命令列參數

| 參數 | 說明 |
|------|------|
| （無參數） | 啟動 AcmeUI 介面（預設） |
| `--ui acme` / `--ui slint` | 指定介面：`acme`（預設）或 `slint`（本版保留的備援）；`--acme-shell` 為 `--ui acme` 的舊別名 |
| `--version` | 顯示版本後結束 |
| `--help` | 顯示用法後結束 |
| `--config-path` | 印出設定檔路徑後結束 |
| `--self-check` | 不開視窗檢查模型清單、情境範本、設定檔與 OpenCC；全部通過結束碼為 0，否則為 1 |

### 設定檔

設定檔位置：

```
%APPDATA%\whisper-typeless\config.toml
```

即 `dirs::config_dir()` 下的 `whisper-typeless\config.toml`。執行 `whisper-typeless.exe --config-path` 可確認實際路徑。

設定檔中的主要區段：`[general]`、`[audio]`、`[whisper]`、`[chinese]`、`[output]`、`[context]`、`[history]`、`[modules]`。

- 舊版會在執行目錄產生 `settings.json`；0.3.0 起會自動遷移到 `config.toml`。
- 若 `config.toml` 無法解析，會先備份為舊檔再以預設值啟動，不會直接覆蓋。

### 疑難排解

**啟動時出現 VCRUNTIME140.dll 或 MSVCP140.dll 錯誤**
安裝 [Microsoft Visual C++ Redistributable（x64）](https://aka.ms/vs/17/release/vc_redist.x64.exe)。

**啟動後沒有視窗，或出現找不到 `cudart64_12.dll` 的錯誤**
這是 CUDA 建置缺少 NVIDIA CUDA runtime。可擇一處理：改用 Releases 的 CPU 版 zip（檔名含 `-cpu`）；或安裝 CUDA Toolkit 12.x。

**模型下載失敗或校驗失敗**
應用內下載會依 `model-catalog.json` 的校驗值（SHA-1 或 SHA-256）驗證檔案，並自動改用備用來源。若仍失敗，可在專案目錄執行 `.\scripts\download-model.ps1`，或從 [Hugging Face 的 whisper.cpp 模型頁面](https://huggingface.co/ggerganov/whisper.cpp) 手動下載 `ggml-*.bin`，放入執行檔旁的 `models\` 資料夾後重新啟動。

**辨識結果沒有貼到目標視窗**
- 確認設定的輸出模式包含「注入焦點視窗」。
- 確認游標仍在目標輸入框內；注入發生在辨識完成時。
- 目標視窗若以系統管理員身分執行，本程式需要以相同權限執行才能注入。

**按住說話（PTT）沒有反應**
快捷鍵可能與其他程式衝突。請在設定中改用其他組合。

**錄音很久不會停止，或句子被切斷**
調整設定中的 VAD 閾值與靜音超時。

**想看詳細記錄**
以原始碼執行時可設定 `RUST_LOG=whisper_typeless=debug`（`cargo run`）。Release 版隱藏主控台，請改看介面上的錯誤紀錄（⚠ 按鈕）。

### 從原始碼建置

需要：

- Rust（stable），建議使用 MSVC 工具鏈（`x86_64-pc-windows-msvc`）
- Visual Studio Build Tools（C++ 桌面開發工作負載）
- CMake
- LLVM / libclang，供 whisper-rs-sys 的 bindgen 使用；請設定 `LIBCLANG_PATH` 指向含 `libclang.dll` 的資料夾
- CUDA Toolkit 12.x（僅 CUDA 建置需要）

使用腳本（建議）：

```powershell
.\scripts\dev.ps1 check      # cargo check --all-targets
.\scripts\dev.ps1 test       # 執行測試
.\scripts\dev.ps1 lint       # clippy（-D warnings）與 fmt 檢查
.\scripts\dev.ps1 build      # CPU 版 release 建置
.\scripts\dev.ps1 build -Cuda  # 加入 CUDA 支援（需要 CUDA Toolkit）
.\scripts\dev.ps1 build -Vulkan # 加入 Vulkan 支援（需要 Vulkan SDK，並設定 VULKAN_SDK）
```

AcmeUI-Native 相依套件以固定 git rev 引用 `github.com/stevenke1981/AcmeUI-Native`，首次建置需要能連到 GitHub。

直接使用 cargo：

```powershell
# Cargo.toml 的預設 feature 含 cuda，CPU 版必須加上 --no-default-features
$env:LIBCLANG_PATH = "<libclang 所在資料夾>"
cargo build --release --no-default-features --target x86_64-pc-windows-msvc
```

若你的預設工具鏈是 GNU（`x86_64-pc-windows-gnu`），無法建置 whisper-rs-sys，請務必加上 `--target x86_64-pc-windows-msvc`。輸出檔位於 `target\x86_64-pc-windows-msvc\release\whisper-typeless.exe`。

### 文件

- [docs/spec.md](docs/spec.md)：初版技術規格（設計草案，以程式碼為準）
- [docs/architecture.md](docs/architecture.md)：模組關係與資料流
- [docs/roadmap.md](docs/roadmap.md)：未完成項目與已知問題
- [MODEL_CATALOG.md](MODEL_CATALOG.md)：模型目錄格式
- [CHANGELOG.md](CHANGELOG.md)：版本變更紀錄
- [AGENTS.md](AGENTS.md)：給 AI 程式代理的開發指引

### 授權

本專案以 [MIT 授權](LICENSE) 釋出。Copyright (c) 2026 whisper-typeless contributors.

---

<a id="english"></a>

## English

**whisper-typeless** is a Windows desktop speech-to-text tool. Hold a hotkey, speak, and the transcript is typed into whichever window has focus. Recognition runs fully offline with whisper.cpp.

- **Local-first**: by default audio and text never leave your machine; the only network access is model download. Optional OpenAI realtime transcription (`gpt-live-transcribe` / `gpt-transcribe`) can be enabled in Settings with explicit consent; the API key is stored encrypted with Windows DPAPI (or read from `OPENAI_API_KEY`). Privacy mode always forces local transcription.
- **System tray**: closing the window hides it to the tray by default; left-click the tray icon to show/hide, right-click for record/settings/quit.
- **Chinese-first**: Simplified output from Whisper is converted to Traditional Chinese (zh-TW) by default (OpenCC). zh-HK, zh-CN and raw modes are also available.
- **Context templates**: built-in templates for meetings, casual talk, technical discussion, medical and legal use. Override or add templates with TOML files.
- **Keyboard-first floating UI** built with AcmeUI-Native (Rust-native, CPU software rendering by default), optional always-on-top, light and dark themes. The previous Slint UI stays available as a fallback via `--ui slint` for this release.
- **Privacy mode** skips saving WAV files and clears the clipboard after output.
- **Single small exe**; CUDA acceleration is optional and built from source.

**Install:** download `whisper-typeless-vX.Y.Z-windows-x86_64-cpu.zip` from [GitHub Releases](https://github.com/stevenke1981/whisper-typeless/releases), unzip, and install the [Visual C++ Redistributable (x64)](https://aka.ms/vs/17/release/vc_redist.x64.exe) if you see missing VCRUNTIME140.dll or MSVCP140.dll errors. The release zip is CPU-only; if the window does not appear and you see a missing `cudart64_12.dll` error, you are running a CUDA build without the NVIDIA CUDA runtime. Use the CPU zip, or install CUDA Toolkit 12.

**First run:** pick a model when prompted. Models are downloaded on demand into the `models` folder next to the exe. Downloads are verified against the catalog checksum (SHA-1 or SHA-256).

**Hotkeys (defaults):** `Ctrl+Shift+W` toggles recording (global, Windows only); `Ctrl+Alt+L` is push-to-talk; `Esc` closes panels or collapses the floating window.

**Config:** `%APPDATA%\whisper-typeless\config.toml` (run `whisper-typeless.exe --config-path` to confirm). CLI flags: `--version`, `--help`, `--config-path`, `--self-check`, `--ui acme|slint` (default `acme`). The AcmeUI renderer defaults to CPU; override with `ACME_RENDER_BACKEND=cpu|gpu|auto`.

**Build from source:** Rust (MSVC toolchain), Visual Studio Build Tools, CMake and libclang (set `LIBCLANG_PATH`). Run `.\scripts\dev.ps1 build` for CPU, `.\scripts\dev.ps1 build -Vulkan` with the Vulkan SDK (`VULKAN_SDK` set, `glslc` required), or `.\scripts\dev.ps1 build -Cuda` with the CUDA Toolkit. Releases ship `-cpu` and `-vulkan` zips; the Vulkan build delay-loads `vulkan-1.dll` and falls back to CPU if GPU initialisation fails (toggle in Settings, takes effect on next model load). AcmeUI-Native dependencies are fetched from GitHub at a pinned revision, so the first build needs network access. See the Chinese section for raw cargo commands.

**License:** MIT. See [LICENSE](LICENSE).
