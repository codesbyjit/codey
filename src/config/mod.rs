use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_MODEL: &str = "openrouter/free";
pub const DEFAULT_CONTEXT_WINDOW: usize = 128_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmationMode {
    #[default]
    Dangerous,

    Always,

    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_context_window")]
    pub context_window: usize,

    #[serde(default)]
    pub workspace: Option<PathBuf>,
    #[serde(default)]
    pub confirmation_mode: ConfirmationMode,
}

fn default_base_url() -> String {
    "https://openrouter.ai/api/v1/chat/completions".to_string()
}

fn default_context_window() -> usize {
    DEFAULT_CONTEXT_WINDOW
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: "openrouter".into(),
            api_key: String::new(),
            model: DEFAULT_MODEL.into(),
            base_url: default_base_url(),
            context_window: DEFAULT_CONTEXT_WINDOW,
            workspace: None,
            confirmation_mode: ConfirmationMode::Dangerous,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_dotenv();
        let mut config = Self::load_from_disk().unwrap_or_default();
        config.apply_env();
        if config.api_key.is_empty() {
            anyhow::bail!("Codey is not configured. Run `codey setup` first.");
        }
        Ok(config)
    }

    pub fn load_or_default() -> Self {
        Self::load_dotenv();
        let mut config = Self::load_from_disk().unwrap_or_default();
        config.apply_env();
        config
    }

    fn load_from_disk() -> Option<Self> {
        let path = config_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    fn load_dotenv() {
        let friendly = [
            ("API_KEY", "CODEY_API_KEY"),
            ("MODEL_NAME", "CODEY_MODEL"),
            ("API_URL", "CODEY_BASE_URL"),
            ("PROVIDER", "CODEY_PROVIDER"),
            ("CONTEXT_WINDOW", "CODEY_CONTEXT_WINDOW"),
            ("CONFIRMATION_MODE", "CODEY_CONFIRMATION_MODE"),
            ("WORKSPACE", "CODEY_WORKSPACE"),
        ];

        let mut dir = std::env::current_dir().ok();
        while let Some(base) = dir {
            let candidate = base.join(".env");
            if candidate.is_file() {
                if let Ok(content) = std::fs::read_to_string(&candidate) {
                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        if let Some((key, value)) = line.split_once('=') {
                            let key = key.trim();
                            let value = value.trim().trim_matches(['"', '\'']);

                            if key.starts_with("CODEY_") {
                                set_if_unset(key, value);
                            } else if let Some(target) = friendly.iter().find(|(f, _)| *f == key) {
                                set_if_unset(target.1, value);
                            }
                        }
                    }
                }
            }
            dir = base.parent().map(|p| p.to_path_buf());
        }
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("CODEY_PROVIDER") {
            self.provider = v;
        }
        if let Ok(v) = std::env::var("CODEY_API_KEY") {
            self.api_key = v;
        }
        if let Ok(v) = std::env::var("CODEY_MODEL") {
            self.model = v;
        }
        if let Ok(v) = std::env::var("CODEY_BASE_URL") {
            self.base_url = v;
        }
        if let Ok(v) = std::env::var("CODEY_CONTEXT_WINDOW") {
            if let Ok(n) = v.parse::<usize>() {
                self.context_window = n;
            }
        }
        if let Ok(v) = std::env::var("CODEY_WORKSPACE") {
            self.workspace = Some(PathBuf::from(v));
        }
        if let Ok(v) = std::env::var("CODEY_CONFIRMATION_MODE") {
            if let Ok(mode) = serde_json::from_value(serde_json::json!(v)) {
                self.confirmation_mode = mode;
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path().context("could not resolve config path")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating config dir {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("writing config {}", path.display()))?;
        Ok(())
    }

    pub fn workspace_path(&self) -> PathBuf {
        self.workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codey").join("config.toml"))
}

fn set_if_unset(key: &str, value: &str) {
    if std::env::var(key).is_err() {
        std::env::set_var(key, value);
    }
}

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codey"))
}

pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("codey"))
}

pub fn user_skills_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codey").join("skills"))
}

pub fn mcp_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codey").join("mcp.json"))
}

pub fn default_base_url_for(provider: &str) -> &'static str {
    match provider {
        "openrouter" => "https://openrouter.ai/api/v1/chat/completions",
        "openai" => "https://api.openai.com/v1/chat/completions",
        "anthropic" => "https://api.anthropic.com/v1/messages",
        _ => "https://openrouter.ai/api/v1/chat/completions",
    }
}

pub fn config_exists() -> bool {
    config_path().map(|p| p.exists()).unwrap_or(false)
}

pub fn config_display_path() -> String {
    config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/codey/config.toml".into())
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating dir {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url_known_providers() {
        assert!(default_base_url_for("openrouter").contains("openrouter"));
        assert!(default_base_url_for("openai").contains("openai"));
        assert!(default_base_url_for("anthropic").contains("anthropic"));
        assert!(default_base_url_for("unknown").contains("openrouter"));
    }

    #[test]
    fn env_overrides_default_config() {
        std::env::set_var("CODEY_MODEL", "openai/gpt-4o");
        std::env::set_var("CODEY_PROVIDER", "openai");
        let config = Config::load_or_default();
        assert_eq!(config.model, "openai/gpt-4o");
        assert_eq!(config.provider, "openai");
        std::env::remove_var("CODEY_MODEL");
        std::env::remove_var("CODEY_PROVIDER");
    }
}
