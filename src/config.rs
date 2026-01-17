use crate::paths;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub initialized: bool,
    pub github_repo: Option<String>,
    pub last_backup: Option<DateTime<Utc>>,
    pub retention_days: u32,
    pub backup_interval_hours: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initialized: false,
            github_repo: None,
            last_backup: None,
            retention_days: 7,
            backup_interval_hours: 6,
        }
    }
}

impl Config {
    fn config_path() -> Result<PathBuf> {
        let dir = paths::config_dir()?;
        Ok(dir.join(CONFIG_FILE))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config: {}", path.display()))?;

        serde_json::from_str(&content).context("Failed to parse config")
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config: {}", path.display()))?;

        Ok(())
    }

    pub fn set_initialized(&mut self, github_repo: Option<String>) -> Result<()> {
        self.initialized = true;
        self.github_repo = github_repo;
        self.save()
    }

    pub fn update_last_backup(&mut self) -> Result<()> {
        self.last_backup = Some(Utc::now());
        self.save()
    }
}
