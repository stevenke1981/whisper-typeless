//! 使用者操作（[`UiCommand`]）與需要由畫面層執行的動作（[`ViewRequest`]）。

use crate::settings::secret::Secret;
use crate::settings::ui_settings::UiSettings;

/// 畫面層送進 [`Controller::handle`](super::Controller::handle) 的使用者操作。
///
/// 前半段一對一對應 Slint `AppWindow` 的每個 callback；後半段是讓其他畫面層
/// （例如 AcmeUI）不必自行改狀態欄位的設定型指令。
#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    // ── 與 Slint callback 一對一 ──
    /// 浮動 ↔ 完整視窗（`toggle-floating-mode`；參數為切換後的值）。
    SetFloatingMode(bool),
    /// 浮動視窗展開 ↔ 收合（`toggle-floating-expanded`；參數為切換後的值）。
    SetFloatingExpanded(bool),
    /// 關閉（隱藏）視窗（`request-close`）。
    RequestClose,
    /// 開始拖曳視窗（`start-drag`）。
    StartDrag,
    /// 清空錯誤紀錄（`clear-error-log`）。
    ClearErrorLog,
    /// 開始 / 停止錄音（`toggle-recording`；熱鍵、PTT、Space 也走這條）。
    ToggleRecording,
    /// 清空轉錄紀錄（`clear-transcript`）。
    ClearTranscript,
    /// 選擇情境模板（`template-selected`）。
    TemplateSelected(String),
    /// 設定值已變更：重新套用熱鍵 / PTT（`settings-changed`）。
    SettingsChanged,
    /// 將目前設定寫入 config.toml（`save-settings`）。
    SaveSettings,
    /// 開啟模型資料夾（`open-model-manager`）。
    OpenModelManager,
    /// 選用模型，必要時下載（`model-select-or-download`）。
    ModelSelectOrDownload(String),
    /// 開啟音訊檔選擇對話框（`open-file-dialog`）。
    OpenFileDialog,
    /// 開始轉錄已選的檔案（`start-file-transcription`）。
    StartFileTranscription,
    /// 複製檔案轉錄結果（`file-copy-result`）。
    FileCopyResult,
    /// 匯出檔案轉錄結果（`file-export-result`；參數為副檔名格式）。
    FileExportResult(String),
    /// 新增 / 修改模板（`template-save`）。
    TemplateSave {
        original: String,
        name: String,
        prompt: String,
    },
    /// 刪除模板（`template-delete`）。
    TemplateDelete(String),

    // ── 設定型指令（Slint 透過雙向綁定直接改屬性，不需要這些）──
    SetSettingsOpen(bool),
    SetShowErrorLog(bool),
    SetDarkMode(bool),
    SetFileMode(bool),
    SetPrivacyMode(bool),
    SetClipboardEnabled(bool),
    SetInjectEnabled(bool),
    SetFileOutputFormat(String),
    /// 一次套用整份設定快照（只改狀態，不寫檔；寫檔請再送 [`UiCommand::SaveSettings`]）。
    UpdateSettings(Box<UiSettings>),

    // ── 雲端轉錄（WP-A）──
    /// 把 OpenAI 金鑰存入作業系統憑證存放區（不寫入 config.toml）。
    SetOpenAiKey(Secret),
    /// 刪除已儲存的 OpenAI 金鑰。
    ClearOpenAiKey,

    // ── 系統匣（WP-B）──
    /// 從系統匣叫回視窗（使用者主動操作，可取得焦點；熱鍵 / PTT 路徑不得送此指令）。
    ShowWindow,
    /// 結束程式。
    Quit,
}

/// 只有畫面層能做的動作（視窗尺寸、隱藏、拖曳）。由 `handle` 回傳。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewRequest {
    /// 依 `floating_mode` / `floating_expanded` 重設視窗尺寸與置頂。
    ApplyWindowGeometry,
    /// 隱藏視窗。
    HideWindow,
    /// 開始系統視窗拖曳。
    StartDrag,
    /// 顯示（從系統匣叫回）視窗。只回應使用者對畫面 / 系統匣的操作。
    ShowWindow,
    /// 結束事件迴圈並離開程式。
    Quit,
}
