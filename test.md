# whisper-typeless — 測試策略與案例

> 適用於：單元測試、整合測試、屬性測試、mock 策略
> 基底指令：`cargo test`（目前僅 9 個測試）
> 目標：達到 70%+ 行覆蓋率，關鍵模組 90%+

---

## 目錄

1. [測試架構](#1-測試架構)
2. [單元測試（Unit Tests）](#2-單元測試unit-tests)
3. [整合測試（Integration Tests）](#3-整合測試integration-tests)
4. [屬性測試（Property-Based Tests）](#4-屬性測試property-based-tests)
5. [Mock 策略](#5-mock-策略)
6. [測試優先級](#6-測試優先級)
7. [不適合自動化測試的項目](#7-不適合自動化測試的項目)

---

## 1. 測試架構

```
tests/                          ← 整合測試（Integration Tests）
├── audio_tests.rs              ← 現有：VAD 基本測試
├── output_tests.rs             ← 現有：Formatter 測試
├── transcription_tests.rs      ← 現有：OpenCC / PostProcess / Detector
├── models_tests.rs             ← 待新增：ModelRegistry / ModelManager / Downloader
├── file_transcription_tests.rs ← 待新增：檔案解碼 / 格式匯出
├── settings_tests.rs           ← 待新增：Config 序列化
└── audio_integration_tests.rs  ← 待新增：Resampler + VAD 全流程

src/                            ← 內聯單元測試（#[cfg(test)]）
├── audio/
│   ├── capture.rs              ← 新增：device listing 邏輯測試
│   ├── resampler.rs            ← 新增：同步/非同步重採樣測試
│   └── vad.rs                  ← 已有 2 個測試，擴充 edge cases
├── whisper/
│   ├── params.rs               ← 新增：序列化/反序列化、預設值測試
│   └── engine.rs               ← 新增：error handling 測試（mock 後）
├── models/
│   ├── registry.rs             ← 新增：catalog 解析測試
│   ├── manager.rs              ← 新增：cache/miss/switch 邏輯測試
│   └── downloader.rs           ← 新增：校驗邏輯測試（不實際下載）
├── transcription/
│   ├── opencc.rs               ← 已有 2 個測試，補全所有 mode
│   ├── postprocess.rs          ← 已有 2 個測試，擴充 edge cases
│   └── pipeline.rs             ← 新增：pipeline 組合邏輯測試
├── output/
│   ├── clipboard.rs            ← 新增：錯誤處理測試
│   ├── injector.rs             ← 新增：延遲計算、錯誤處理
│   ├── formatter.rs            ← 現有 3 個測試，補 timestamp
│   └── mod.rs                  ← 新增：OutputRouter 路由測試
├── settings/
│   ├── config.rs               ← 新增：序列化/反序列化、預設值
│   └── persistence.rs          ← 新增：讀/寫/匯出/匯入
├── context_templates/
│   ├── templates.rs            ← 新增：模板複製、apply_to_params
│   ├── detector.rs             ← 已有 2 個測試，擴充混和關鍵詞
│   └── custom.rs               ← 新增：儲存/讀取 roundtrip
└── file_transcription/
    └── mod.rs                  ← 新增：ExportFormat、format_transcript
```

---

## 2. 單元測試（Unit Tests）

### 2.1 `audio::vad` — 現有 2 個，應擴充

| 測試名稱 | 類別 | 條件 | 預期 |
|---------|------|------|------|
| `silence_produces_no_segment` | 現有 | 0.01 RMS, threshold 0.5 | ✓ |
| `loud_audio_starts_speech` | 現有 | 0.8 RMS, threshold 0.1 | ✓ |
| `vad_accumulates_audio_during_speech` | 現有 | 5 chunk, 長 timeout | ✓ |
| `vad_detects_silence_timeout` | 新增 | 3 chunks loud + 等待 > timeout | 產生 Segment → Silence |
| `vad_flush_returns_buffered` | 新增 | 累積 chunk 後 flush | 回傳 Some(AudioSegment) |
| `vad_flush_empty_returns_none` | 新增 | 無緩衝時呼叫 flush | 回傳 None |
| `vad_max_segment_triggers_flush` | 新增 | 累積超過 max_segment_samples | 自動 flush |
| `vad_rms_empty_returns_zero` | 新增 | 空切片輸入 | 0.0 不回 panic |
| `vad_threshold_update_takes_effect` | 新增 | 動態變更 threshold | 新值生效 |
| `vad_silence_timeout_update` | 新增 | 動態變更 timeout | 新超時生效 |

### 2.2 `audio::resampler` — 新增 4 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `resampler_passthrough_at_16k` | source_rate = 16000 | 輸入等於輸出 |
| `resampler_downsample_48k_to_16k` | 48000 Hz 正弦波輸入 | 輸出長度 = 輸入長度 / 3（±容忍） |
| `resampler_accumulates_partial_chunks` | 非整數倍 chunk 大小的輸入 | 能正確累積並輸出 |
| `resampler_handles_empty_input` | 空切片 | 回傳 Ok(空 Vec) |

### 2.3 `whisper::params` — 新增 3 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `params_default_values` | `WhisperParams::default()` | 驗證所有欄位正確 |
| `params_serde_roundtrip` | 序列化→反序列化 | 等於原始 |
| `params_strategy_default_is_greedy` | 不指定 | DecodingStrategy::Greedy |

### 2.4 `models::registry` — 新增 3 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `registry_parses_embedded_catalog` | 不指定外部檔案 | 回傳至少 5 個模型 |
| `registry_entries_have_required_fields` | 所有模型 | `id`、`download_url`、`size.disk_mb > 0` |
| `registry_fallback_on_broken_catalog` | 無效 JSON | 回傳空 Vec |
| `model_id_as_str_all_variants` | 每種 ModelId | 回傳非空字串 |

### 2.5 `models::manager` — 新增 4 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `manager_cache_hit_returns_same` | 相同 model_id 兩次 load | 同一 Arc 指標 |
| `manager_cache_miss_loads_new` | 不同 model_id | 載入不同引擎 |
| `manager_unload_clears_cache` | unload 後再 load | 新載入 |
| `manager_list_marks_downloaded` | 指定 models_dir | `is_downloaded` 正確 |

### 2.6 `models::downloader` — 新增 3 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `downloader_checksum_match` | SHA1 正確 | Ok(path) |
| `downloader_checksum_mismatch` | SHA1 錯誤 | Err(checksum mismatch) |
| `downloader_no_urls_returns_error` | 空 URL 清單 | Err |

### 2.7 `transcription::pipeline` — 新增 3 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `pipeline_applies_postprocess_and_opencc` | 簡體輸入 + zh-TW 模式 | 輸出繁體且去除填詞 |
| `pipeline_raw_mode_bypasses_opencc` | raw 模式 | 不轉換字體 |
| `pipeline_empty_result_handled` | 空白 TranscriptResult | 回傳 ProcessedText 但 text 為空 |

### 2.8 `transcription::postprocess` — 擴充 7 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `postprocess_removes_fillers` | 現有 | ✓ |
| `postprocess_normalizes_punctuation` | 現有 | ✓ |
| `postprocess_removes_repeated_phrases` | 現有 | ✓ |
| `postprocess_overlapping_repeats` | "ABCABCABCABC" | 保留一組「ABC」 |
| `postprocess_short_text_no_repeat_check` | 少於 6 字元 | 原樣回傳 |
| `postprocess_punctuation_normalize_all` | 所有半形標點 | 全部轉全形 |
| `postprocess_formal_style_noop` | formal_style=true | 不改變無標點字串 |
| `postprocess_repeated_edge_bigrams` | "ABABAB" | 至少保留一組 |
| `postprocess_empty_input` | 空白字串 | 空白字串 |

### 2.9 `output::formatter` — 擴充 4 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `formatter_appends_newline` | 現有 | ✓ |
| `formatter_no_append` | 現有 | ✓ |
| `formatter_appends_space` | 現有 | ✓ |
| `formatter_adds_timestamp` | `add_timestamp=true` | 輸出字串以 `[` 開頭 |
| `formatter_all_flags_combined` | newline + timestamp | 兩者都生效 |

### 2.10 `output::mod`（OutputRouter）— 新增 6 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `router_clipboard_only` | mode=ClipboardOnly | 寫入剪貼簿，不 inject |
| `router_none_does_nothing` | mode=None | 無錯誤，無副作用 |
| `router_mode_switch` | 先設 ClipboardOnly 再切換 | 新 mode 生效 |
| `router_format_is_applied` | append_newline=true | 輸出包含換行 |
| `router_inject_only` | mode=InjectToFocused | 不寫剪貼簿 |

> 注意：Clipboard 與 Inject 測試需隔離，建議使用 mock（見第 5 節）

### 2.11 `settings::config` — 新增 5 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `config_default_values` | `AppConfig::default()` | 驗證所有子結構 |
| `config_toml_roundtrip` | 序列化 TOML → 反序列化 | 等於原始 |
| `config_modules_is_enabled` | 查詢已知/未知 module | true/false |
| `config_module_all_enabled` | `ModuleConfig::all_enabled()` | 主要 module 全 true |
| `config_load_or_default_no_file` | 不存在的路徑 | 回傳 default |

### 2.12 `settings::persistence` — 新增 3 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `persistence_load_nonexistent` | 檔案不存在 | Ok(default) |
| `persistence_save_and_load` | 寫入後讀取 | 一致 |
| `persistence_export_import` | 匯出後匯入 | 設定正確 |

### 2.13 `context_templates::templates` — 新增 4 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `templates_all_returns_ordered` | `TemplateRegistry::all()` | 順序固定 |
| `templates_apply_prompt_to_params` | 有 initial_prompt 的模板 | params.initial_prompt 被設定 |
| `templates_apply_language_override` | language_override 不為 None | params.language 被覆寫 |
| `templates_get_casual_exists` | `get_casual()` | 不回 panic |

### 2.14 `context_templates::detector` — 擴充 3 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `detects_meeting` | 現有 | ✓ |
| `defaults_to_casual` | 現有 | ✓ |
| `detector_technical_keywords` | 含程式/API/bug 字串 | Technical |
| `detector_multiple_matches_highest_wins` | 同時匹配 meeting + technical | 分數較高的 |
| `detector_empty_text` | 空白文字 | Casual |

### 2.15 `context_templates::custom` — 新增 2 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `custom_store_load_missing` | 檔案不存在 | 空 Vec |
| `custom_store_save_and_load_roundtrip` | 寫入 2 個模板後讀取 | 一致 |

### 2.16 `file_transcription` — 新增 5 個

| 測試名稱 | 條件 | 預期 |
|---------|------|------|
| `export_format_from_extension` | "srt" / "vtt" / "txt" / "unknown" | 對應 enum |
| `format_transcript_txt` | segments → TXT | 純文字串聯 |
| `format_transcript_srt` | segments → SRT | 符合 SRT 格式 |
| `format_transcript_vtt` | segments → VTT | 以 "WEBVTT" 開頭 |
| `ms_to_srt_vtt_matches` | 相同時間 ms | SRT 用逗號，VTT 用句點 |

---

## 3. 整合測試（Integration Tests）

### 3.1 `tests/audio_integration_tests.rs`

| 測試名稱 | 描述 | 備註 |
|---------|------|------|
| `resampler_vad_roundtrip` | 48kHz 音訊 → resampler → VAD 處理 | 無需麥克風 |
| `vad_large_audio_file` | 5 秒合成音訊 → VAD → segment | 程式產生 sine wave |
| `resampler_chaining` | 多個 chunk 依序餵入 resampler | 累積正確性 |

### 3.2 `tests/models_tests.rs`

| 測試名稱 | 描述 | 備註 |
|---------|------|------|
| `model_manager_list_available` | 指定 models_dir 掃描 | 需建立暫存 .bin 檔案 |
| `model_downloader_no_network` | 不實際發送 HTTP | mock HTTP 測試 |

### 3.3 `tests/file_transcription_tests.rs`

| 測試名稱 | 描述 | 備註 |
|---------|------|------|
| `export_txt_creates_valid_file` | 寫入暫存檔案 | 內容驗證 |
| `export_srt_creates_valid_file` | SRT 格式驗證 | 正規表達式驗證時間軸格式 |
| `export_vtt_creates_valid_file` | VTT 格式驗證 | 以 WEBVTT 開頭 |

---

## 4. 屬性測試（Property-Based Tests）

使用 `proptest` crate（需加入 dev-dependencies）：

### 4.1 VAD 屬性

```rust
proptest! {
    #[test]
    fn vad_never_panics_on_any_input(
        samples in proptest::collection::vec(proptest::num::f32::ANY, 0..3200),
        threshold in 0.0f32..1.0f32,
    ) {
        let mut vad = VoiceActivityDetector::new(threshold, 1000);
        let events = vad.process(&samples);
        // 不 panic 即為成功
    }

    #[test]
    fn vad_rms_is_bounded_0_to_1(
        samples in proptest::collection::vec(-10.0f32..10.0f32, 1..3200),
    ) {
        let rms = VoiceActivityDetector::rms(&samples);
        prop_assert!(rms.is_finite() && rms >= 0.0);
    }
}
```

### 4.2 PostProcessor 屬性

```rust
proptest! {
    #[test]
    fn postprocess_never_panics(
        text in ".*",
        punctuation in proptest::bool::ANY,
    ) {
        let settings = PostProcessSettings {
            add_punctuation: punctuation,
            ..Default::default()
        };
        let proc = PostProcessor::new(settings);
        let result = proc.process(&text);
        // 不 panic 即為成功
    }
}
```

### 4.3 ModelId 屬性

```rust
proptest! {
    #[test]
    fn model_id_as_str_is_non_empty(id in prop::sample::select(vec![
        ModelId::Tiny, ModelId::Small, ModelId::Medium, ModelId::LargeV3, ModelId::LargeV3Turbo,
    ])) {
        prop_assert!(!id.as_str().is_empty());
        prop_assert!(!id.filename().is_empty());
    }
}
```

---

## 5. Mock 策略

### 5.1 需要 mock 的外部資源

| 資源 | 測試模組 | Mock 方案 |
|------|---------|----------|
| Whisper 模型檔案（.bin） | `whisper::engine` | 新增 `WhisperEngine::load` 回傳 mock context；或使用 `#[cfg(test)]` 回傳 stub |
| 音訊裝置（cpal） | `audio::capture` | 新增 `AudioCapture::new` 的 test-only constructor，注入 mock Device |
| 剪貼簿（arboard） | `output::clipboard` | 在 `ClipboardOutput` 新增 `#[cfg(test)]` 內部 buffer |
| 鍵盤注入（enigo） | `output::injector` | 在 `FocusedWindowInjector` 新增 test-only flag 記錄最後注入文字 |
| HTTP 請求 | `models::downloader` | 用 `wiremock` 或 mock `reqwest::Client` |
| OpenCC 字典 | `transcription::opencc` | OpenCCProcessor 在 test mode 使用內建轉換；或 mock converter |
| 檔案系統 | `settings::persistence` | 使用 `tempfile::TempDir` 暫存目錄 |
| Slint UI | `ui::bridge` | 無法安全 mock；bridge 邏輯應抽離為純資料處理層後測試 |

### 5.2 建議的 mock 模式

```rust
// src/whisper/engine.rs — 測試用 stub
#[cfg(test)]
pub(crate) fn mock_engine() -> WhisperEngine {
    // 回傳一個總是成功的 stub
    unimplemented!("待實作：MockWhisperContext")
}

// src/output/clipboard.rs — test-only buffer
#[cfg(test)]
mod test {
    use std::sync::Mutex;
    static LAST_WRITE: Mutex<Option<String>> = Mutex::new(None);

    impl ClipboardOutput {
        pub fn test_last_written() -> Option<String> {
            LAST_WRITE.lock().unwrap().clone()
        }
    }
}
```

---

## 6. 測試優先級

### Phase 1 — 立即補上（無需外部依賴、高效益，2-3 天）

| 順序 | 模組 | 現有測試 | 應新增 | 核心價值 |
|------|------|---------|--------|---------|
| 1 | `whisper::params` | 0 | 3 | 確認參數預設值與序列化 |
| 2 | `transcription::postprocess` | 2 | +7 | 中文文字處理品質 |
| 3 | `context_templates::templates` | 0 | 4 | 模板邏輯正確性 |
| 4 | `models::registry` | 0 | 4 | 模型清單與 catalog 解析 |
| 5 | `file_transcription::mod` | 0 | 5 | 匯出格式 |

### Phase 2 — 抽象層測試（需輕度 mock）

| 順序 | 模組 | 現有測試 | 應新增 | 核心價值 |
|------|------|---------|--------|---------|
| 6 | `output::formatter` | 3 | +2 | |
| 7 | `transcription::opencc` | 2 | +0 | 已夠用 |
| 8 | `audio::resampler` | 0 | 4 | 重採樣品質 |
| 9 | `audio::vad` | 2 | +7 | VAD 行為 |
| 10 | `context_templates::detector` | 2 | +3 | 分類準確性 |
| 11 | `output::mod` (OutputRouter) | 0 | 6 | 輸出路由 |
| 12 | `models::manager` | 0 | 4 | 快取邏輯 |

### Phase 3 — 整合測試（需 mock 或合成資料）

| 順序 | 模組 | 應新增 | 核心價值 |
|------|------|--------|---------|
| 13 | `tests/audio_integration_tests.rs` | 3 | resampler+VAD 全流程 |
| 14 | `transcription::pipeline` | 3 | pipeline 組合邏輯 |
| 15 | `settings::config` | 5 | 設定序列化 |
| 16 | `settings::persistence` | 3 | 檔案讀寫 |
| 17 | `models::downloader` | 3 | 校驗邏輯 |
| 18 | `context_templates::custom` | 2 | 自訂模板持久化 |
| 19 | `tests/models_tests.rs` | 2 | 整合行為 |

### Phase 4 — 屬性測試與邊界

| 順序 | 測試 | 應新增 |
|------|------|--------|
| 20 | VAD 屬性測試 | 2 |
| 21 | PostProcessor 屬性測試 | 1 |
| 22 | ModelId 屬性測試 | 1 |

---

## 7. 不適合自動化測試的項目

| 項目 | 原因 | 替代驗證方式 |
|------|------|-------------|
| 實際麥克風錄音 | 需要硬體與環境 | 人工測試 + 合成音訊檔案 |
| Whisper 模型實際推論準確度 | 需要大量 GPU 資源 + GGUF 模型 | 僅在 release 前手動驗證 |
| UI 佈局 / 外觀 / 動畫 | Slint 無 headless 測試工具 | 人工視覺檢查 |
| 全域快捷鍵（RegisterHotKey） | 需要 OS 層級權限 | 人工測試 |
| PTT Hook（WH_KEYBOARD_LL） | 需要系統層級 hook | 人工測試 |
| 實際剪貼簿內容 | 可能與其他程式互動 | 人工測試 |
| 效能/延遲基準 | 需要特定環境 | 基準測試（benchmark） |

---

## 執行方式

```powershell
# 全部測試
cargo test

# 特定模組
cargo test --lib audio::vad

# 特定測試名稱
cargo test vad_loud_starts_speaking

# 測試含 stdout 輸出
cargo test -- --nocapture

# 忽略需要特定 feature 的測試
cargo test -- --skip needs_model

# 追蹤覆蓋率（需 tarpaulin）
cargo tarpaulin --out Html
```

---

## 覆蓋率目標

```
模組              現狀    目標    Phase 1  Phase 2  Phase 3  Phase 4
────────────────── ────── ────── ──────── ──────── ──────── ────────
audio::vad          ~40%   95%    ✓
audio::resampler    0      90%             ✓
whisper::params     0      95%    ✓
whisper::engine     0      40%                      ✓
models::registry    0      90%    ✓
models::manager     0      70%             ✓
models::downloader  0      50%                      ✓
transcription::*    ~30%   95%    ✓       ✓
output::*           ~15%   90%             ✓
settings::*         0      90%                      ✓
context_templates::* ~20%  90%    ✓       ✓
file_transcription  0      90%    ✓
────────────────── ────── ────── ──────── ──────── ──────── ────────
整體                ~5%    70%+
```
