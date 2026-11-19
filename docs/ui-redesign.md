# UI 重新設計規格（v0.6）

> 對象：三位平行實作者。依據：`ui_snapshots` 快照（下稱「快照」，檔名省略 `@1.5x.png`）與 `src/ui_acme` 現況。
> 範圍：只動 AcmeUI 畫面層；視窗尺寸不變（完整 560x620、浮動列 380x52、展開 380x540）。熱鍵 / PTT 路徑不新增任何視窗動作。

## 1. 診斷

主要用法是「在別的程式按熱鍵說話」，但視窗把版面花在次要的東西上，而關鍵狀態幾乎看不到。

| # | 問題 | 證據 |
|---|------|------|
| D1 | **狀態不醒目**：就緒 / 辨識中 / 下載模型 42% 都只是左下角 11px 灰字；下載中與閒置畫面幾乎相同；「模型未下載」完全沒有畫面。 | `07_live_idle_empty` vs `12_live_model_downloading`、`11_live_processing` |
| D2 | **沒教熱鍵**：空白狀態寫「按下錄音按鈕開始語音輸入」，沒提全域熱鍵；熱鍵只在狀態列 11px，PTT 熱鍵用紅色，看起來像錯誤。 | `07_live_idle_empty` |
| D3 | **內容上方 4 層外框**（標題列、分頁、模板列、波形線共約 150px），錄音鍵卻在左下角。 | `07`–`11` |
| D4 | **Emoji 圖示糊成深色色塊**（CPU 渲染器沒有彩色字形），深色模式幾乎看不見；剪貼簿 / 注入切換只有色塊沒有文字。 | `07`、`15_file_empty`、`45_dark_live_recording` |
| D5 | **浮動列**：5 個無標籤圖示佔去一半寬度，辨識中文字只剩 5 個字；「📌」的藍底比錄音狀態還搶眼；閒置時只有「就緒」。 | `01_overlay_idle`、`02_overlay_recording` |
| D6 | **收合狀態開錯誤紀錄只看得到標題**，清單被 52px 高度裁掉。 | `06_overlay_error_log` |
| D7 | **設定是一條約 4 頁長的捲軸**，順序照實作而非使用頻率：開頭是 VAD 閾值 / 溫度 / 執行緒，最常用的「模型」在最底；浮動面板內約 6 頁。 | `18`–`21_settings_light_p*`、`42_overlay_settings` |
| D8 | **「儲存設定」在頁面中段**，其後還有模板、GPU、模型；不按就不會寫檔，使用者無從得知。深色模式不在 `UiSettings`，重開即失效。 | `20_settings_light_p3`、`33_settings_tray_gpu_p3` |
| D9 | **重複與術語**：「功能模組」6 個開關與其他設定重複（繁簡轉換、自動注入），其中 `mod_vad` / `mod_inject` / `mod_history` 在 `src/ui*` 以外以 grep 查不到讀取者（疑似無作用）；「VAD 閾值、溫度、注入、PTT」未解釋；引擎區標題是「雲端轉錄」，選本機時也一樣。 | `19`、`20` |
| D10 | **模型列顯示原始下載網址**；此版本沒有 GPU 時「使用 GPU」仍是開啟的綠色。 | `21_settings_light_p4` |
| D11 | **雲端狀態只在設定裡看得到**；主畫面看不出語音正送往 OpenAI，也看不出「選了雲端但缺金鑰 / 未同意而退回本機」。金鑰「清除」與「儲存」同等顯眼。 | `23_settings_cloud_consent_p2`、`10` |
| D12 | **檔案頁**：停用的「開始轉錄」像空白輸入框；轉錄中沒有百分比；完成後主按鈕仍是「開始轉錄」。 | `15`、`16_file_processing`、`17_file_result` |
| D13 | **對比與一致性**：`placeholder` #aeaeb2 在 #f5f5f7 上約 2:1，卻用在時間戳與空白提示；「開啟」有三種顏色（綠開關、藍分段、綠「選擇」）；辨識中的文字卡是灰底，像停用。 | `09_live_transcripts`、`10`、`21` |
| D14 | 快照假資料 `current_template: "會議"` 不是 id，模板列看不到選取狀態。 | `07`–`14` |

## 2. 設計

### 2.1 原則

1. 一眼看懂現在的狀態，以及下一步按什麼。
2. 常用的放主畫面，設定分 6 頁，每頁盡量一屏。
3. 所有設定即改即存，沒有儲存鈕。
4. 不用 emoji：圖示用圖元或單色符號，其餘用文字。
5. 「開啟」一律用 accent 藍；綠 = 成功，紅 = 錄音 / 危險，琥珀 = 需要注意（雲端、缺模型）。

### 2.2 Tokens（`theme.rs` / `widgets.rs`）

間距 `SP1..SP6` = 4 / 8 / 12 / 16 / 20 / 24。內容邊距 16，卡片內距 12，群組間距 16。

字級（沿用常數名）：`FONT_XS` 11（說明、時間戳）、`FONT_SM` 13（列標題、控制項）、`FONT_MD` 15（轉錄文字）、`FONT_LG` 17（頁標題）、新增 `FONT_XL` 20 粗體（狀態卡標題）。

圓角：`RADIUS_SM` 6（控制項）、`RADIUS_MD` 10（卡片）、新增 `RADIUS_LG` 14（狀態卡、浮動視窗）、膠囊 = 高 / 2。
高度：`CTRL_SM` 28、`CTRL_MD` 32、`CTRL_LG` 44；設定列 44（有說明時加說明高度 + 8）；開關 44x24。

| Token | 淺色 | 深色 | 用途 |
|-------|------|------|------|
| `bg` / `bg2` / `bg3` | f5f5f7 / ffffff / e8e8ed | 1c1c1e / 2c2c2e / 3a3a3c | 底 / 卡片 / 控制項底 |
| `text` / `text2` | 1d1d1f / 6e6e73 | f5f5f7 / 98989d | 主文字 / 說明、時間戳、空白提示 |
| `placeholder`（改） | 8e8e93 | 8e8e93 | 只用於輸入框提示與停用文字 |
| `accent` | 0071e3 | 0a84ff | 主按鈕、開啟狀態、選取 |
| `recording` / `recording_soft`（新） | ff3b30 / 10% | ff453a / 18% | 錄音鍵 / 錄音中卡片底 |
| `danger_text`（新） | d70015 | ff6961 | 危險文字 |
| `success` / `success_text`（新） | 34c759 / 1a7f37 | 30d158 / 30d158 | 成功 |
| `warning` / `warning_text` / `warning_soft` / `warning_line`（新） | ff9f0a / 9a5b00 / 14% / 40% | ff9f0a / ffb340 / 20% / 45% | 需要注意 |

沒有陰影圖元：層次只靠 `bg` → `bg2` 與 1px `border`。

### 2.3 元件（WP-1 提供，簽名見 4.2）

| 元件 | 規格 |
|------|------|
| 按鈕 `btn` | Primary（accent 底白字）、Secondary（`bg2` 底 + 1px `border_strong`）、Ghost（透明、`text2`、hover 才有底）、Danger（`danger_soft` 底 + `danger_text`）。`msg = None` 為停用：`bg3` 底、`placeholder` 字、無邊框。 |
| 開關 `toggle` | 44x24；開 = accent、關 = `bg3` + 1px `border_strong`；停用時整體 45% 透明度。 |
| 分段 `segmented_fill` | 一條 `bg3` 軌道（高 32、圓角 8），等寬選項；選取項是 `bg2` 膠囊 + 粗體 `text`，其餘 `text2`。 |
| 卡片 `card` | `bg2` + 1px `border` + 圓角 10；有色調時用該色的 soft 底與 line 邊框。 |
| 群組 `group_begin/end` | 11px 粗體 `text2` 標題（高 24）+ 一張卡片；列與列之間 1px `border`（左縮 12）。 |
| 設定列 `setting_row` | 左：13px 標題 + 可選的 11px `text2` 說明（自動換行）；右：寬 160 的控制區。 |
| 狀態膠囊 `status_pill` | 高 24，6px 圓點 + 11px 文字；色調 Neutral / Accent / Recording / Success / Warning / Danger；可點。 |
| 切換膠囊 `toggle_chip` | 高 28；開 = `chip_bg_active` + accent 粗體字與勾號；關 = `chip_bg` + `text2`。 |
| 文字框 | 沿用 `text_field`，高 32。金鑰欄維持遮蔽，提示字「貼上 sk-… 金鑰」。 |
| 圖示 `icon` | 16 格線、1.5px 線條，只用矩形、圓環、短線段：Gear、Close、Minimize、ChevronDown、ChevronUp、Window、Pin、Alert、Check、Download、Plus。 |

### 2.4 浮動列（380x52）

```text
┌──────────────────────────────────────────────────────────────┐
│▌ (●)  就緒                                 [⚙][˅][▢][✕]    │ 52
│▌ 36   Ctrl+Shift+W 開始 · 按住 Ctrl+Alt+L                    │
└──────────────────────────────────────────────────────────────┘
 0-4 狀態色條 │ 14-50 錄音鍵 36 │ 60..246 兩行文字（13 粗 / 11）│ 4x28 圖示，間距 2，右距 8
```

| 狀態 | 色條 | 第一行 | 第二行 |
|------|------|--------|--------|
| 就緒 | `border_strong` | 就緒 | `{hotkey} 開始`；`ptt_mode` 時加 ` · 按住 {ptt_hotkey}`（放不下就省略） |
| 錄音中 | `recording`，列底 `recording_soft` | 有 `current_partial` 顯示它（尾端省略），否則「聆聽中…」 | 迷你波形（24 條、高 14） |
| 辨識中 | `accent` | 辨識中… | 完成後會輸入到游標位置 |
| 需要模型 / 下載中 | `warning` | 尚未下載模型 / 下載模型 42% | 展開以完成設定 / 4px 進度條 |
| 有錯誤（閒置時） | `danger` | `status_text` | 點 ˅ 查看錯誤紀錄 |

圖示由左到右：設定、展開、切到完整視窗、關閉。置頂移到展開後的第二列。點設定或錯誤徽章時若為收合狀態，先展開（修 D6）。

展開（380x540）：

```text
 0-52    浮動列
 52-92   [ 即時 | 檔案 ]（分段，寬 136）                 [置頂] [! 3]
 92-132  模板列（mod_context 開啟時）
 132-160 紀錄標題列：「紀錄 · 143 字」                        清除
 160-492 轉錄清單
 492-540 頁尾：[自動貼上 ✓] [剪貼簿 ✓]            [本機 · small]
```

### 2.5 完整視窗：即時（560x620）

```text
┌──────────────────────────────────────────────────────────────────┐
│ whisper-typeless       [ 即時 | 檔案 ]            [▢] [⚙] [—] [✕]│ 0-44 標題列
├──────────────────────────────────────────────────────────────────┤
│ ┌──────────────────────────────────────────────────────────────┐ │ 52-140 狀態卡
│ │ (●)   就緒                                                   │ │ 528x88，圓角 14
│ │ 56    [Ctrl+Shift+W] 開始／停止 · 按住 [Ctrl+Alt+L] 說話      │ │ 錄音鍵 56 於 x=32
│ └──────────────────────────────────────────────────────────────┘ │
│ 自動  [會議]  口語  技術  醫療  法律                              │ 148-188 模板列（文字膠囊，高 28）
│ 紀錄 · 143 字                                              清除   │ 188-216 標題列
│ 10:05  接下來請小明報告雲端轉錄與隱私模式的測試結果。             │ 216-572 轉錄清單
│ ──────────────────────────────────────────────────────────────── │ 時間欄寬 48（11px text2）
│ 10:07  目前所有的語音都只在本機處理，只有模型下載會連網。         │ 文字 15px；列距 10；1px 分隔線
│ ▌辨識中  接下來我們討論一下下一個版本的                           │ 進行中列：accent_soft 底 + 3px accent 左條
├──────────────────────────────────────────────────────────────────┤
│ [自動貼上 ✓] [剪貼簿 ✓]               [本機 · small · CPU] [! 3] │ 572-620 頁尾
└──────────────────────────────────────────────────────────────────┘
```

- 標題列：移除主題切換（搬到設定 › 一般）與常駐錯誤徽章；錯誤徽章只在 `error_log` 非空時出現在頁尾。▢ = 切到浮動列。
- 狀態卡（`views::phase(s)`）：

| Phase | 色調 | 標題（20 粗） | 內文 | 右側 |
|-------|------|---------------|------|------|
| `NeedsModel` | Warning | 還差一步：下載語音模型 | 下載 {display_name}（{size_mb} MB）後即可開始，只需一次；語音只在本機處理。 | 「下載模型」Primary、「選其他模型」Ghost → `OpenSettings(Models)` |
| `Downloading` | Accent | 正在下載模型 42% | 6px 進度條；「下載完成後會自動載入」 | — |
| `Ready` | Neutral | 就緒 | 鍵帽樣式的 `{hotkey}`「開始／停止」，`ptt_mode` 時加「按住 `{ptt_hotkey}` 說話」 | — |
| `Recording` | Recording | 錄音中 | 波形（32 條、高 28） | — |
| `Processing` | Accent | 辨識中… | 完成後會自動輸入到游標所在的視窗 | — |

  錄音鍵在卡片左側（`NeedsModel` / `Downloading` 時不畫）。`status_text` 不是「就緒」且與標題不同時，取代內文顯示（核心的錯誤訊息仍看得到）。
- 空白紀錄（置中、`text2`、13px）：「在任何視窗按 {hotkey} 開始說話，文字會輸入到游標所在的位置。」第二行：「說完的句子會顯示在這裡。」
- 頁尾的引擎膠囊（點擊 → `OpenSettings(Engine)`）：

| 條件 | 文字 | 色調 |
|------|------|------|
| `privacy_mode` | 隱私模式 · 本機 | Success |
| `cloud_ready(s)` | 雲端 · OpenAI | Warning |
| `engine == "openai"` 但未同意或無金鑰 | 雲端未完成設定 · 使用本機 | Danger |
| 其他 | 本機 · {model_name} · {short_device} | Neutral |

- 「自動貼上」= `inject_enabled`，「剪貼簿」= `clipboard_enabled`。兩者皆關時，頁尾顯示 Warning 膠囊「文字不會輸出」。

### 2.6 檔案頁

```text
 52-124  檔案卡 528x72：未選：「選擇音訊或影片檔…」+ [選擇檔案] Primary
                         已選：檔名（15 粗，中段省略）+ [更換] Secondary
 136-168 輸出格式  [ TXT | SRT | VTT ]（分段，寬 180）
 180-224 [開始轉錄] Primary 44，全寬；未選檔時停用
         轉錄中：6px 進度條 + 「轉錄中 37%」，按鈕停用並顯示「轉錄中…」
         完成後：按鈕變 Secondary「重新轉錄」，上方 Success 膠囊「轉錄完成」
 236-572 結果卡：標題列「轉錄結果」 [複製] Secondary  [匯出 TXT] Primary
 頁尾    「檔案轉錄一律使用本機模型」（11px text2）
```

### 2.7 設定（6 個分頁）

```text
┌──────────────────────────────────────────────────────────────────┐
│ 設定                                    ● 已自動儲存         [✕] │ 0-44
│ [ 一般 | 辨識 | 模型 | 輸出 | 模板 | 進階 ]                       │ 44-88 分段，高 32，邊距 16
│ 快捷鍵                                                            │ 88-620 可捲動；每頁各自從頂端開始
│ ┌──────────────────────────────────────────────────────────────┐ │
│ │ 開始／停止錄音                               [ Ctrl+Shift+W ] │ │ 44
│ │ 按住說話                                                (●)  │ │ 44 + 說明
│ │ 按住快捷鍵時錄音，放開就送出辨識                              │ │
│ │ 按住說話快捷鍵                               [ Ctrl+Alt+L ]  │ │ 44
│ └──────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

浮動面板（380 寬）用同一版面：分頁每格約 58，控制區縮為 132。

| 分頁 | 群組 › 列（標題｜說明｜控制項） |
|------|--------------------------------|
| **一般** | **快捷鍵**：開始／停止錄音｜—｜文字框；按住說話｜按住快捷鍵時錄音，放開就送出辨識｜開關；按住說話快捷鍵｜—｜文字框（按住說話關閉時停用）。**視窗**：關閉時縮到系統匣；最小化時縮到系統匣；開機後直接縮到系統匣｜皆為開關。**外觀**：主題｜—｜分段「淺色 / 深色」；顯示波形（`mod_waveform`）；顯示情境模板列（`mod_context`）。 |
| **辨識** | **轉錄引擎**：引擎｜本機：語音不離開電腦。雲端：語音會傳到 OpenAI。｜分段「本機 / OpenAI 雲端」（隱私模式下停用）。**OpenAI 雲端**（引擎為 openai 時才出現，Warning 色調卡片）：我同意把語音傳送到 OpenAI｜需要網路，費用由你的 API 帳號支付；檔案轉錄仍使用本機模型。｜開關；API 金鑰｜以 Windows 加密保存在本機，不寫入設定檔。｜膠囊「已設定」Success 或「未設定」Danger，下一列是遮蔽文字框 + 「儲存」Primary，已設定時最右是「移除金鑰」Ghost；雲端模型｜—｜分段。**隱私**：隱私模式｜一律本機轉錄、不保留錄音檔，輸出後清除剪貼簿。｜開關。 |
| **模型** | **效能**：使用 GPU 加速（{backend}）｜變更在下次載入模型時生效；若當機請關閉。｜開關（此版本不含 GPU 時停用且顯示為關，說明改為「此版本不含 GPU；需要時請改用 Vulkan 版」）。**語音模型**（列高 56）：名稱（13 粗）＋「準確度 ★★★☆☆ · 466 MB」；右側：使用中 = Success 膠囊；已下載 = 「使用」Secondary；未下載 = 「下載」Secondary + Download 圖示；下載中 = 進度條 + 百分比。不顯示網址。清單下方：「建議使用 Tiny / Base / Small；較大的模型下載前會再次確認。」與「開啟模型資料夾」Ghost。 |
| **輸出** | **文字**：中文輸出｜—｜分段「台灣繁體 / 香港繁體 / 簡體 / 不轉換」；結尾自動換行（`append_newline`）。**送出方式**：自動貼到游標位置（`inject_enabled`）｜辨識完成後直接輸入到你正在使用的視窗。；複製到剪貼簿（`clipboard_enabled`）。 |
| **模板** | **情境模板**：清單（列高 40；內建項目有「內建」膠囊；選取列 `accent_soft`）+ 「新增模板」Secondary。選取後下方出現編輯卡：名稱、提示詞（說明：告訴辨識引擎這段話的情境與常見用詞）、「儲存」Primary、「刪除」Danger（內建不顯示）。 |
| **進階** | **語音偵測**：收音靈敏度（`vad_threshold`）｜數值越高，越不容易把雜音當成說話。｜滑桿 + 數值；停頓多久後送出（`silence_timeout`）｜—｜步進器，顯示「1.5 秒」。**辨識**：隨機性（溫度）｜0 最穩定，一般不需調整。｜滑桿；CPU 執行緒｜—｜步進器。**診斷**：錯誤紀錄｜共 {n} 筆｜「查看」Secondary。 |

- 「中文輸出」的選取值：`!mod_opencc` 時顯示「不轉換」；選任一項同時送 `ConversionMode(id)` 與 `Module(OpenCc, id != "raw")`。
- `mod_vad` / `mod_inject` / `mod_history` 不再顯示（欄位保留、不改值）。
- 標題列膠囊：`save_status` 為「✓ 已儲存」→ Success「已自動儲存」；失敗 → Danger「儲存失敗，請查看錯誤紀錄」。

### 2.8 錯誤紀錄

卡片（最大 480x420，置中）：標題「錯誤紀錄（3）」、「清空」Danger、「關閉」Secondary。每筆一列：時間欄 64（`text2` 11px）+ 訊息（`text` 13px，自動換行），列間 1px 分隔線。空白時置中顯示「目前沒有錯誤」。

### 2.9 互動與快捷鍵

- **即改即存**（`app.rs`）：開關、分段、步進器變更後立即送 `SaveSettings`；滑桿在放開時送；快捷鍵文字框在失焦時送（照舊重新註冊熱鍵）。`SetPrivacyMode` / `SetClipboardEnabled` / `SetInjectEnabled` / `SetDarkMode` 之後同樣送 `SaveSettings`。
- 視窗內快捷鍵（在 `app.rs` 處理，先於 `key_to_command`；文字框有焦點時不作用）：`Ctrl+,` 開關設定；`Ctrl+1` / `Ctrl+2` 即時 / 檔案；設定開啟時 `Ctrl+1..6` 切分頁。既有的 Esc / Space / E 不變。
- 切換分頁時把 `ScrollId::Settings` 歸零。
- 展開浮動面板只發生在使用者點擊（設定、錯誤徽章、˅）；熱鍵 / PTT 只會改變色條與文字，不改尺寸、不取得焦點。

## 3. 範圍守則

- 不新增 crate，不升級 AcmeUI rev，不改 `UiCommand` / `ViewRequest` / `CoreEvent`。
- 唯一的核心 / 設定層異動（C-1，WP-1 負責）：讓深色模式可持久化。
  - `src/settings/ui_settings.rs`：`UiSettings` 新增 `pub dark_mode: bool`；`from_config` 取 `general.theme == "dark"`；`apply_to` 在 `dark_mode` 為真時寫 `"dark"`，為假且原值是 `"dark"` 時寫 `"light"`，其餘（例如 `"auto"`）保留。
  - `src/core/state.rs`：`apply_ui_settings` / `ui_settings` 各加一行對應 `dark_mode`。
- 其餘皆為畫面層新增：
  - `msg.rs`：`enum SettingsTab { General, Engine, Models, Output, Templates, Advanced }`（含 `ALL`、`label()`）；`Msg::OpenSettings(SettingsTab)`、`Msg::SettingsTab(SettingsTab)`、`Msg::ToggleErrorLog`；`SettingEdit::{AppendNewline(bool), CloseToTray(bool), MinimizeToTray(bool), StartMinimized(bool)}`。
  - `local.rs`：`UiLocal.settings_tab: SettingsTab`（預設 `General`）。
  - `paint.rs`：`Item::Line`、`Item::Ring` 與 `View::line` / `View::ring`（對應 `Painter::line` / `Painter::ring`）。
- Slint 備援（`src/ui`）不動。P2（有餘力才做，WP-1）：快捷鍵欄改為「按下組合鍵即錄入」。

## 4. 工作包

### 4.1 檔案歸屬與順序

| WP | 擁有的檔案 | 內容 |
|----|-----------|------|
| **WP-1 基礎** | `theme.rs`、`paint.rs`、`icons.rs`（新）、`msg.rs`、`local.rs`、`app.rs`、`app_tests.rs`、`snapshots.rs`、`views/mod.rs`、`views/widgets.rs`、`src/settings/ui_settings.rs`、`src/core/state.rs` | Tokens、元件、圖示、新 `Msg`、即改即存、快捷鍵、C-1、快照場景 |
| **WP-2 主畫面** | `views/live.rs`、`views/file.rs`、`views/overlay.rs` | 2.4、2.5、2.6、2.8 |
| **WP-3 設定與首次使用** | `views/settings.rs`（可拆成 `views/settings/*.rs`）、`views/onboarding.rs`（新） | 2.7，以及狀態卡的 `NeedsModel` / `Downloading` |

順序：

1. WP-1 先交「契約提交」：4.2 的全部簽名與可用實作、`onboarding.rs` 空殼、新快照場景、修正 D14。舊元件函式暫時保留，讓現有畫面照常編譯。
2. WP-2、WP-3 從契約提交開分支平行實作，WP-1 同時做即改即存、快捷鍵、C-1。
3. 合併順序 WP-1 → WP-2 → WP-3；之後 WP-1 刪除沒人用的舊元件並重產全部快照。

WP-2 / WP-3 只能在自己的檔案內加 `#[cfg(test)]` 測試；`app_tests.rs` 中因畫面改版而失效的斷言，由各 WP 只改與自己畫面相關的測試函式。需要新元件時向 WP-1 提出，不自行修改 `widgets.rs`。

### 4.2 契約 API

```rust
// theme.rs
pub enum Tone { Neutral, Accent, Recording, Success, Warning, Danger }
pub struct ToneColors { pub fg: Color, pub soft: Color, pub line: Color, pub solid: Color }
impl Theme { pub fn tone(&self, tone: Tone) -> ToneColors; }
// Theme 新欄位：recording_soft, danger_text, success_text, warning_text, warning_soft, warning_line

// icons.rs
pub enum Icon { Gear, Close, Minimize, ChevronDown, ChevronUp, Window, Pin, Alert, Check, Download, Plus }
pub fn icon(v: &mut View, rect: Rect<Logical>, icon: Icon, color: Color);

// views/mod.rs
pub enum Phase { NeedsModel, Downloading, Recording, Processing, Ready }
pub fn phase(s: &AppViewState) -> Phase;
pub fn cloud_ready(s: &AppViewState) -> bool;   // engine=="openai" && openai_consent && openai_key_set && !privacy_mode
pub fn needs_model(s: &AppViewState) -> bool;   // !cloud_ready && 選用的模型存在且未下載 && downloading_id 為空

// views/widgets.rs（msg = None 表示停用）
pub const FONT_XL: f32 = 20.0;  pub const RADIUS_LG: f32 = 14.0;
pub const SP1: f32 = 4.0;  /* SP2..SP6 = 8, 12, 16, 20, 24 */
pub const CTRL_SM: f32 = 28.0;  pub const CTRL_MD: f32 = 32.0;  pub const CTRL_LG: f32 = 44.0;
pub enum Btn { Primary, Secondary, Ghost, Danger }
pub fn btn(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>, label: &str, icon: Option<Icon>, kind: Btn, msg: Option<Msg>);
pub fn icon_btn(v: &mut View, t: &Theme, rect: Rect<Logical>, icon: Icon, active: bool, msg: impl Into<Msg>);
pub fn toggle(v: &mut View, t: &Theme, rect: Rect<Logical>, on: bool, msg: Option<Msg>);
pub fn segmented_fill(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>, options: &[(&str, &str)],
                      selected: &str, enabled: bool, make: impl Fn(&str) -> Msg);
pub fn card(v: &mut View, t: &Theme, rect: Rect<Logical>, radius: f32, tone: Tone);
pub fn status_pill(v: &mut View, cx: &mut Ctx<'_>, x: f32, y: f32, label: &str, tone: Tone, msg: Option<Msg>) -> f32; // 寬
pub fn toggle_chip(v: &mut View, cx: &mut Ctx<'_>, x: f32, y: f32, label: &str, on: bool, msg: impl Into<Msg>) -> f32; // 寬
pub fn keycap(v: &mut View, cx: &mut Ctx<'_>, x: f32, y: f32, label: &str) -> f32; // 寬，高 22
pub struct Group { pub x: f32, pub y: f32, pub w: f32, /* 私有：卡片 Item 的索引 */ }
pub fn group_begin(v: &mut View, cx: &mut Ctx<'_>, x0: f32, y0: f32, w: f32, title: &str, tone: Tone) -> Group;
pub fn group_end(v: &mut View, g: Group, y_end: f32) -> f32;      // 補上卡片高度，回傳下一個群組的 y
pub fn setting_row(v: &mut View, cx: &mut Ctx<'_>, g: &Group, y: f32, title: &str, hint: Option<&str>,
                   enabled: bool, divider: bool) -> (f32, Rect<Logical>); // (列高, 控制區)
// 沿用：text_field, slider, stepper, progress, waveform, record_button

// views/onboarding.rs（WP-3；WP-2 在 phase 為 NeedsModel / Downloading 時呼叫）
pub fn card(v: &mut View, cx: &mut Ctx<'_>, rect: Rect<Logical>);
```

### 4.3 新增的快照場景（WP-1 於契約提交加入）

`overlay_processing`、`overlay_needs_model`、`overlay_expanded_idle`、`live_needs_model`、`live_cloud_active`、`live_cloud_incomplete`、`live_privacy`、`live_no_errors`、`live_output_off`、`file_selected`、`error_log_empty`；設定改為每個分頁一張：`settings_general`、`settings_engine_local`、`settings_engine_cloud`、`settings_engine_cloud_no_key`、`settings_engine_privacy`、`settings_models`、`settings_models_downloading`、`settings_output`、`settings_templates`、`settings_templates_edit`、`settings_advanced`、`settings_general_dark`、`overlay_settings_general`（模型與模板頁可沿用 `settings_series` 捲動）。假資料的 `current_template` 改為 `"meeting"`。

### 4.4 驗收（看快照）

共通：

- `rg -n "[\x{1F300}-\x{1FAFF}\x{2600}-\x{27BF}]" src/ui_acme/views` 找不到 emoji（單色的 ✓ ✕ ★ ☆ 例外）。
- 深色與淺色快照中，沒有任何說明或時間戳文字使用 `placeholder`。
- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 通過。

WP-1：

- 任一畫面都看不到綠色開關；所有「開啟」狀態都是 accent 藍。
- 測試：開關設定後 `sent` 以 `SaveSettings` 結尾；滑桿拖曳中不送，放開後送一次；`Ctrl+,` 切換 `settings_open`；收合狀態送 `Msg::ToggleErrorLog` 會先展開。
- 測試：`UiSettings.dark_mode` 經 config 來回後不變，且 `theme = "auto"` 在淺色時不被改寫。
- 熱鍵 / PTT 相關的既有測試（不取得焦點、不改尺寸）維持通過。

WP-2：

- `overlay_idle`：看得到熱鍵提示，右側只有 4 個圖示。`overlay_recording`：紅色色條與淡紅底，辨識中文字至少顯示 10 個中文字。`overlay_processing` 與 `overlay_needs_model` 的色條和文字與閒置明顯不同。
- `overlay_error_log` 改為展開尺寸拍攝，看得到三筆錯誤。
- `live_idle_empty`：狀態卡顯示「就緒」與兩個鍵帽；空白提示包含熱鍵；第一筆內容的起點不低於 y=216。
- `live_idle_empty`、`live_recording`、`live_processing`、`live_model_downloading`、`live_needs_model` 五種狀態卡縮成縮圖也分得出來（色調與標題不同）；`live_model_downloading` 有進度條。
- `live_transcripts`：沒有逐筆卡片外框；時間欄對齊；模板列看得到「會議」被選取。
- `live_cloud_active` 頁尾有琥珀色「雲端 · OpenAI」；`live_cloud_incomplete` 有紅色「雲端未完成設定 · 使用本機」；`live_privacy` 有綠色「隱私模式 · 本機」；`live_no_errors` 沒有錯誤徽章；`live_output_off` 有「文字不會輸出」。
- `file_empty`：主按鈕是明顯停用的實心灰；`file_processing` 顯示百分比；`file_result` 的主按鈕是「匯出 TXT」而不是「開始轉錄」。

WP-3：

- 六個分頁快照都看得到分頁列，且目前分頁被選取；`settings_general`、`settings_output`、`settings_advanced`、`settings_engine_local` 在 560x620 不需捲動。
- 任何設定快照都沒有「儲存設定」按鈕、沒有「功能模組」區段、沒有網址。
- `settings_engine_cloud`：琥珀色卡片內依序為同意、金鑰（綠色「已設定」）、雲端模型；`settings_engine_cloud_no_key` 顯示紅色「未設定」且沒有「移除金鑰」；`settings_engine_privacy` 的引擎分段呈停用並有說明。
- `settings_models`：使用中的模型是綠色膠囊，其餘是「使用」或「下載」；此版本不含 GPU 時開關呈停用的關閉狀態。`settings_models_downloading` 有進度條與百分比。
- `overlay_settings_general`（380 寬）：六個分頁標籤完整顯示，控制項不超出右邊界。
- `live_needs_model`：卡片上有「下載模型」主按鈕，點擊送出 `ModelSelectOrDownload(selected_model_id)`（單元測試）。
