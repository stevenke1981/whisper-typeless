# whisper-typeless — 技術規格書

## 1. 系統概述

whisper-typeless 是一個即時語音轉文字應用，
使用 whisper.cpp 做推論引擎，Rust 負責核心邏輯，
Slint 呈現 macOS 風格 GUI。
主要特色：轉錄結果自動注入作業系統焦點視窗。

---

## 2. Whisper 參數完整規格

### 2.1 模型參數

| 參數 | Rust 欄位 | 型別 | 預設值 | 說明 |
|------|-----------|------|--------|------|
| 模型路徑 | `model_path` | `PathBuf` | tiny.bin | GGUF 模型位置 |
| 使用 GPU | `use_gpu` | `bool` | true | CUDA/Metal 加速 |
| GPU 裝置 | `gpu_device` | `i32` | 0 | GPU 編號 |
| 執行緒數 | `n_threads` | `i32` | 4 | CPU 執行緒 |

### 2.2 轉錄參數

| 參數 | Rust 欄位 | 型別 | 預設值 | 說明 |
|------|-----------|------|--------|------|
| 語言 | `language` | `Option<String>` | "zh" | 語言代碼，None=自動偵測 |
| 翻譯模式 | `translate` | `bool` | false | 翻譯成英文 |
| 初始提示 | `initial_prompt` | `Option<String>` | None | 情境提示詞 |
| 最大 token | `max_tokens` | `i32` | 0 | 0=不限 |
| 音訊上下文 | `audio_ctx` | `i32` | 0 | 0=使用全部 |
| 靜音閾值 | `vad_thold` | `f32` | 0.6 | 0.0-1.0 |
| 頻率閾值 | `freq_thold` | `f32` | 100.0 | Hz |

### 2.3 解碼策略

| 參數 | Rust 欄位 | 型別 | 預設值 | 說明 |
|------|-----------|------|--------|------|
| 解碼策略 | `strategy` | `DecodingStrategy` | Greedy | Greedy / BeamSearch |
| Beam 數量 | `beam_size` | `i32` | 5 | BeamSearch beam 數 |
| Best of | `best_of` | `i32` | 5 | Greedy 候選數 |
| 溫度 | `temperature` | `f32` | 0.0 | 0.0=確定性 |
| 溫度遞增 | `temperature_inc` | `f32` | 0.2 | fallback 遞增量 |
| 耐心 | `patience` | `f32` | -1.0 | BeamSearch 耐心 |

### 2.4 時間戳記

| 參數 | Rust 欄位 | 型別 | 預設值 | 說明 |
|------|-----------|------|--------|------|
| 時間戳記 | `timestamps` | `bool` | false | 輸出時間戳 |
| Token 時間戳 | `token_timestamps` | `bool` | false | Token 級時間戳 |
| 時間戳 閾值 | `thold_pt` | `f32` | 0.01 | Token 時間戳閾值 |
| 最大段落長度 | `max_len` | `i32` | 0 | 字元數，0=不限 |
| 分割句子 | `split_on_word` | `bool` | false | 依詞分割 |

### 2.5 過濾與品質

| 參數 | Rust 欄位 | 型別 | 預設值 | 說明 |
|------|-----------|------|--------|------|
| 幻覺抑制 | `suppress_blank` | `bool` | true | 抑制空白輸出 |
| 抑制 NES | `suppress_non_speech` | `bool` | false | 抑制非語音 token |
| 熵閾值 | `entropy_thold` | `f32` | 2.4 | 高熵=不可信 |
| 對數機率閾值 | `logprob_thold` | `f32` | -1.0 | 最低 logprob |
| 無語音閾值 | `no_speech_thold` | `f32` | 0.6 | 靜音判定閾值 |
| 速度提升 | `speed_up` | `bool` | false | 2x 速度（降品質）|

---

## 3. 音訊規格

```
取樣率:   16,000 Hz（whisper 標準）
聲道:     單聲道
位元深度: f32 normalized（-1.0 ~ 1.0）
chunk 大小: 1600 samples（100ms）
VAD 視窗: 480 samples（30ms）
靜音超時: 可設定（預設 1500ms）
最大段落: 30 秒（whisper 上限）
```

### 3.1 VAD 流程

```
麥克風串流
    │
    ▼
[重採樣 → 16kHz]
    │
    ▼
[RMS 能量計算]
    │
    ├─ 能量 > vad_thold ──→ [累積音訊緩衝區]
    │                              │
    └─ 靜音 > timeout ─────→ [送入 Whisper 推論]
                                   │
                                   ▼
                             [後處理 + 輸出]
```

---

## 4. 情境模板系統

### 4.1 內建模板

```toml
# templates/meeting.toml
[template]
id = "meeting"
name_zh = "會議記錄"
name_en = "Meeting"
icon = "🏢"

[whisper]
initial_prompt = "這是一段商業會議的記錄，包含專業術語、人名和公司名稱。"
language = "zh"
temperature = 0.0
suppress_non_speech = true

[postprocess]
add_punctuation = true
formal_style = true
remove_fillers = ["嗯", "啊", "那個", "就是說"]
```

```toml
# templates/casual.toml
[template]
id = "casual"
name_zh = "口語對話"
name_en = "Casual"
icon = "💬"

[whisper]
initial_prompt = "這是一段日常對話。"
language = "zh"
temperature = 0.2

[postprocess]
add_punctuation = true
formal_style = false
```

```toml
# templates/technical.toml
[template]
id = "technical"
name_zh = "技術討論"
name_en = "Technical"
icon = "💻"

[whisper]
initial_prompt = "This is a technical discussion about software engineering, including programming terms, API names, and technical concepts."
language = "zh"
temperature = 0.0

[postprocess]
preserve_english_terms = true
code_term_protection = true
```

```toml
# templates/medical.toml
[template]
id = "medical"
name_zh = "醫療紀錄"
name_en = "Medical"
icon = "🏥"

[whisper]
initial_prompt = "這是醫療診療的對話，包含醫學術語、藥品名稱和診斷名稱。"
language = "zh"
temperature = 0.0

[postprocess]
formal_style = true
```

```toml
# templates/legal.toml
[template]
id = "legal"
name_zh = "法律文書"
name_en = "Legal"
icon = "⚖️"

[whisper]
initial_prompt = "這是法律相關的對話，包含法律術語和條文引用。"
language = "zh"
temperature = 0.0

[postprocess]
formal_style = true
traditional_chinese_only = true
```

### 4.2 自動情境偵測演算法

```
啟動時分析前 3 秒音訊（靜音不計）
→ 轉錄前 10 個詞彙
→ 對照關鍵詞字典評分
→ 選擇最高分模板
→ 可被使用者手動覆蓋
```

---

## 5. 中文處理規格

### 5.1 OpenCC 轉換模式

| 模式 ID | 說明 |
|---------|------|
| `zh-TW` | 簡體→繁體（台灣標準）**預設** |
| `zh-HK` | 簡體→繁體（香港標準） |
| `zh-CN` | 繁體→簡體 |
| `raw` | 不轉換，保持 whisper 原始輸出 |

### 5.2 後處理管線

```
Whisper 原始輸出
    │
    ▼
[OpenCC 轉換]
    │
    ▼
[標點符號正規化]  ← 半形→全形（中文語境）
    │
    ▼
[填詞移除]        ← 依模板設定
    │
    ▼
[重複段落偵測]    ← 移除 whisper 重複幻覺
    │
    ▼
[英文術語保護]    ← 技術模板：避免翻譯程式術語
    │
    ▼
最終輸出文字
```

---

## 6. 輸出系統規格

### 6.1 輸出模式

```rust
pub enum OutputMode {
    ClipboardOnly,           // 只寫剪貼簿，不自動貼上
    InjectToFocused,         // 直接注入焦點視窗
    ClipboardAndInject,      // 兩者皆做（預設）
    FileAppend(PathBuf),     // 追加到檔案
    None,                    // 只顯示，不輸出
}
```

### 6.2 注入焦點視窗流程

```
轉錄完成
    │
    ▼
[暫存目前焦點視窗 handle]
    │
    ▼
[寫入剪貼簿]
    │
    ▼
[等待 50ms]
    │
    ▼
[模擬 Ctrl+V 至焦點視窗]  (Windows)
[模擬 Cmd+V 至焦點視窗]   (macOS)
    │
    ▼
[等待 100ms 確認]
    │
    ▼
[清除剪貼簿] (可選，隱私模式)
```

### 6.3 輸出格式選項

| 選項 | 說明 |
|------|------|
| 附加換行 | 每段後加 \n |
| 附加空格 | 每段後加空格（英文模式）|
| 加時間戳 | [HH:MM:SS] 前綴 |
| 說話者標記 | 多人模式顯示 Speaker 1/2 |

---

## 7. 模型管理規格

### 7.1 可用模型清單

| 模型 | 大小 | VRAM | 速度 | 品質 |
|------|------|------|------|------|
| tiny | 75MB | 125MB | 10x | ★★☆☆☆ |
| tiny.en | 75MB | 125MB | 10x | ★★★☆☆ |
| base | 142MB | 210MB | 7x | ★★★☆☆ |
| base.en | 142MB | 210MB | 7x | ★★★★☆ |
| small | 466MB | 600MB | 4x | ★★★★☆ |
| small.en | 466MB | 600MB | 4x | ★★★★☆ |
| medium | 1.5GB | 1.7GB | 2x | ★★★★★ |
| medium.en | 1.5GB | 1.7GB | 2x | ★★★★★ |
| large-v2 | 2.9GB | 3.1GB | 1x | ★★★★★ |
| large-v3 | 2.9GB | 3.1GB | 1x | ★★★★★ |
| large-v3-turbo | 1.6GB | 1.8GB | 2x | ★★★★★ |

### 7.2 下載來源

```
主要：https://huggingface.co/ggerganov/whisper.cpp
備用：https://modelscope.cn/models/ggerganov/whisper.cpp
本地：models/ 目錄中的 .bin 檔案
```

### 7.3 下載功能

- 顯示即時進度（速度、剩餘時間）
- SHA256 完整性校驗
- 斷點續傳支援
- 多模型並行管理（但同時只用一個推論）

---

## 8. 設定結構規格

```toml
# ~/.config/whisper-typeless/config.toml

[general]
theme = "auto"                  # auto / light / dark
language_ui = "zh-TW"          # 介面語言
start_minimized = false
global_hotkey = "Ctrl+Shift+W"  # 全域快捷鍵
auto_start = false

[modules]
vad_enabled = true              # 靜音偵測模組
context_templates_enabled = true
opencc_enabled = true
auto_inject_enabled = true
waveform_display = true
history_enabled = true

[audio]
device_name = "default"
sample_rate = 16000
vad_silence_timeout_ms = 1500
vad_threshold = 0.6
max_segment_seconds = 30

[whisper]
model = "small"
language = "zh"
threads = 4
use_gpu = true
strategy = "greedy"
temperature = 0.0
beam_size = 5
suppress_blank = true
timestamps = false

[chinese]
conversion = "zh-TW"           # zh-TW / zh-HK / zh-CN / raw
add_punctuation = true
remove_fillers = true

[output]
mode = "clipboard_and_inject"
append_newline = true
append_space = false
clear_clipboard_after = false
inject_delay_ms = 50
privacy_mode = false

[context]
auto_detect = true
default_template = "casual"

[history]
max_entries = 1000
save_audio = false
export_format = "txt"           # txt / srt / json
```

---

## 9. GUI 規格

### 9.1 主視窗佈局

```
┌─────────────────────────────────────────────────┐
│  🎙 whisper-typeless          ─  □  ✕  (titlebar)│
├─────────────────────────────────────────────────┤
│ [模型: small ▼] [語言: 中文 ▼] [主題: ●○]        │
├─────────────────────────────────────────────────┤
│  情境: [會議 🏢][口語 💬][技術 💻][醫療 🏥][自動]  │
├─────────────────────────────────────────────────┤
│                                                 │
│  ▓▓▓▒▒░░▒▒▒▓▓▓▒▒░   ← 音訊波形                  │
│                                                 │
├─────────────────────────────────────────────────┤
│ ┌─────────────────────────────────────────────┐ │
│ │ 今天的會議主要討論了三個議題，第一個是關於  │ │
│ │ 產品路線圖的規劃，第二個是技術債的處理方式  │ │
│ │ ，第三個是新功能的優先級排序。             │ │
│ └─────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────┤
│ [● 錄音] [⏸ 暫停] [🗑 清除]    輸出: [📋✓][⌨️✓] │
├─────────────────────────────────────────────────┤
│ 狀態: 已辨識 127 字 | 延遲: 1.2s | 模型: small   │
└─────────────────────────────────────────────────┘
```

### 9.2 設定面板（側邊抽屜式）

```
┌─────────────────────────┐
│ ⚙ 設定                ✕ │
├─────────────────────────┤
│ ▸ 音訊                  │
│   裝置: [系統預設    ▼] │
│   靜音超時: [1500ms  ─] │
│   VAD 閾值: [0.6    ──] │
├─────────────────────────┤
│ ▸ Whisper 引擎          │
│   執行緒: [4         ─] │
│   GPU: [●  開]          │
│   溫度: [0.0        ─]  │
│   解碼: [Greedy     ▼]  │
├─────────────────────────┤
│ ▸ 中文處理              │
│   轉換: [繁體台灣   ▼]  │
│   標點: [●  開]         │
│   去填詞: [●  開]       │
├─────────────────────────┤
│ ▸ 輸出                  │
│   模式: [剪貼簿+注入▼]  │
│   換行: [●  開]         │
│   隱私清除: [○  關]     │
├─────────────────────────┤
│ ▸ 模組開關              │
│   VAD: [●  開]          │
│   情境模板: [●  開]     │
│   波形顯示: [●  開]     │
│   歷史記錄: [●  開]     │
└─────────────────────────┘
```

### 9.3 模型管理面板

```
┌─────────────────────────────────────────┐
│ 📦 模型管理                          ✕ │
├─────────────────────────────────────────┤
│ ✓ tiny          75MB   [使用中]         │
│ ✓ small        466MB   [選擇] [刪除]    │
│   medium       1.5GB   [下載]           │
│   large-v3     2.9GB   [下載]           │
│   large-v3-turbo 1.6GB [下載]           │
├─────────────────────────────────────────┤
│ 儲存位置: C:\Users\...\models  [變更]   │
│ 使用空間: 541MB / 可用: 120GB           │
└─────────────────────────────────────────┘
```

---

## 10. 模組開關介面規格

每個功能模組實作 `Module` trait：

```rust
pub trait Module: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn is_enabled(&self) -> bool;
    fn enable(&mut self);
    fn disable(&mut self);
    fn settings_schema(&self) -> ModuleSettings;
}
```

可開關的模組清單：

| 模組 ID | 名稱 | 說明 |
|---------|------|------|
| `vad` | 靜音偵測 | 自動偵測說話開始/結束 |
| `context_templates` | 情境模板 | 智慧選擇轉錄提示詞 |
| `opencc` | 繁簡轉換 | OpenCC 中文字體轉換 |
| `auto_inject` | 自動注入 | 結果自動貼入焦點視窗 |
| `waveform` | 波形顯示 | 即時音訊波形視覺化 |
| `history` | 歷史記錄 | 儲存所有轉錄結果 |
| `noise_suppress` | 降噪 | 音訊前處理降噪 |
| `auto_punctuation` | 自動標點 | 中文標點符號自動添加 |
| `speaker_detect` | 說話者偵測 | 多人場景說話者分離 |

---

## 11. 錯誤處理規格

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("音訊裝置錯誤: {0}")]
    AudioDevice(#[from] cpal::BuildStreamError),

    #[error("模型載入失敗: {path}")]
    ModelLoad { path: PathBuf },

    #[error("模型下載失敗: {url}, 狀態: {status}")]
    ModelDownload { url: String, status: u16 },

    #[error("轉錄引擎錯誤: {0}")]
    TranscriptionEngine(String),

    #[error("剪貼簿存取失敗: {0}")]
    Clipboard(String),

    #[error("設定讀取失敗: {0}")]
    Config(#[from] toml::de::Error),

    #[error("OpenCC 初始化失敗: {0}")]
    OpenCC(String),
}
```

---

## 12. 相依套件清單

```toml
[dependencies]
# GUI
slint = "1.7"

# Whisper
whisper-rs = "0.12"

# 音訊
cpal = "0.15"
rubato = "0.15"           # 重採樣

# 剪貼簿 + 鍵盤注入
arboard = "3.4"
enigo = "0.2"

# 中文轉換
opencc-rs = "0.1"         # 或使用 opencc-sys

# HTTP 下載
reqwest = { version = "0.12", features = ["stream", "json"] }
tokio = { version = "1", features = ["full"] }

# 設定
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# 工具
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
sha2 = "0.10"
indicatif = "0.17"        # CLI 進度條（下載用）

[build-dependencies]
cmake = "0.1"
```
