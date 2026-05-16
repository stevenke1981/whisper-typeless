use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use super::{TranscriptResult, WhisperEngine, WhisperParams};
use crate::audio::AudioSegment;

pub struct TranscriptionSession {
    engine: Arc<WhisperEngine>,
    params: Arc<RwLock<WhisperParams>>,
}

impl TranscriptionSession {
    pub fn new(engine: Arc<WhisperEngine>, params: Arc<RwLock<WhisperParams>>) -> Self {
        Self { engine, params }
    }

    pub async fn run(
        &self,
        mut segment_rx: mpsc::Receiver<AudioSegment>,
        result_tx: mpsc::Sender<TranscriptResult>,
    ) {
        info!("轉錄 Session 啟動");

        while let Some(segment) = segment_rx.recv().await {
            let params = self.params.read().await.clone();
            let engine = Arc::clone(&self.engine);

            let tx = result_tx.clone();
            tokio::task::spawn_blocking(move || match engine.transcribe(&segment, &params) {
                Ok(result) => {
                    if !result.text.is_empty() {
                        let _ = tx.blocking_send(result);
                    }
                }
                Err(e) => tracing::error!("轉錄失敗: {e}"),
            });
        }
    }
}
