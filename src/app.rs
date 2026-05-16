use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::context_templates::TemplateId;
use crate::models::ModelInfo;
use crate::settings::{AppConfig, ModuleRegistry};

#[derive(Debug, Clone, PartialEq)]
pub enum RecordingState {
    Idle,
    Recording,
    Processing,
}

#[derive(Debug, Clone)]
pub struct TranscriptEntry {
    pub text: String,
    pub timestamp_ms: u64,
    pub template_id: TemplateId,
}

pub struct AppState {
    pub recording_state: RecordingState,
    pub current_model: Option<ModelInfo>,
    pub current_template: TemplateId,
    pub transcript_history: VecDeque<TranscriptEntry>,
    pub audio_level: f32,
    pub last_error: Option<String>,
    pub config: Arc<RwLock<AppConfig>>,
    pub module_registry: Arc<RwLock<ModuleRegistry>>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Arc<RwLock<Self>>> {
        let config = AppConfig::load_or_default()?;
        let module_registry = ModuleRegistry::with_all_modules(&config);

        let state = Self {
            recording_state: RecordingState::Idle,
            current_model: None,
            current_template: TemplateId::Casual,
            transcript_history: VecDeque::with_capacity(1000),
            audio_level: 0.0,
            last_error: None,
            config: Arc::new(RwLock::new(config)),
            module_registry: Arc::new(RwLock::new(module_registry)),
        };

        Ok(Arc::new(RwLock::new(state)))
    }

    pub fn push_transcript(&mut self, entry: TranscriptEntry) {
        if self.transcript_history.len() >= 1000 {
            self.transcript_history.pop_front();
        }
        self.transcript_history.push_back(entry);
    }
}
