use super::config::AppConfig;
use std::path::PathBuf;
use tokio::fs;

pub struct ConfigPersistence {
    path: PathBuf,
}

impl ConfigPersistence {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn load(&self) -> anyhow::Result<AppConfig> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }
        let content = fs::read_to_string(&self.path).await?;
        Ok(toml::from_str(&content).unwrap_or_default())
    }

    pub async fn save(&self, config: &AppConfig) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let content = toml::to_string_pretty(config)?;
        fs::write(&self.path, content).await?;
        Ok(())
    }

    pub async fn export(&self, dest: &PathBuf) -> anyhow::Result<()> {
        fs::copy(&self.path, dest).await?;
        Ok(())
    }

    pub async fn import(&self, src: &PathBuf) -> anyhow::Result<AppConfig> {
        let content = fs::read_to_string(src).await?;
        let config: AppConfig = toml::from_str(&content)?;
        self.save(&config).await?;
        Ok(config)
    }
}
