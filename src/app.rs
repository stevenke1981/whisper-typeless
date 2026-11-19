use std::sync::Arc;
use tokio::sync::RwLock;

use crate::context_templates::TemplateId;
use crate::models::ModelInfo;
use crate::settings::{migration, AppConfig, ConfigPersistence, ModuleRegistry};

#[derive(Debug, Clone, PartialEq)]
pub enum RecordingState {
    Idle,
    Recording,
    Processing,
}

pub struct AppState {
    pub recording_state: RecordingState,
    pub current_model: Option<ModelInfo>,
    pub current_template: TemplateId,
    pub audio_level: f32,
    pub config: Arc<RwLock<AppConfig>>,
    pub module_registry: Arc<RwLock<ModuleRegistry>>,
    /// 啟動時的設定警告（設定檔損毀、舊設定匯入失敗等），由 UI 顯示於錯誤紀錄
    pub startup_warnings: Vec<String>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Arc<RwLock<Self>>> {
        let (config, startup_warnings) = match ConfigPersistence::default_location() {
            Ok(persistence) => {
                let startup =
                    migration::load_startup_config(&persistence, &migration::legacy_candidates());
                (startup.config, startup.warnings)
            }
            Err(e) => {
                tracing::error!("無法取得設定檔路徑: {e}");
                (
                    AppConfig::default(),
                    vec![format!(
                        "無法取得設定檔路徑（{e}），本次使用預設設定且無法儲存"
                    )],
                )
            }
        };
        let module_registry = ModuleRegistry::with_all_modules(&config);

        let state = Self {
            recording_state: RecordingState::Idle,
            current_model: None,
            current_template: TemplateId::Casual,
            audio_level: 0.0,
            config: Arc::new(RwLock::new(config)),
            module_registry: Arc::new(RwLock::new(module_registry)),
            startup_warnings,
        };

        Ok(Arc::new(RwLock::new(state)))
    }
}
