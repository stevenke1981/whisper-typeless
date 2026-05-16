use std::path::PathBuf;
use tokio::fs;

use super::templates::ContextTemplate;

pub struct CustomTemplateStore {
    path: PathBuf,
}

impl CustomTemplateStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join("templates.json"),
        }
    }

    pub async fn load(&self) -> Vec<ContextTemplate> {
        let Ok(content) = fs::read_to_string(&self.path).await else {
            return Vec::new();
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    pub async fn save(&self, templates: &[ContextTemplate]) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(templates)?;
        fs::write(&self.path, json).await?;
        Ok(())
    }
}
