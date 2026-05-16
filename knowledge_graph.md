# whisper-typeless — 知識圖譜

## 節點類型說明
- **[模組]** 可開關的功能單元
- **[組件]** 不可分割的核心組件
- **[資料]** 資料結構 / 狀態
- **[外部]** 第三方依賴
- **[UI]** 介面元素

---

## 1. 頂層架構圖

```
┌─────────────────────────────────────────────────────────┐
│                     whisper-typeless                    │
│                                                         │
│  ┌──────────┐    ┌──────────┐    ┌──────────────────┐  │
│  │  Audio   │───▶│ Whisper  │───▶│  Transcription   │  │
│  │  Layer   │    │  Engine  │    │    Pipeline      │  │
│  └──────────┘    └──────────┘    └────────┬─────────┘  │
│                                           │             │
│  ┌──────────┐    ┌──────────┐    ┌────────▼─────────┐  │
│  │  Slint   │◀──▶│ AppState │◀───│  Output System   │  │
│  │   GUI    │    │          │    │                  │  │
│  └──────────┘    └──────────┘    └──────────────────┘  │
│                       │                                 │
│              ┌────────▼────────┐                        │
│              │    Settings     │                        │
│              │   & Modules     │                        │
│              └─────────────────┘                        │
└─────────────────────────────────────────────────────────┘
```

---

## 2. 節點關係圖

### 2.1 Audio Layer

```
[外部] cpal
    │
    ▼
[組件] audio::capture::AudioCapture
    │ 輸出: Vec<f32> @ 系統取樣率
    │
    ▼
[組件] audio::resampler::Resampler     [外部] rubato
    │ 輸出: Vec<f32> @ 16000 Hz
    │
    ▼
[模組] audio::vad::VoiceActivityDetector
    │ 輸出: VadEvent { Speaking / Silence / Segment(Vec<f32>) }
    │ 設定: vad_threshold, silence_timeout_ms
    │
    ▼
[資料] AudioSegment { samples: Vec<f32>, duration_ms: u64 }
```

### 2.2 Whisper Engine

```
[外部] whisper-rs (whisper.cpp bindings)
    │
    ▼
[組件] models::manager::ModelManager
    │ ├── scan_local() → Vec<ModelInfo>
    │ ├── load(model_id) → WhisperContext
    │ └── unload()
    │
    ▼
[組件] models::downloader::ModelDownloader    [外部] reqwest
    │ ├── download(model_id, progress_cb)
    │ ├── verify_sha256(path)
    │ └── resume_download(url, offset)
    │
[組件] whisper::engine::WhisperEngine
    │ ├── 持有 WhisperContext
    │ ├── transcribe(audio: &AudioSegment, params: &WhisperParams) → TranscriptResult
    │ └── transcribe_stream(rx: Receiver<AudioSegment>) → impl Stream<TranscriptResult>
    │
    └─── 依賴 ─→ [資料] whisper::params::WhisperParams (全部 whisper.cpp 參數)
```

### 2.3 Transcription Pipeline

```
[資料] TranscriptResult { text: String, segments: Vec<Segment>, language: String }
    │
    ▼
[組件] transcription::pipeline::TranscriptionPipeline
    │ 串連下列處理器：
    │
    ├─→ [模組] transcription::opencc::OpenCCProcessor
    │       │ [外部] opencc-rs
    │       │ 模式: zh-TW / zh-HK / zh-CN / raw
    │       └─ 輸出: 轉換後文字
    │
    ├─→ [模組] transcription::postprocess::PostProcessor
    │       │ 功能: 標點正規化、全形轉換、填詞移除、重複偵測
    │       └─ 設定: remove_fillers, add_punctuation, formal_style
    │
    └─→ [模組] context_templates::context::ContextInjector
            │ 功能: 在推論前注入 initial_prompt
            └─ 依賴: ContextTemplate (當前模板)
```

### 2.4 Context Template System

```
[組件] context_templates::templates::TemplateRegistry
    │ ├── 內建模板: meeting / casual / technical / medical / legal
    │ ├── load_custom(path: &Path) → Template
    │ └── list_all() → Vec<Template>
    │
[模組] context_templates::detector::ContextDetector
    │ ├── analyze(text: &str) → TemplateId (信心分數最高的)
    │ └── 依賴: 關鍵詞字典 HashMap<TemplateId, Vec<String>>
    │
[組件] context_templates::custom::CustomTemplateEditor
    │ 讀寫: ~/.config/whisper-typeless/templates/*.toml
    └─ GUI 對應: TemplateEditorPanel
```

### 2.5 Output System

```
[資料] ProcessedText { text: String, metadata: OutputMetadata }
    │
    ▼
[組件] output::OutputRouter
    │ 根據 OutputMode 路由到：
    │
    ├─→ [模組] output::clipboard::ClipboardOutput
    │       │ [外部] arboard
    │       └─ write(text) → Result<()>
    │
    ├─→ [模組] output::injector::FocusedWindowInjector
    │       │ [外部] enigo
    │       │ Platform: Windows → Ctrl+V / macOS → Cmd+V
    │       └─ inject(text) → Result<()>
    │
    └─→ [模組] output::file_writer::FileWriter
            │ 追加模式寫入指定路徑
            └─ write(text, path) → Result<()>
```

### 2.6 Settings & Module System

```
[組件] settings::config::AppConfig
    │ 包含: GeneralConfig, AudioConfig, WhisperConfig, ChineseConfig,
    │       OutputConfig, ContextConfig, HistoryConfig, ModuleConfig
    │
[組件] settings::persistence::ConfigPersistence
    │ [外部] serde + toml
    │ 路徑: ~/.config/whisper-typeless/config.toml
    │
[組件] settings::module_registry::ModuleRegistry
    │ 持有所有 Box<dyn Module>
    │ ├── register(module)
    │ ├── enable(module_id)
    │ ├── disable(module_id)
    │ └── get_settings_schemas() → Vec<ModuleSettings>
    │
[資料] Module trait
    ├── name() / description()
    ├── is_enabled() / enable() / disable()
    └── settings_schema() → ModuleSettings
```

### 2.7 AppState (全局狀態)

```
[資料] AppState
    │
    ├── recording_state: RecordingState { Idle / Recording / Processing }
    ├── current_model: Option<ModelInfo>
    ├── current_template: TemplateId
    ├── transcript_history: VecDeque<TranscriptEntry>
    ├── audio_level: f32                    (波形顯示用)
    ├── config: Arc<RwLock<AppConfig>>
    └── module_registry: Arc<RwLock<ModuleRegistry>>
```

### 2.8 Slint GUI 元件樹

```
AppWindow (appwindow.slint)
    │
    ├── TitleBar
    │   └── WindowControls (最小化/最大化/關閉)
    │
    ├── ToolBar
    │   ├── ModelPicker (model_picker.slint)
    │   ├── LanguagePicker
    │   └── ThemeToggle
    │
    ├── ContextBar (context_bar.slint)
    │   └── TemplateButton × N
    │
    ├── WaveformView (waveform.slint)          [模組:waveform]
    │   └── AnimatedBars
    │
    ├── TranscriptView (transcript.slint)
    │   ├── ScrollView
    │   └── TextSegment × N (帶時間戳)
    │
    ├── ControlBar
    │   ├── RecordButton (狀態: Idle/Recording/Processing)
    │   ├── PauseButton
    │   ├── ClearButton
    │   └── OutputModeToggle
    │
    ├── StatusBar
    │   ├── CharCount
    │   ├── Latency
    │   └── ModelIndicator
    │
    └── SettingsDrawer (settings_panel.slint)  [側邊抽屜]
        ├── AudioSection
        ├── WhisperSection
        ├── ChineseSection
        ├── OutputSection
        ├── ModuleToggles
        └── ModelManagerButton → ModelManagerDialog
```

---

## 3. 資料流圖

```
麥克風硬體
    │ PCM samples
    ▼
AudioCapture (cpal)
    │ f32 @ 原始取樣率
    ▼
Resampler (rubato)
    │ f32 @ 16000Hz
    ▼
VoiceActivityDetector ←── vad_threshold (設定)
    │ AudioSegment (說話片段)
    ▼
WhisperEngine ←──────────── WhisperParams (設定)
    │              ←────── ContextTemplate.initial_prompt
    │ TranscriptResult { raw_text, language }
    ▼
OpenCCProcessor ←─────────── conversion_mode (設定)
    │ 轉換後文字
    ▼
PostProcessor ←───────────── postprocess 設定
    │ 最終文字
    ▼
OutputRouter ←────────────── OutputMode (設定)
    │
    ├──▶ Clipboard (arboard)
    ├──▶ FocusedWindowInjector (enigo)
    └──▶ FileWriter
              │
              ▼
         TranscriptHistory ──▶ UI TranscriptView
```

---

## 4. 狀態機

### 4.1 錄音狀態機

```
         ┌──────────────────────────────┐
         │                              │
    [Idle] ──按下錄音──▶ [Recording] ──VAD靜音──▶ [Processing]
         │                              │                │
         │◀──────────錄音停止────────────┘                │
         │                                               │
         │◀──────────────────轉錄完成──────────────────────┘
```

### 4.2 模型狀態機

```
[Unloaded] ──load()──▶ [Loading] ──成功──▶ [Ready]
                                   │
                                   └──失敗──▶ [Error]
                                                │
                                         [Unloaded]◀──retry
```

---

## 5. 模組依賴矩陣

| 模組 | 依賴模組 | 被依賴 |
|------|---------|--------|
| `vad` | `audio::capture` | `whisper::engine` |
| `context_templates` | (無) | `transcription::pipeline` |
| `opencc` | (無) | `transcription::pipeline` |
| `auto_inject` | `output::clipboard` | (無) |
| `waveform` | `audio::capture` | `ui::WaveformView` |
| `history` | `transcription::pipeline` | `ui::TranscriptView` |
| `noise_suppress` | `audio::capture` | `audio::resampler` |

---

## 6. 外部依賴關係圖

```
whisper-typeless
    │
    ├── [whisper-rs] ──▶ [whisper.cpp] ──▶ [GGML / CUDA / Metal]
    │
    ├── [slint] ──▶ 平台 OpenGL/Vulkan/Metal renderer
    │
    ├── [cpal] ──▶ WASAPI (Windows) / CoreAudio (macOS) / ALSA (Linux)
    │
    ├── [arboard] ──▶ Win32 Clipboard / NSPasteboard / XClipboard
    │
    ├── [enigo] ──▶ Win32 SendInput / CGEvent / xdotool
    │
    ├── [opencc-rs] ──▶ OpenCC C++ library (static link)
    │
    └── [reqwest + tokio] ──▶ TLS (rustls) ──▶ Hugging Face HTTPS
```

---

## 7. 錯誤傳播圖

```
AppError
    ├── AudioDevice ◀── cpal::BuildStreamError
    ├── ModelLoad ◀── std::io::Error
    ├── ModelDownload ◀── reqwest::Error
    ├── TranscriptionEngine ◀── whisper-rs error
    ├── Clipboard ◀── arboard::Error
    ├── Config ◀── toml::de::Error
    └── OpenCC ◀── opencc-rs error

所有錯誤 → AppState.last_error → UI StatusBar 顯示
嚴重錯誤 → ErrorDialog 彈出
```

---

## 8. 設定優先序

```
程式碼預設值
    ▲ 覆蓋
全域設定檔 ~/.config/whisper-typeless/config.toml
    ▲ 覆蓋
情境模板設定 (僅覆蓋 whisper + postprocess 相關)
    ▲ 覆蓋
使用者 session 臨時設定 (UI 即時調整，重啟不保留)
```

---

## 9. 國際化節點

```
[組件] i18n::Translator
    │ 支援: zh-TW (預設) / zh-CN / en
    │ 字串資源: ui/i18n/*.ftl (Fluent 格式)
    │
    └── GUI 所有文字 ──▶ translator.get(key)
```

---

## 10. 測試覆蓋矩陣

| 模組 | 單元測試 | 整合測試 | E2E |
|------|---------|---------|-----|
| audio::capture | mock cpal | — | — |
| audio::vad | 測試向量 | — | — |
| whisper::engine | mock context | 真實模型(CI skip) | — |
| models::downloader | mock HTTP | — | — |
| transcription::opencc | 字串對 | — | — |
| transcription::postprocess | 字串對 | — | — |
| output::clipboard | mock arboard | — | — |
| output::injector | mock enigo | — | — |
| settings | 讀寫循環 | 完整 config | — |
| context_templates | 模板解析 | 偵測準確度 | — |
| **全流程** | — | — | 錄音→輸出 |

---

## v2.0 UI 重設計新增節點（2026-05-16）

### 新節點
- **[UI] CompactBar** — 摺疊狀態頂部條（錄音 + mini waveform + partial text + 工具）
- **[UI] QuickActions** — 展開後 inline 操作列（chips：模板/模型/輸出/清除/錯誤）
- **[UI] ErrorLogOverlay** — 錯誤紀錄浮層
- **[資料] error-log: [string]** — 錯誤訊息陣列
- **[資料] expanded: bool** — 視窗摺疊狀態
- **[資料] pin-on-top: bool** — 永遠置頂旗標
- **[Glass tokens]** — Liquid Glass 設計 token 集（glass-bg / sheen / ambient / chip-bg）

### 關係
- CompactBar → composed_in → AppWindow
- QuickActions → composed_in → AppWindow (條件：expanded)
- ErrorLogOverlay → triggered_by → error-log.length > 0 + show-error-log
- AppWindow → exposes_callback → toggle-expanded / request-close / start-drag / clear-error-log
- CompactBar → uses → Tokens.glass-* / GlassDark.glass-*
- QuickActions → uses → Tokens.chip-bg / Tokens.chip-bg-active
- pin-on-top → controls → Window.always-on-top
- expanded → controls → preferred-height (52 ↔ 540)
- start-drag → delegates_to → Rust bridge（待實作，Win32/X11 視窗拖曳）
- error-log → push_from → src/whisper/engine.rs Result::Err、models/downloader.rs

### 鍵盤映射（新增）
- `Esc` → close overlay / collapse
- `Space` → toggle-recording
- `E` → toggle-expanded

---

## 變更歷史
| 版本 | 日期 | 內容 |
|------|------|------|
| v2.0 | 2026-05-16 | UI 重設計：新增 CompactBar/QuickActions/ErrorLogOverlay 節點與 Liquid Glass token 群 |
