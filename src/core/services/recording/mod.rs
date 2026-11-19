//! 即時錄音：音訊擷取 → 重採樣 / VAD / 波形 → 停止後整段轉錄並輸出。

pub(crate) mod io;
pub(crate) mod pipeline;
pub(crate) mod transcribe;

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::runtime::Handle as TokioHandle;
use tracing::info;

use crate::audio::AudioCapture;
use crate::core::backend::RecordingRequest;
use crate::core::Notifier;
use crate::settings::config::AppConfig;

use super::ModelRuntime;

// ─── 錄音控制句柄 ─────────────────────────────────────────────────────────────

pub(crate) struct RecordingHandle {
    _stream: cpal::Stream, // 保持 CPAL 串流存活
    stop_flag: Arc<AtomicBool>,
}

// cpal::Stream 在所有主流平台實作 Send
unsafe impl Send for RecordingHandle {}

impl RecordingHandle {
    /// 通知管線停止；drop 後 CPAL 串流結束。
    pub(crate) fn stop(self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

/// 開始錄音失敗的原因。
pub(crate) enum StartError {
    ModelMissing,
    Failed(String),
}

/// 建立音訊擷取與處理管線。
pub(crate) fn start_recording(
    tokio: &TokioHandle,
    config: &AppConfig,
    model_id: String,
    model_path: PathBuf,
    req: RecordingRequest,
    runtime: ModelRuntime,
    notifier: Notifier,
) -> Result<RecordingHandle, StartError> {
    info!("嘗試啟動錄音，模型: {}", model_id);
    if !model_path.exists() {
        tracing::error!("模型 {model_id} 尚未下載，等待使用者確認下載");
        return Err(StartError::ModelMissing);
    }

    // 建立音訊捕捉
    let capture = AudioCapture::new(config.audio.device_name.as_deref())
        .map_err(|e| StartError::Failed(format!("音訊裝置錯誤: {e}")))?;
    let sample_rate = capture.sample_rate();

    // tokio 通道（供 cpal 回呼使用 try_send）
    let (audio_tok_tx, audio_tok_rx) = tokio::sync::mpsc::channel::<Vec<f32>>(128);
    let (level_tok_tx, level_tok_rx) = tokio::sync::mpsc::channel::<f32>(128);

    // std 通道（供處理執行緒使用 try_recv / blocking）
    let (audio_std_tx, audio_std_rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(64);
    let (level_std_tx, level_std_rx) = std::sync::mpsc::sync_channel::<f32>(64);

    // 橋接：tokio → std
    // buffer 滿時丟棄 chunk（模型載入期間）而非退出，否則管線會提早結束
    tokio.spawn(async move {
        let mut rx = audio_tok_rx;
        while let Some(data) = rx.recv().await {
            let _ = audio_std_tx.try_send(data);
        }
    });
    tokio.spawn(async move {
        let mut rx = level_tok_rx;
        while let Some(lvl) = rx.recv().await {
            let _ = level_std_tx.try_send(lvl);
        }
    });

    // 啟動 CPAL 串流
    let stream = capture
        .start(audio_tok_tx, level_tok_tx)
        .map_err(|e| StartError::Failed(format!("無法啟動錄音串流: {e}")))?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = Arc::clone(&stop_flag);

    std::thread::spawn(move || {
        pipeline::run_recording_pipeline(
            pipeline::PipelineJob {
                model_id,
                model_path,
                sample_rate,
                req,
            },
            audio_std_rx,
            level_std_rx,
            stop_flag_thread,
            notifier,
            runtime,
        );
    });

    Ok(RecordingHandle {
        _stream: stream,
        stop_flag,
    })
}
