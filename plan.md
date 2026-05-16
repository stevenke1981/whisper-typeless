# whisper-typeless — 開發計畫

## 專案概述

基於 whisper.cpp 的 Rust + Slint GUI 語音輸入應用，
macOS 風格介面，支援自動輸出至焦點視窗，
繁簡中文選擇（預設 zh-TW），情境模板智慧辨識。

---

## 技術棧

| 層次 | 技術 | 說明 |
|------|------|------|
| GUI | Slint 1.x | macOS 風格，宣告式 UI |
| 核心 | Rust 1.78+ | 安全、高效能 |
| Whisper | whisper-rs | whisper.cpp 官方 Rust 綁定 |
| 中文轉換 | opencc-rs | OpenCC 繁簡轉換 |
| 音訊 | cpal | 跨平台音訊擷取 |
| 剪貼簿 | arboard | 跨平台剪貼簿 |
| 鍵盤模擬 | enigo | 焦點視窗自動貼上 |
| 設定 | serde + toml | 設定持久化 |
| HTTP | reqwest | 模型自動下載 |

---

## 開發階段

### Phase 0 — 基礎建設（Week 1）
- [ ] 初始化 Cargo workspace
- [ ] 設定 whisper-rs 相依（含 GGML 後端）
- [ ] build.rs 自動編譯 whisper.cpp
- [ ] 基本 Slint 視窗骨架
- [ ] 模組目錄結構建立
- [ ] CI 設定（Windows / macOS / Linux）

### Phase 1 — 核心引擎（Week 2）
- [ ] `whisper::engine` — 載入模型、執行推論
- [ ] `whisper::params` — 全參數映射結構
- [ ] `audio::capture` — cpal 麥克風串流
- [ ] `audio::vad` — Voice Activity Detection 靜音偵測
- [ ] 基本轉錄流程端對端驗證

### Phase 2 — 模型管理（Week 2-3）
- [ ] `models::registry` — 模型清單（Hugging Face GGUF）
- [ ] `models::downloader` — 進度條下載、SHA256 校驗
- [ ] `models::manager` — 本地模型掃描、載入、切換
- [ ] UI：模型選擇下拉、下載進度顯示

### Phase 3 — 輸出模組（Week 3）
- [ ] `output::clipboard` — 寫入剪貼簿
- [ ] `output::injector` — enigo 模擬 Ctrl+V 至焦點視窗
- [ ] `output::formatter` — 去除重複、標點修正
- [ ] 輸出模式切換（剪貼簿 / 直接輸入 / 兩者）

### Phase 4 — 中文處理（Week 3-4）
- [ ] `transcription::opencc` — OpenCC 整合（zh-TW / zh-CN / 原文）
- [ ] `transcription::postprocess` — 全形標點、空白處理
- [ ] `transcription::context` — 情境詞彙表注入 prompt

### Phase 5 — 情境模板（Week 4）
- [ ] `context::templates` — 內建模板（會議、口語、技術、醫療、法律）
- [ ] `context::detector` — 自動情境偵測（關鍵詞統計）
- [ ] `context::custom` — 使用者自訂模板 TOML 格式
- [ ] UI：模板選擇器、自訂編輯器

### Phase 6 — 完整 GUI（Week 4-5）
- [ ] 主視窗佈局（macOS 風格，毛玻璃感）
- [ ] 即時波形顯示
- [ ] 轉錄文字捲動顯示
- [ ] 設定面板（全部 whisper 參數）
- [ ] 系統匣圖示 + 全域快捷鍵
- [ ] 深色 / 淺色主題

### Phase 7 — 設定與模組化（Week 5）
- [ ] `settings::config` — TOML 設定讀寫
- [ ] 每個功能模組可在設定中啟用 / 停用
- [ ] 設定 UI 完整實作
- [ ] 設定匯出 / 匯入

### Phase 9 — UI v2.0 重設計（2026-05-16，迭代新增）
- [x] Liquid Glass 視覺語言（半透明、sheen、ambient gradient）
- [x] Compact Bar 模式（摺疊 380x52 / 展開 380x540）
- [x] `expanded` / `pin-on-top` / `show-error-log` 新屬性
- [x] Inline QuickActions 列（模板 / 模型 / 輸出 / 清除 / 錯誤）
- [x] Keyboard-first：Esc 收合 / Space 錄音 / E 展開
- [x] 錯誤紀錄浮層 + 計數徽章
- [x] 24-bar mini waveform 嵌入頂部條
- [ ] Rust 端：實作 `start-drag` (Win32 SetWindowPos / X11 _NET_WM_MOVERESIZE)
- [ ] Rust 端：實作 `error-log` 推送（從 Result::Err 收集）
- [ ] Rust 端：`pin-on-top` 同步至 OS 視窗 flag

### Phase 8 — 測試與最佳化（Week 6）
- [ ] 單元測試 80%+ 覆蓋率
- [ ] 整合測試（音訊→轉錄→輸出 全流程）
- [ ] 效能調校（VAD 延遲、記憶體）
- [ ] 打包（Windows NSIS、macOS DMG）

---

## 目錄結構

```
whisper-typeless/
├── Cargo.toml
├── build.rs                    # 自動編譯 whisper.cpp
├── plan.md
├── spec.md
├── knowledge_graph.md
├── src/
│   ├── main.rs
│   ├── app.rs                  # AppState 全局狀態
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── capture.rs          # 麥克風擷取
│   │   ├── vad.rs              # 靜音偵測
│   │   └── resampler.rs        # 重採樣 → 16kHz
│   ├── whisper/
│   │   ├── mod.rs
│   │   ├── engine.rs           # 推論引擎
│   │   ├── params.rs           # 參數結構 (全部)
│   │   └── session.rs          # 推論 Session 管理
│   ├── models/
│   │   ├── mod.rs
│   │   ├── registry.rs         # 可用模型清單
│   │   ├── downloader.rs       # 自動下載
│   │   └── manager.rs          # 本地管理
│   ├── transcription/
│   │   ├── mod.rs
│   │   ├── pipeline.rs         # 轉錄流水線
│   │   ├── opencc.rs           # 繁簡轉換
│   │   ├── postprocess.rs      # 後處理
│   │   └── context.rs          # 情境提示詞
│   ├── context_templates/
│   │   ├── mod.rs
│   │   ├── templates.rs        # 內建模板定義
│   │   ├── detector.rs         # 自動偵測
│   │   └── custom.rs           # 使用者自訂
│   ├── output/
│   │   ├── mod.rs
│   │   ├── clipboard.rs        # 剪貼簿
│   │   ├── injector.rs         # 焦點視窗注入
│   │   └── formatter.rs        # 文字格式化
│   ├── settings/
│   │   ├── mod.rs
│   │   ├── config.rs           # 設定結構
│   │   └── persistence.rs      # 讀寫 TOML
│   └── ui/
│       ├── mod.rs
│       └── bridge.rs           # Slint ↔ Rust 橋接
├── ui/
│   ├── appwindow.slint          # 主視窗
│   ├── components/
│   │   ├── waveform.slint       # 音訊波形
│   │   ├── transcript.slint     # 轉錄文字區
│   │   ├── model_picker.slint   # 模型選擇
│   │   ├── context_bar.slint    # 情境選擇列
│   │   ├── settings_panel.slint # 設定面板
│   │   └── toggle.slint         # 開關元件
│   └── styles/
│       ├── tokens.slint         # 設計 Token
│       └── macos_theme.slint    # macOS 主題
├── models/                      # 下載的 GGUF 模型
├── templates/                   # 內建情境模板 TOML
│   ├── meeting.toml
│   ├── casual.toml
│   ├── technical.toml
│   ├── medical.toml
│   └── legal.toml
└── tests/
    ├── audio_tests.rs
    ├── transcription_tests.rs
    └── output_tests.rs
```

---

## 風險與緩解

| 風險 | 機率 | 緩解策略 |
|------|------|---------|
| whisper-rs 編譯失敗 (Windows) | 中 | 預先安裝 CMake/clang，build.rs fallback |
| enigo 在 Wayland 不支援 | 低 | 偵測平台，Wayland 降級為剪貼簿模式 |
| 模型下載速度慢 | 中 | 多源 mirror (HF / ModelScope)，斷點續傳 |
| VAD 延遲過高 | 低 | 可調靜默閾值，streaming chunk 最佳化 |
| OpenCC 詞典打包 | 低 | 靜態連結或內嵌詞典 bytes |

---

## 變更歷史
| 版本 | 日期 | 內容 | 影響範圍 |
|------|------|------|---------|
| v2.1 | 2026-05-16 | 改為「完整視窗 ↔ 浮動視窗」雙模式切換；設定/錯誤紀錄兩模式共用；新增 floating-mode/floating-expanded 屬性 | ui/appwindow.slint |
| v2.0 | 2026-05-16 | UI 重設計：Liquid Glass + Compact Bar；新增 expanded/pin-on-top/error-log；新增 CompactBar、QuickActions 元件 | ui/appwindow.slint, ui/styles/tokens.slint, ui/components/compact_bar.slint, ui/components/quick_actions.slint |
| v1.0 | — | 初始建立 | — |
