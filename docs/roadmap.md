# 路線圖（Roadmap）

本文件取代已刪除的 `plan.md`、`todos.md`、`test.md`，只保留仍未完成或仍有效的項目。已完成的項目（例如 CI、隱私模式、音訊常數抽取、`ui_update!` 巨集、GGUF 檔名支援）不再列出，細節見 [CHANGELOG.md](../CHANGELOG.md)。

## 已知問題（優先處理）

1. **應用內模型下載的校驗不一致**
   `src/models/downloader.rs` 以 SHA-256 計算並比對，但 `model-catalog.json` 每個模型的欄位仍是 `sha1`（40 字元的 SHA-1 值）。結果是所有應用內下載都會在校驗階段失敗。`scripts/download-model.ps1` 仍以 SHA-1 正確校驗。
   修正方向（擇一）：
   - 將 `model-catalog.json` 改為 `sha256` 欄位，並以來源的 SHA-256 值重新填入；或
   - 短期內讓 downloader 同時接受 SHA-1 與 SHA-256。
   修正後請同步更新 [MODEL_CATALOG.md](../MODEL_CATALOG.md) 與 README 的疑難排解段落。

2. **文件與實作的差異**
   `docs/spec.md` 是初版設計草案（例如 whisper-rs 版本與部分參數），未實作的項目以程式碼為準。

## 功能待辦

- **降噪（noise_suppress）**：設定旗標存在，沒有實作。
- **說話者偵測（speaker_detect）**：設定旗標存在，沒有實作。
- **視窗內鍵盤快捷鍵**：規格中的 `Space` 錄音、`E` 展開尚未實作；目前只有 `Esc`（`ui/appwindow.slint`）。
- **情境自動偵測**：目前為關鍵字比對（`detector.rs`），可擴充詞典或加入權重。
- **Prompt 組裝集中化**：原 plan 規劃的 `transcription::context` 尚未建立，初始提示詞目前由模板提供。
- **後處理效能**：`PostProcessor` 的贅字移除會多次配置字串，重複句偵測為 O(n²)。可改為單次掃描或一次性取代。
- **相依套件升級**：`enigo` 0.2 → 0.3，需確認 Windows 注入行為。
- **舊設定遷移**：0.3.0 保留的 `settings.json` 遷移路徑，確認使用者大多已遷移後可移除。
- **測試補強**：設定檔序列化往返測試、`ModelManager` 掃描測試、模板 TOML 錯誤路徑測試。仍須遵守 [AGENTS.md](../AGENTS.md) 的規則：不依賴音訊、GPU、網路或模型檔案。
- **效能調校**：VAD 延遲與記憶體使用量尚未量測。

## 平台與發布

- **平台**：目前只支援 Windows。全域快捷鍵、按住說話、注入與視窗圖示都是 Win32 實作；macOS / Linux 沒有實作，也尚未規劃。
- **安裝程式**：目前以 zip 發布。NSIS 安裝程式與 macOS DMG 尚未製作。
