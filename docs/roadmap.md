# 路線圖（Roadmap）

本文件取代已刪除的 `plan.md`、`todos.md`、`test.md`，只保留仍未完成或仍有效的項目。已完成的項目（例如 CI、隱私模式、音訊常數抽取、`ui_update!` 巨集、GGUF 檔名支援）不再列出，細節見 [CHANGELOG.md](../CHANGELOG.md)。

## 已知問題（優先處理）

1. **文件與實作的差異**
   `docs/spec.md` 是初版設計草案（例如 whisper-rs 版本與部分參數），未實作的項目以程式碼為準。

## GUI 遷移（AcmeUI-Native）

1. **移除 Slint 備援**：待 AcmeUI 版實機驗證通過後，於下一版移除 `--ui slint`、`src/ui/`、`ui/*.slint`、`build.rs`，以及 `slint` / `slint-build` 相依。
2. **AcmeUI 缺口**：
   - 多行提示詞編輯器（AcmeUI 版目前沒有）。
   - 視窗縮放把手（resize grip）。
   - emoji 圖示後備字型（emoji 在 AcmeUI 文字繪製中的 fallback）。
   - 中文輸入法（IME）實機驗證：組字、候選字選取與送出。
3. **AcmeUI 相依升級**：目前固定在 `c2e0fd9`。升級時四個 crate 一起更新，並確認 `Cargo.lock` 中 `gpu-allocator` → `windows 0.62.2` 仍在。

## 功能待辦

- **降噪（noise_suppress）**：設定旗標存在，沒有實作。
- **說話者偵測（speaker_detect）**：設定旗標存在，沒有實作。
- **情境自動偵測**：目前為加權關鍵字比對（`detector.rs`），可再擴充詞典。
- **檔案模式輸出**：`src/core/services/file_mode.rs` 仍直接使用 `arboard` 與 `convert_chinese_text`，應改走 `TranscriptionPipeline` / `OutputRouter`。
- **模型目錄校驗值**：`model-catalog.json` 目前為 SHA-1（downloader 依長度自動判斷），之後可改填 SHA-256。
- **Prompt 組裝集中化**：原 plan 規劃的 `transcription::context` 尚未建立，初始提示詞目前由模板提供。
- **後處理效能**：`PostProcessor` 的贅字移除會多次配置字串，重複句偵測為 O(n²)。可改為單次掃描或一次性取代。
- **相依套件升級**：`enigo` 0.2 → 0.3，需確認 Windows 注入行為。
- **舊設定遷移**：0.3.0 保留的 `settings.json` 遷移路徑，確認使用者大多已遷移後可移除。
- **測試補強**：設定檔序列化往返測試、`ModelManager` 掃描測試、模板 TOML 錯誤路徑測試。仍須遵守 [AGENTS.md](../AGENTS.md) 的規則：不依賴音訊、GPU、網路或模型檔案。
- **效能調校**：VAD 延遲與記憶體使用量尚未量測。

## 平台與發布

- **平台**：目前只支援 Windows。全域快捷鍵、按住說話、注入與視窗圖示都是 Win32 實作；macOS / Linux 沒有實作，也尚未規劃。
- **安裝程式**：目前以 zip 發布。NSIS 安裝程式與 macOS DMG 尚未製作。
