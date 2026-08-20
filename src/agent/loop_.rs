use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use super::subagent::{self, parse_delegate_args, run_subagent, MAX_SUBAGENTS, MAX_SUBAGENT_DEPTH};
use super::{summarize_args, AgentEvent, EventSink, Session};
use crate::config::ConfirmationMode;
use crate::provider::{CompletionRequest, FinishDecision, Provider, StreamDelta};
use crate::tools::{ToolContext, ToolRegistry};

const MAX_MODEL_RETRIES: usize = 2;

#[allow(clippy::too_many_arguments)]
pub async fn run_agent(
    session: &mut Session,
    provider: &Arc<dyn Provider>,
    registry: &ToolRegistry,
    system_prompt: &str,
    events: &EventSink,
    confirmation_mode: ConfirmationMode,
    headless: bool,
    depth: usize,
    active_subagents: Arc<AtomicUsize>,
    allowed_tools: Option<Vec<String>>,
) -> Result<String, String> {
    let _ = confirmation_mode;
    let workspace = session.workspace();
    let tool_ctx = ToolContext::new(workspace.clone());

    let mut tool_defs = registry.definitions();
    if depth == 0 {
        tool_defs.push(subagent::delegate_definition());
    }

    let request_tools = tool_defs;

    let mut iteration = 0usize;
    let mut attempt = 0usize;
    loop {
        iteration += 1;
        attempt += 1;
        if iteration > super::MAX_ITERATIONS {
            let msg = format!(
                "Codey stopped after reaching the maximum of {} agent steps.",
                super::MAX_ITERATIONS
            );
            let _ = events.send(AgentEvent::Error(msg.clone()));
            return Err(msg);
        }

        let _ = events.send(AgentEvent::Status("thinking…".into()));

        let mut messages = vec![crate::provider::ChatMessage::system(system_prompt)];
        messages.extend(session.context().messages().iter().cloned());

        if session.context().usage_percent() > 90.0 {
            let removed = session.context_mut().truncate_to_budget(80.0);
            if removed > 0 {
                let _ = events.send(AgentEvent::Status(format!(
                    "compacted context (removed {removed} old messages)"
                )));
            }
        }

        let req = CompletionRequest {
            model: session.model().to_string(),
            messages,
            tools: request_tools.clone(),
            temperature: 0.2,
            max_tokens: None,
        };

        let (delta_tx, mut delta_rx) = mpsc::unbounded_channel::<StreamDelta>();

        let mut streamed_answer = String::new();

        let stream_fut = provider.stream(req.clone(), delta_tx.clone());
        drop(delta_tx);

        let drain_fut = async {
            while let Some(delta) = delta_rx.recv().await {
                match delta {
                    StreamDelta::Text(t) => {
                        streamed_answer.push_str(&t);
                        let _ = events.send(AgentEvent::AssistantText(t));
                    }
                    StreamDelta::Status(s) => {
                        let _ = events.send(AgentEvent::Status(s));
                    }
                }
            }
        };

        let result = tokio::join!(stream_fut, drain_fut).0;

        let decision = match result {
            Ok(completion) => completion.decision,
            Err(e) => {
                let msg = format!("Model error: {e}");

                // Retry once with a nudge before giving up.
                if attempt < MAX_MODEL_RETRIES
                    && (msg.contains("empty response") || msg.contains("unusable"))
                {
                    let _ = events.send(AgentEvent::Status("retrying…".into()));
                    continue;
                }
                let _ = events.send(AgentEvent::Error(msg.clone()));
                return Err(msg);
            }
        };

        match decision {
            FinishDecision::Final(text) => {
                let text = crate::provider::clean_answer(&text);

                if text.trim().is_empty() {
                    if attempt < MAX_MODEL_RETRIES {
                        let _ = events.send(AgentEvent::Status(
                            "model gave no usable answer, retrying…".into(),
                        ));
                        continue;
                    }
                    let msg = "model returned an empty response".to_string();
                    let _ = events.send(AgentEvent::Error(msg.clone()));
                    return Err(msg);
                }
                session.add_assistant_message(text.clone());

                if streamed_answer.trim().is_empty() {
                    let _ = events.send(AgentEvent::AssistantText(text.clone()));
                }
                let _ = events.send(AgentEvent::Finished(text.clone()));
                return Ok(text);
            }

            FinishDecision::ToolCall { name, arguments } => {
                if !streamed_answer.trim().is_empty() {
                    let _ = events.send(AgentEvent::ClearAssistant);
                }

                let _ = events.send(AgentEvent::ToolCall {
                    name: name.clone(),
                    summary: summarize_args(&arguments),
                });

                if let Some(allowed) = &allowed_tools {
                    if !allowed.contains(&name) {
                        let msg = format!(
                            "Tool `{name}` is not permitted for this subagent (allowed: {}).",
                            allowed.join(", ")
                        );
                        let _ = events.send(AgentEvent::ToolResult {
                            name: name.clone(),
                            output: msg.clone(),
                            is_error: true,
                        });
                        session.add_tool_result(name.clone(), msg);
                        continue;
                    }
                }

                let need_confirm = registry
                    .get(&name)
                    .map(|t| t.requires_confirmation(&arguments))
                    .unwrap_or(false);
                if need_confirm && !headless {
                    let description = format!("Codey wants to run `{name}`.");
                    if !request_permission(events, &description).await {
                        let msg = format!("Permission denied for `{name}`.");
                        let _ = events.send(AgentEvent::ToolResult {
                            name: name.clone(),
                            output: msg.clone(),
                            is_error: true,
                        });
                        session.add_tool_result(name.clone(), msg);
                        continue;
                    }
                }

                let output = if name == super::DELEGATE_TOOL {
                    execute_delegate(
                        &name,
                        &arguments,
                        provider,
                        events,
                        confirmation_mode,
                        headless,
                        &workspace,
                        depth,
                        active_subagents.clone(),
                    )
                    .await
                } else {
                    registry.execute(&name, &arguments, &tool_ctx).await
                };

                let (out, is_error) = match output {
                    Ok(o) => (o, false),
                    Err(e) => (e, true),
                };

                let _ = events.send(AgentEvent::ToolResult {
                    name: name.clone(),
                    output: out.clone(),
                    is_error,
                });
                session.add_tool_result(name.clone(), out);
            }
        }
    }
}

async fn request_permission(events: &EventSink, description: &str) -> bool {
    let (tx, rx) = oneshot::channel::<bool>();
    if events
        .send(AgentEvent::PermissionRequest {
            description: description.to_string(),
            responder: tx,
        })
        .is_err()
    {
        return false;
    }
    rx.await.unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
async fn execute_delegate(
    _name: &str,
    arguments: &serde_json::Value,
    provider: &Arc<dyn Provider>,
    events: &EventSink,
    confirmation_mode: ConfirmationMode,
    headless: bool,
    workspace: &std::path::Path,
    depth: usize,
    active_subagents: Arc<AtomicUsize>,
) -> Result<String, String> {
    let (agent_name, task) = parse_delegate_args(arguments)
        .ok_or_else(|| "delegate_subagent requires `agent` and `task` arguments.".to_string())?;

    if depth >= MAX_SUBAGENT_DEPTH {
        return Err(format!(
            "Maximum subagent depth ({MAX_SUBAGENT_DEPTH}) reached; cannot delegate further."
        ));
    }
    if active_subagents.load(Ordering::SeqCst) >= MAX_SUBAGENTS {
        return Err(format!(
            "Maximum concurrent subagents ({MAX_SUBAGENTS}) reached."
        ));
    }

    let defs = subagent::builtin_subagents();
    let def = defs
        .iter()
        .find(|d| d.name == agent_name)
        .ok_or_else(|| format!("Unknown subagent `{agent_name}`."))?;

    let _ = events.send(AgentEvent::Status(format!("delegating to `{agent_name}`…")));
    run_subagent(
        provider,
        def,
        &task,
        depth,
        events,
        confirmation_mode,
        headless,
        workspace,
        active_subagents,
    )
    .await
}
