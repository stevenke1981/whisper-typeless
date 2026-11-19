# CLAUDE.md

請先閱讀 [AGENTS.md](AGENTS.md)，其中包含建置指令、模組地圖與發版流程。

## 關鍵規則

- **隱私優先**：預設一律本機轉錄。只有使用者選擇 OpenAI 引擎並明確同意（`openai_consent`）後，語音才可送往 OpenAI 轉錄；除此之外唯一允許的網路行為是下載模型。API 金鑰以 Windows DPAPI 加密存在使用者設定資料夾（或環境變數 `OPENAI_API_KEY`），不得寫入 config.toml、日誌或錯誤紀錄。
- **結束前檢查**：`cargo fmt --all -- --check` 與 `cargo clippy --all-targets -- -D warnings` 必須通過。
- **測試不依賴硬體**：不得需要音訊裝置、GPU、網路或模型檔案。
- **使用者字串用 zh-TW**：UI 與錯誤訊息使用繁體中文；預設中文輸出為 zh-TW。
- **不擅自發版**：未經使用者明確要求，不建立或推送 `vX.Y.Z` tag。
