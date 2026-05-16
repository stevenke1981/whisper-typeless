use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use super::registry::{ModelInfo, ModelRegistry};
use crate::whisper::{WhisperEngine, WhisperParams};

type LoadedEngine = Option<(String, Arc<WhisperEngine>)>;

pub struct ModelManager {
    models_dir: PathBuf,
    loaded_engine: Arc<Mutex<LoadedEngine>>,
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            loaded_engine: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn list_available(&self) -> Vec<ModelInfo> {
        let mut models = ModelRegistry::all();
        for model in &mut models {
            let path = self.models_dir.join(format!("ggml-{}.bin", model.id));
            if path.exists() {
                model.local_path = Some(path);
                model.is_downloaded = true;
            }
        }
        models
    }

    pub async fn load(
        &self,
        model_id: &str,
        params: &WhisperParams,
    ) -> anyhow::Result<Arc<WhisperEngine>> {
        let mut lock = self.loaded_engine.lock().await;

        if let Some((id, engine)) = &*lock {
            if id == model_id {
                return Ok(Arc::clone(engine));
            }
        }

        let path = self.models_dir.join(format!("ggml-{model_id}.bin"));
        if !path.exists() {
            return Err(anyhow::anyhow!("模型未下載: {model_id}"));
        }

        info!("載入模型: {model_id}");
        let engine = Arc::new(WhisperEngine::load(&path, params)?);
        *lock = Some((model_id.to_string(), Arc::clone(&engine)));

        Ok(engine)
    }

    pub async fn unload(&self) {
        *self.loaded_engine.lock().await = None;
        info!("模型已卸載");
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }
}
