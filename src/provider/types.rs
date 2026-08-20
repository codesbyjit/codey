//! Shared types for providers and the agent.
//!
//! These types form the contract between the agent loop, providers, and tools.
//! Keeping them here avoids circular dependencies: `provider` depends on
//! nothing in `agent`, and `agent` depends on `provider::types`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A chat message exchanged with a model provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// For `tool` role messages: the name of the tool that produced `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// For `assistant` messages that requested a tool call (OpenAI-style),
    /// the structured call. We keep our own JSON-contract parsing separate,
    /// but this is populated when a provider uses native tool calling.
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

/// A tool definition handed to the model (and used to build the system prompt).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the arguments, e.g.
    /// `{"type":"object","properties":{...},"required":[...]}`.
    pub parameters: Value,
}

/// The decision the model reached after a completion.
#[derive(Debug, Clone)]
pub enum FinishDecision {
    /// The model produced a final natural-language answer.
    Final(String),
    /// The model requested a tool invocation.
    ToolCall { name: String, arguments: Value },
}

/// Events emitted by a provider while a completion is in flight.
///
/// The agent loop forwards these to the UI so the user sees streaming text,
/// tool calls, and errors without the UI thread ever blocking.
#[derive(Debug, Clone)]
pub enum ProviderEvent {
    /// A chunk of the model's final answer (for streaming display).
    TextDelta(String),
    /// An informational status line, e.g. "contacting model".
    Status(String),
    /// The structured decision the model reached.
    Decision(FinishDecision),
    /// A transport/model error.
    Error(String),
}

/// Errors that can occur while talking to a provider.
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

/// A JSON-value helper used widely when building tool arguments.
pub fn string_arg(value: &Value, name: &str) -> Result<String, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing string argument `{name}`"))
}
