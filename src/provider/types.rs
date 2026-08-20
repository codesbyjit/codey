use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            name: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            name: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            name: None,
            tool_calls: None,
        }
    }

    pub fn tool(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            name: Some(name.into()),
            tool_calls: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,

    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub enum FinishDecision {
    Final(String),

    ToolCall { name: String, arguments: Value },
}

#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub decision: FinishDecision,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum StreamDelta {
    Text(String),

    Status(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider transport error: {0}")]
    Transport(String),
    #[error("provider returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("the model produced an unusable response: {0}")]
    Malformed(String),
    #[error("request cancelled")]
    Cancelled,
    #[error("configuration error: {0}")]
    Config(String),
}

pub fn string_arg(value: &Value, name: &str) -> Result<String, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing string argument `{name}`"))
}

pub fn require_arg<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string argument `{name}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_string_arg() {
        let v = json!({"path": "/tmp/x"});
        assert_eq!(string_arg(&v, "path").unwrap(), "/tmp/x");
        assert!(string_arg(&v, "missing").is_err());
        assert_eq!(require_arg(&v, "path").unwrap(), "/tmp/x");
        assert!(require_arg(&v, "missing").is_err());
    }
}
