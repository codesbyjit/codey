use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_MODEL: &str = "openrouter/free";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const DEFAULT_CONTEXT_WINDOW: usize = 128_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub provider: String,
    pub api_key: String,
    pub model: String,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = config_path()?;

        if !path.exists() {
            return Err("Codey is not configured. Run `codey setup` first.".into());
        }

        let content = fs::read_to_string(path)?;

        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = config_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;

        fs::write(&path, content)?;

        Ok(())
    }

    pub fn api_url(&self) -> &'static str {
        match self.provider.as_str() {
            "openrouter" => OPENROUTER_URL,
            _ => OPENROUTER_URL,
        }
    }
}

fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;

    Ok(PathBuf::from(home)
        .join(".config")
        .join("codey")
        .join("config.toml"))
}

pub fn default_model() -> &'static str {
    DEFAULT_MODEL
}
