use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::agent::{prompt, run_agent, EventSink, Session, DELEGATE_TOOL};
use crate::config::ConfirmationMode;
use crate::provider::Provider;

pub const MAX_SUBAGENT_DEPTH: usize = 4;
pub const MAX_SUBAGENTS: usize = 8;

#[derive(Debug, Clone)]
pub struct SubagentDef {
    pub name: &'static str,
    pub purpose: &'static str,
    pub persona: &'static str,
    pub allowed_tools: &'static [&'static str],
}

pub fn builtin_subagents() -> Vec<SubagentDef> {
    vec![
        SubagentDef {
            name: "explorer",
            purpose: "Explore and understand the codebase using read-only tools.",
            persona: "You are the Explorer subagent. Investigate the codebase and report findings. Use read_file, list_files, and search_files only. Do not modify anything. Be thorough and cite file paths and line numbers.",
            allowed_tools: &["read_file", "list_files", "search_files"],
        },
        SubagentDef {
            name: "coder",
            purpose: "Implement changes using read and write tools.",
            persona: "You are the Coder subagent. Implement the requested change precisely. Prefer targeted edits. Use tools to read before writing. Return a concise summary of what you changed.",
            allowed_tools: &["read_file", "list_files", "search_files", "write_file", "edit_file"],
        },
        SubagentDef {
            name: "reviewer",
            purpose: "Review code and changes for correctness and style.",
            persona: "You are the Reviewer subagent. Read the relevant code and review it for bugs, security issues, and style problems. Report a prioritized list of findings with file:line references. Do not modify files.",
            allowed_tools: &["read_file", "list_files", "search_files"],
        },
        SubagentDef {
            name: "tester",
            purpose: "Run tests and report results.",
            persona: "You are the Tester subagent. Run the relevant test commands and report pass/fail results and any errors. Use read-only and shell tools. Summarize clearly.",
            allowed_tools: &["read_file", "list_files", "search_files", "run_command"],
        },
    ]
}

pub fn delegate_definition() -> crate::provider::ToolDefinition {
    crate::provider::ToolDefinition {
        name: DELEGATE_TOOL.to_string(),
        description: "Delegate a specialized task to a subagent (explorer, coder, reviewer, tester). Returns the subagent's findings.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "agent": {"type": "string", "description": "Subagent name: explorer, coder, reviewer, or tester"},
                "task": {"type": "string", "description": "The task to delegate"}
            },
            "required": ["agent", "task"]
        }),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_subagent(
    provider: &Arc<dyn Provider>,
    def: &SubagentDef,
    task: &str,
    depth: usize,
    events: &EventSink,
    confirmation_mode: ConfirmationMode,
    headless: bool,
    _workspace: &Path,
    active: Arc<AtomicUsize>,
) -> Result<String, String> {
    let _ = active.fetch_add(1, Ordering::SeqCst);
    let counter = active.clone();
    let _guard = CallCounter(counter);

    let allowed: Vec<String> = def.allowed_tools.iter().map(|s| s.to_string()).collect();

    let mut session = Session::new();
    session.add_user_message(task);

    let tool_defs = crate::tools::builtin_registry()
        .definitions()
        .into_iter()
        .filter(|d| allowed.contains(&d.name))
        .collect::<Vec<_>>();

    let system_prompt = format!(
        "{}\n\nYOUR ROLE:\n{}\n\nOnly use these tools: {}.",
        prompt::base_persona(),
        def.persona,
        allowed.join(", ")
    );
    let _ = tool_defs;

    let builtins = crate::tools::builtin_registry();

    Box::pin(run_agent(
        &mut session,
        provider,
        &builtins,
        &system_prompt,
        events,
        confirmation_mode,
        headless,
        depth + 1,
        active,
        Some(allowed),
    ))
    .await
    .map_err(|e| format!("subagent `{}` failed: {e}", def.name))
}

struct CallCounter(Arc<AtomicUsize>);
impl Drop for CallCounter {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn parse_delegate_args(args: &Value) -> Option<(String, String)> {
    let agent = args.get("agent").and_then(|v| v.as_str())?.to_string();
    let task = args.get("task").and_then(|v| v.as_str())?.to_string();
    Some((agent, task))
}
