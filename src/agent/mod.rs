pub mod context;
pub mod instructions;
pub mod loop_;
pub mod prompt;
pub mod session;
pub mod skills;
pub mod subagent;

use tokio::sync::{mpsc::UnboundedSender, oneshot};

pub use loop_::run_agent;
pub use session::{Session, SessionManager};

pub const MAX_ITERATIONS: usize = 16;

pub const DELEGATE_TOOL: &str = "delegate_subagent";

#[derive(Debug)]
#[allow(dead_code)]
pub enum AgentEvent {
    Status(String),

    UserMessage(String),

    AssistantText(String),

    ClearAssistant,

    ToolCall {
        name: String,
        summary: String,
    },

    ToolResult {
        name: String,
        output: String,
        is_error: bool,
    },

    Error(String),

    Finished(String),

    Cancelled,

    PermissionRequest {
        description: String,
        responder: oneshot::Sender<bool>,
    },
}

pub type EventSink = UnboundedSender<AgentEvent>;

pub fn summarize_args(value: &serde_json::Value) -> String {
    match serde_json::to_string_pretty(value) {
        Ok(s) => s,
        Err(_) => value.to_string(),
    }
}
