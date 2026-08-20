# AGENTS.md — Codey

This file documents Codey's architecture for engineers working on the codebase.
Codey is a Rust terminal coding agent (TUI + headless) built on Tokio and Ratatui.

## Directory structure

```
src/
  main.rs              CLI dispatch, `codey setup`, headless runs; uses the `codey` lib crate.
  lib.rs               Library crate root; re-exports the public API.
  config/mod.rs        Config loading/saving, env + .env overrides, confirmation modes.
  provider/
    types.rs           ChatMessage, ToolDefinition, FinishDecision, StreamDelta, ProviderError.
    mod.rs             Provider trait, create_provider, parse_decision, StreamingAnswerExtractor.
    openrouter.rs      OpenRouter / OpenAI-compatible HTTP + SSE streaming implementation.
  tools/
    mod.rs             Tool trait, ToolContext, resolve_path, schema, builtin_registry.
    registry.rs        ToolRegistry: register/execute; unknown tools return a model-readable error.
    filesystem.rs      list_files, read_file, write_file (with diff), edit_file (with diff).
    search.rs          regex search_files over the workspace.
    shell.rs           async run_command with timeout, kill_on_drop, danger heuristic.
  agent/
    mod.rs             AgentEvent enum, run_agent re-export, MAX_ITERATIONS, DELEGATE_TOOL.
    loop_.rs           The agent loop: stream → decide → tool/subagent → repeat.
    context.rs         Conversation + token estimation + budget/truncation.
    session.rs         Session (JSON persistence) and SessionManager.
    prompt.rs          build_system_prompt + base_persona (the Claude-like system prompt).
    skills.rs          discover_skills, select_for_task (selective loading).
    instructions.rs    AGENTS.md upward discovery.
    subagent.rs        builtin subagents + delegate_subagent tool + run_subagent.
  storage/mod.rs       sessions_dir, save/load/list/delete as JSON.
  mcp/mod.rs           MCP config loading + tool adapter scaffold (transport not wired).
  tui/
    mod.rs             App state, event loop, handle_event, submit_input, cancel_run.
    ui.rs              Rendering: messages, input, status bar, help, permission overlay.
    input.rs           Keyboard handling (Enter, Ctrl-C, scroll, history, permission y/n).
    commands.rs        Slash command handling.
docs/superpowers/specs/  Design specs (e.g. the Claude-like agent design).
```

## Execution flow

```
user ─▶ context ─▶ model ─▶ decision
                       ├─ final  → return answer
                       └─ tool   → execute / delegate → context → model → …
```

- `agent::run_agent` (loop_.rs) is the heart. It never blocks the UI: it runs on a
  background Tokio task and communicates only through an `EventSink`
  (`UnboundedSender<AgentEvent>`).
- The TUI (`tui/mod.rs`) spawns `run_agent`, then drains `AgentEvent`s between
  frames and turns them into conversation entries.
- Because `Session` is `Clone` but not shared-mutable, the finished session is sent
  back via an `mpsc` channel and stored with `SessionManager::replace_current`.

## TUI flow

1. `run()` loads config, builds the provider + registry, and enters `app_loop`.
2. Each frame draws, drains agent events, and reads keyboard input.
3. `submit_input` clones the current `Session`, spawns the agent task, and stores a
   `result_rx` to collect the updated session on completion.
4. Live answer text arrives as `AgentEvent::AssistantText` and is appended to the
   latest assistant entry.

## Agent loop details

- **Streaming**: the provider pushes `StreamDelta::Text` as tokens arrive.
  `StreamingAnswerExtractor` pulls the growing answer out of the model's JSON
  text-protocol so answers render live (like Claude).
- **Decision parsing** (`parse_decision`) is intentionally forgiving and handles:
  strict JSON, fenced ```json, first `{...}` object, Claude-style XML
  `<tool_call>`, inline `content`/loose JSON, and plain prose (treated as final).
- **Permissions**: risky tools request confirmation via a oneshot
  `PermissionRequest`; the TUI answers with `y`/`n`. Headless mode skips prompts.
- **Subagents**: the `delegate_subagent` tool hands work to a specialized
  subagent (`run_subagent`) with depth/token limits.
- **Context**: token budget is tracked; at >90% the context is compacted
  (old tool/assistant messages dropped) and a status note is shown.
- **Resilience**: an empty/unusable model response is retried once before failing.

## Config & secrets

- Config: `~/.config/codey/config.toml` (never commit secrets).
- `.env` (project or any parent dir) is read on load and mapped to `CODEY_*`
  env vars; explicit env vars win over `.env`.
- **Never hardcode API keys.**

## How to extend

- **Add a tool**: implement `tools::Tool` and register it in
  `builtin_registry()` (tools/mod.rs). Unknown tool names already surface a
  model-readable error, so no loop changes are needed.
- **Add a provider**: implement `provider::Provider` and add a branch in
  `create_provider`. Reuse `parse_decision` / `StreamingAnswerExtractor`.
- **Add a slash command**: add an arm in `tui/commands.rs::handle_command`.
- **Add a skill**: drop a `SKILL.md` (with optional YAML frontmatter) under
  `.codey/skills/<name>/` or `~/.config/codey/skills/<name>/`.
- **Add a subagent**: add a `SubagentDef` in `agent/subagent.rs::builtin_subagents`.

## Invariants

- The TUI thread never performs model/tool work directly.
- The agent loop communicates only via `AgentEvent`.
- Sessions persist as JSON and are recoverable on restart.
- No database is used for local persistence.
- `cargo check` / `cargo test` / `cargo clippy` must stay clean.

## Testing

- Unit tests live next to the code they cover (provider parsing, context budget,
  skills, instructions, tools, config, extractor).
- Run `cargo test`, `cargo clippy --all-targets`, and a manual `codey "task"` to
  verify streaming, diffs, permissions, and session persistence.
- MCP transport is currently a scaffold (documented as such); the tool adapter is
  wired but no live server connection exists yet.
