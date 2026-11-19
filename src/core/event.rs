//! 背景工作 → 畫面的更新事件（[`CoreEvent`]）。
//!
//! 背景執行緒只透過 [`Notifier::send`](super::Notifier::send) 送出事件；
//! 畫面執行緒收到喚醒後呼叫 [`EventQueue::drain`](super::EventQueue::drain)，
//! 再逐一交給 [`Controller::apply`](super::Controller::apply) 改寫 [`AppViewState`](super::AppViewState)。

use super::state::{ModelEntryView, TemplateEntryView};

/// 系統匣選單 / 圖示操作（WP-B）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// 左鍵點圖示：顯示 ↔ 隱藏視窗。
    ToggleWindow,
    /// 開始 / 停止錄音。
    ToggleRecording,
    /// 開啟設定面板。
    OpenSettings,
    /// 結束程式。
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    // ── 一般 ──
    /// 狀態列文字。
    Status(String),
    /// 狀態列文字（錄音中則忽略，例如模型預載進度）。
    StatusIfIdle(String),
    /// 運算裝置標籤（CPU / CUDA…）。
    DeviceLabel(String),
    /// 全域錯誤紀錄有變動；`apply` 會向後端重新取快照。
    ErrorLogChanged,
    /// 「✓ 已儲存」提示到期。
    SaveStatusExpired,

    // ── 熱鍵 ──
    /// 切換錄音熱鍵（預設 Ctrl+Shift+W）。
    HotkeyPressed,
    /// PTT 按下（true）/ 放開（false），預設 Ctrl+Alt+L。
    PttKey(bool),

    // ── 錄音管線 ──
    /// 即時音量（VAD 語音持續中）。
    Level(f32),
    /// 波形條（Notifier 會合併：佇列中最多一筆，取最新值）。
    Waveform(Vec<f32>),
    /// VAD 偵測到開始說話。
    SpeechStarted,
    /// VAD 偵測到靜音。
    Silence,
    /// 暫時文字（覆寫 current_partial）。
    Partial(String),
    /// 錄音管線結束（波形歸零、音量歸零、處理中結束）。
    PipelineFinished,
    /// 模型尚未預載，轉錄前正在載入。
    ModelLoading,
    /// 模型載入失敗。
    ModelLoadFailed(String),
    /// 開始轉錄完整錄音。
    TranscribeStarted,
    /// 轉錄完成但沒有文字。
    TranscriptEmpty,
    /// 新的轉錄段落。
    TranscriptSegment {
        text: String,
        timestamp: String,
        status: String,
    },
    /// 轉錄失敗。
    TranscriptionFailed(String),

    // ── 模型 ──
    /// 重新整理模型清單（含目前選用模型的顯示名稱）。
    ModelEntries {
        entries: Vec<ModelEntryView>,
        model_name: Option<String>,
    },
    /// 開始下載模型。
    DownloadStarted { model_id: String, size_mb: u64 },
    /// 下載進度（0.0–1.0）。
    DownloadProgress(f32),
    /// 下載失敗。
    DownloadFailed(String),
    /// 模型已選用（下載完成或直接切換）。
    ModelSelected {
        model_id: String,
        display_name: String,
        entries: Vec<ModelEntryView>,
        status: String,
    },

    // ── 檔案模式 ──
    /// 使用者選了檔案（完整路徑）。
    FilePicked(String),
    /// 開始檔案轉錄。
    FileStarted,
    /// 檔案轉錄進度（0.0–1.0）。
    FileProgress(f32),
    /// 檔案轉錄完成。
    FileFinished(String),
    /// 檔案轉錄失敗（訊息顯示在結果區）。
    FileFailed(String),

    // ── 情境模板 ──
    /// 模板清單已更新。
    TemplatesUpdated {
        entries: Vec<TemplateEntryView>,
        status: Option<String>,
    },

    // ── 系統匣（WP-B）──
    /// 系統匣選單或圖示被操作。
    Tray(TrayAction),

    // ── 雲端轉錄（WP-A）──
    /// 是否有可用的 OpenAI 金鑰（只傳有無，不傳內容）。
    OpenAiKeyState(bool),
}
