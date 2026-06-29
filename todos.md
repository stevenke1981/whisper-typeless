# whisper-typeless — 優化改善待辦清單

> 產生日期：2026-06-29
> 分析方法：CBM 圖形索引 + 原始碼逐檔審查 + 規格比對
> 分類：C=重大(critical), H=高(high), M=中(medium), L=低(low), E=強化(enhancement)

---

## P0 — 重大架構問題（Critical）

### [C-01] 雙設定系統衝突
**問題**：同時存在 `AppConfig`（TOML，`~/.config/whisper-typeless/config.toml`）與 `UiSettings`（JSON，`settings.json` 於 CWD），欄位重疊但各自獨立，會導致儲存一方的設定覆蓋另一方的預設值。

**驗證**：設定理論上分屬兩個檔案，但 `vad_threshold` 預設值在 `AudioConfig` 為 `0.02`，在 `UiSettings` 為 `0.6`，不一致。

**建議方案**：
1. 統一為單一設定來源，廢棄 `UiSettings`，所有 UI 設定讀寫統一走 `AppConfig`
2. 或明確分離：`AppConfig` 作為底層引擎設定（唯讀啟動時載入），`UiSettings` 作為 UI 偏好（經常儲存）
3. 至少將 `vad_threshold` 預設值統一

**影響範圍**：`src/settings/config.rs`、`src/settings/ui_settings.rs`、`src/ui/bridge.rs`

---

### [C-02] 錄音管線繞過抽象層
**問題**：`src/ui/bridge/recording.rs` 的 `transcribe_and_update()` 直接：
- 建立 `arboard::Clipboard` 寫入剪貼簿（跳過 `ClipboardOutput`）
- 呼叫獨立的 `inject_text_to_focused_window()`（跳過 `FocusedWindowInjector`）
- 直接呼叫 `convert_chinese_text()`（跳過 `TranscriptionPipeline`）

這導致 `output/` 與 `transcription/` 模組的設計形同虛設，修改 `ClipboardOutput` 或 `OutputRouter` 的行為不會影響實際輸出。

**建議方案**：
1. 在 `recording.rs` 中改使用 `OutputRouter` 與 `TranscriptionPipeline`
2. 或移除此處重複實作，統一委派給 output 模組

**影響範圍**：`src/ui/bridge/recording.rs`（主要）、`src/output/mod.rs`、`src/transcription/pipeline.rs`

---

### [C-03] 測試覆蓋率嚴重不足
**問題**：專案僅 9 個整合測試，全集中在 audio VAD（3）、transcription opencc/postprocess（4）、output formatter（3）。**完全無測試的模組**：
- `audio::capture`（麥克風擷取）
- `audio::resampler`（重採樣）
- `whisper::engine`（推論引擎）
- `whisper::params`（參數序列化）
- `models::registry`、`manager`、`downloader`
- `output::clipboard`、`injector`、`router`
- `settings::config`（設定序列化/反序列化）
- `context_templates::custom`、`templates`（模板持久化）
- `file_transcription`（檔案解碼/匯出）
- `ui::bridge` （全部 callback 邏輯）

**驗證**：`cargo test` 僅執行 9 個測試，且無需實際模型即可全部通過。

**建議方案**：參見 `test.md`

---

### [C-04] 缺少 CI/CD 管線
**問題**：無 `.github/workflows/`、無 CI 設定檔。無法自動化建置驗證、lint 檢查、測試執行。

**建議方案**：新增 GitHub Actions workflow，包含：
- `cargo check` / `cargo test`（Windows + Linux）
- `cargo clippy`
- `cargo fmt --check`
- 建置 artifact 上傳

---

## P1 — 高優先程式碼品質

### [H-01] VAD 預設閾值不一致
**問題**：
- `AudioConfig::default()` → `vad_threshold: 0.02`（幾乎所有音訊都會觸發）
- `UiSettings::default()` → `vad_threshold: 0.6`（較合理但 UI 不讀 config）
- `WhisperParams` 的 `no_speech_thold` 也為 `0.3`

**影響**：使用者啟動 app 時若未載入 `settings.json`，VAD 會用 0.02 的極低閾值，導致持續錄音。

**影響範圍**：`src/settings/config.rs:41`、`src/settings/ui_settings.rs:47`

---

### [H-02] 重複的 UI callback 樣板
**問題**：`slint::invoke_from_event_loop()` + `ui_weak.upgrade()` 模式在 `bridge.rs`、`recording.rs`、`file_mode.rs`、`templates.rs` 等檔案中重複約 **40 次以上**。

**建議方案**：新增輔助 macro 或函式：
```rust
macro_rules! ui_update {
    ($ui_weak:expr, $block:expr) => {
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = $ui_weak.upgrade() {
                $block(ui);
            }
        });
    };
}
```

**影響範圍**：`src/ui/bridge/*.rs`

---

### [H-03] 重採樣器使用硬編碼常數
**問題**：`resampler.rs` 中 `TARGET_SAMPLE_RATE = 16000`、`CHUNK_SIZE = 1024` 為私有常數，但 `vad.rs`、`recording.rs`、`file_transcription.rs` 中也直接使用 `16000` 與 `16_000` 等 magic number。

**建議方案**：將取樣率常數提升至 `audio::mod.rs` 或共用常數模組。

**影響範圍**：`src/audio/resampler.rs`、`src/audio/vad.rs`、`src/file_transcription/mod.rs`、`src/ui/bridge/recording.rs`

---

### [H-04] 模型檔名使用舊版 ggml 前綴
**問題**：程式碼寫死 `ggml-{name}.bin` 格式，但 whisper.cpp 新模型（如 large-v3-turbo）使用 `ggml-` 前綴仍可運作。然而 HF 上已逐漸改用非 `ggml-` 命名。

**建議方案**：將檔名格式抽為 `ModelId::filename()` 的可設定邏輯，並支援 `{name}.gguf` 副檔名。

**影響範圍**：`src/models/mod.rs:41`、`src/models/manager.rs:27,49`

---

### [H-05] SHA-1 用於模型校驗
**問題**：`downloader.rs` 使用 SHA-1 進行檔案完整性檢查，但 SHA-1 已被視為密碼學弱式。專案已依賴 `sha2` crate。

**建議方案**：改用 SHA-256（`sha2::Sha256`），同時更新 `model-catalog.json` 中的 checksum 欄位。

**影響範圍**：`src/models/downloader.rs`、`model-catalog.json`

---

## P2 — 功能缺失

### [M-01] 降噪模組未實作
**問題**：`ModuleConfig` 包含 `noise_suppress` 欄位、`ModuleRegistry` 包含對應條目，但無任何降噪實作程式碼。

**建議方案**：可整合 `noise-suppression` crate 或基於 WebRTC 的簡易降噪演算法。

**影響範圍**：`src/audio/`（需新增 `denoiser.rs` 或擴充 `vad.rs`）

---

### [M-02] 說話者偵測未實作
**問題**：`speaker_detect` 在 module 清單中但無實作。

**建議方案**：基於音訊能量頻譜的簡易說話者變換偵測，或整合純 Rust 的說話者日誌。

**影響範圍**：需新增 `src/audio/speaker_detect.rs`。

---

### [M-03] 缺少 `transcription::context` 模組
**問題**：`plan.md` 列出 `transcription::context.rs` 作為情境提示詞注入模組，但實際 `src/transcription/` 僅有 `mod.rs`、`opencc.rs`、`pipeline.rs`、`postprocess.rs`。

**建議方案**：新增 `context.rs` 處理 `initial_prompt` 組裝邏輯（整合模板系統的提示詞）。

**影響範圍**：`src/transcription/`（新增檔案）

---

### [M-04] 模板 TOML 檔案目錄為空
**問題**：`templates/` 目錄存在但無任何 `.toml` 檔案，模板完全硬編碼於 Rust。外部使用者無法自訂。

**建議方案**：建立 `templates/meeting.toml` 等檔案並在啟動時載入，與硬編碼互補。

**影響範圍**：`templates/`（新增檔案）、`src/context_templates/templates.rs`

---

### [M-05] 隱私模式未實作
**問題**：`OutputConfig.privacy_mode` 存在且可設定，但：
- `recording.rs` 在 `save_recording_wav()` 中不檢查此旗標
- 輸出管線不檢查是否應清除剪貼簿

**建議方案**：在錄音停止時檢查 `privacy_mode`，若啟用則不保留本機 WAV 檔，並在輸出後清除剪貼簿。

**影響範圍**：`src/ui/bridge/recording.rs`、`src/output/mod.rs`

---

### [M-06] 集中錯誤收集未實作
**問題**：UI 有 `error-log` 屬性與 `show-error-log` 旗標，但 Rust 端沒有任何推送錯誤至 UI 的機制。所有錯誤僅透過 `tracing::error!()` 輸出至 stderr。

**建議方案**：建立全域錯誤通道，讓各模組可 push error 字串至 UI 顯示。

**影響範圍**：`src/app.rs`（新增 error log 收集欄位）、`src/ui/bridge.rs`（wire 至 UI 屬性）

---

## P3 — 低優先 / 強化

### [L-01] enigo 0.2 版本老舊
**問題**：`Cargo.toml` 使用 `enigo = "0.2"`，最新版為 0.3。API 差異較大，升級需調整 injector 用法。

**建議方案**：升級至 0.3（或評估改用 `minput` 或 `autoimp`），需注意 Windows/macOS/Linux 相容性。

---

### [L-02] PostProcessor 不必要的重複配置
**問題**：`remove_fillers()` 使用 `fold(text, |acc, filler| acc.replace(...))`，每次 replace 都產生新 String。`remove_repeated_phrases()` 演算法為 O(n²)。

**建議方案**：
- 使用單次掃描 + `String::with_capacity` 預分配
- 或使用 `regex::Regex::replace_all` 一次性移除所有 filler

---

### [L-03] 缺少 Linux/macOS CI 與 plaftform 注意事項
**問題**：大量 Win32 API 呼叫（hotkey、PTT、stderr redirect、window icon、topmost），但無對應的 Linux/macOS 實作，部分函式為空殼。

**建議方案**：
- 記錄明確的 platform gate 文件
- 為 Linux (X11/Wayland) 與 macOS 實作 `start-drag` 與 `pin-on-top`

---

### [L-04] 模板系統使用 display_name 作為 HashMap key
**問題**：`TemplateRegistry` 使用 `id.display_name()`（如 "會議記錄"）作為 `HashMap<String, ContextTemplate>` 的 key。這使得 key 依賴於語言。

**建議方案**：改用 `TemplateId` 作為 key（實作 Hash trait），或使用 `id` 欄位的英文名稱。

---

### [L-05] ContextDetector 僅用關鍵詞比對
**問題**：偵測演算法只做簡單的 `contains()` 字串比對，無 TF-IDF 或語意相似度。對於同義詞或上下文無法正確分類。

**建議方案**：擴充關鍵詞字典或加入輕量統計分數加權。

---

### [L-06] `build-release.ps1` 無對應的 CI 腳本檢查
**問題**：有 `build-release.ps1`、`build.bat`、`test.bat`，但 Rust 社群慣用的 `cargo` 指令可達成同樣效果，維護 batch 腳本增加負擔。

**建議方案**：評估是否保留外部腳本，或統一為 Makefile / Justfile / cargo-make。

---

### [L-07] `model-catalog.json` 中 SHA-1 續用
**問題**：若將 downloader 改為 SHA-256，需同步更新 catalog 中的 checksum 與對應欄位名。

---

## 改善優先級矩陣

```
            高效益       中效益       低效益
高風險    C-01, C-02   H-01         L-03
中風險    C-03, C-04   M-01, M-02   L-05
低風險    H-02, H-03   M-03, M-04   L-01, L-02, L-06
```

---

## 快速上手（建議執行順序）

1. **C-01**：統一 VAD threshold 預設值（1 處修改）
2. **H-02**：新增 `ui_update!` macro（減量 40+ 重複樣板）
3. **H-03**：抽取共用常數（消除 magic numbers）
4. **C-03**：優先增加無需外部依賴的單元測試（依 `test.md`）
5. **C-04**：建立 CI workflow（`cargo check + test + clippy`）
6. **C-02**：錄音管線改由 `OutputRouter` + `TranscriptionPipeline` 處理
7. **M-05**：實作隱私模式檢查
8. **M-06**：集中錯誤收集通道
9. **H-04**、**H-05**：模型檔案管理改善
10. **M-01**、**M-02**、**M-03**：補實缺失模組
