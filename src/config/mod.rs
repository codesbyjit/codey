//! Codey configuration.
//!
//! Stored as TOML at `<config_dir>/codey/config.toml` (e.g.
//! `~/.config/codey/config.toml`). Secrets are never hardcoded; they are read
//! from this file or from environment variables, which take precedence.

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_MODEL: &str = "openrouter/free";
pub const DEFAULT_CONTEXT_WINDOW: usize = 128_000;

/// How aggressively Codey asks for confirmation before risky operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmationMode {
    /// Ask only before destructive / dangerous operations.
    #[default]
    Dangerous,
    /// Ask before every tool that mutates state.
    Always,
    /// Never ask (use with care).
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
    /// Workspace the agent operates in. `None` means the current directory
    /// at launch time.
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
    /// Load configuration from disk, then apply environment overrides.
    pub fn load() -> Result<Self> {
        let mut config = Self::load_from_disk().unwrap_or_default();
        config.apply_env();
        if config.api_key.is_empty() {
            anyhow::bail!("Codey is not configured. Run `codey setup` first.");
        }
        Ok(config)
    }

    /// Load without requiring an API key (used by `codey setup` itself).
    pub fn load_or_default() -> Self {
        let mut config = Self::load_from_disk().unwrap_or_default();
        config.apply_env();
        config
    }

    fn load_from_disk() -> Option<Self> {
        let path = config_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
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

    /// The workspace directory, falling back to the current directory.
    pub fn workspace_path(&self) -> PathBuf {
        self.workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// Resolve the config file path: `<config_dir>/codey/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codey").join("config.toml"))
}

/// Resolve the Codey data directory (sessions, skills cache, etc.).
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("codey"))
}

/// Resolve the user-level skills directory: `<config_dir>/codey/skills`.
pub fn user_skills_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codey").join("skills"))
}

/// Resolve the user-level MCP config path.
pub fn mcp_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codey").join("mcp.json"))
}

/// The base URL used when the user did not specify one.
pub fn default_base_url_for(provider: &str) -> &'static str {
    match provider {
        "openrouter" => "https://openrouter.ai/api/v1/chat/completions",
        "openai" => "https://api.openai.com/v1/chat/completions",
        "anthropic" => "https://api.anthropic.com/v1/messages",
        _ => "https://openrouter.ai/api/v1/chat/completions",
    }
}

/// Convenience: does this path exist on disk as a config file?
pub fn config_exists() -> bool {
    config_path().map(|p| p.exists()).unwrap_or(false)
}

/// Used by `codey setup` to render the path the user should know about.
pub fn config_display_path() -> String {
    config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/codey/config.toml".into())
}

/// Ensure a parent directory exists for any data file.
pub fn ensure_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating dir {}", parent.display()))?;
    }
    Ok(())
}
