use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{config_dir, mcp_config_path};
use crate::tools::{Tool, ToolContext};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum McpTransport {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(flatten)]
    pub transport: McpTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

pub fn load_config() -> Option<McpConfig> {
    for candidate in project_config().into_iter().chain(user_config()) {
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            if let Ok(config) = serde_json::from_str::<McpConfig>(&text) {
                return Some(config);
            }
        }
    }
    None
}

fn project_config() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    Some(cwd.join(".codey").join("mcp.json"))
}

fn user_config() -> Option<PathBuf> {
    mcp_config_path().or_else(config_dir)
}

#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("mcp client not connected: {0}")]
    NotConnected(String),
    #[error("mcp tool call failed: {0}")]
    ToolCall(String),
    #[error("mcp protocol error: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait McpClient: Send + Sync {
    fn name(&self) -> &str;

    async fn list_tools(&self) -> Result<Vec<McpTool>, McpError>;

    async fn call_tool(&self, name: &str, arguments: &Value) -> Result<String, McpError>;
}

pub struct McpToolAdapter {
    client: std::sync::Arc<dyn McpClient>,
    tool: McpTool,
}

impl McpToolAdapter {
    pub fn new(client: std::sync::Arc<dyn McpClient>, tool: McpTool) -> Self {
        Self { client, tool }
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.tool.name
    }

    fn description(&self) -> &str {
        &self.tool.description
    }

    fn parameters(&self) -> Value {
        self.tool.input_schema.clone()
    }

    async fn execute(&self, args: &Value, _ctx: &ToolContext) -> Result<String, String> {
        self.client
            .call_tool(&self.tool.name, args)
            .await
            .map_err(|e| e.to_string())
    }
}

pub struct StubMcpClient {
    name: String,
}

#[async_trait]
impl McpClient for StubMcpClient {
    fn name(&self) -> &str {
        &self.name
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>, McpError> {
        Err(McpError::NotConnected(
            "MCP stdio transport is not yet implemented in this MVP".into(),
        ))
    }

    async fn call_tool(&self, _name: &str, _arguments: &Value) -> Result<String, McpError> {
        Err(McpError::NotConnected(
            "MCP stdio transport is not yet implemented in this MVP".into(),
        ))
    }
}

pub fn register_mcp_tools(registry: &mut crate::tools::ToolRegistry) -> usize {
    let Some(config) = load_config() else {
        return 0;
    };
    let mut count = 0;
    for server in config.servers {
        let name = server.name.clone();
        let client: std::sync::Arc<dyn McpClient> = std::sync::Arc::new(StubMcpClient { name });

        let adapter = McpToolAdapter::new(
            client,
            McpTool {
                name: format!("mcp_{}", server.name),
                description: format!(
                    "MCP tool from server `{}` (transport not wired).",
                    server.name
                ),
                input_schema: serde_json::json!({"type": "object"}),
            },
        );
        registry.register(Box::new(adapter));
        count += 1;
    }
    count
}
