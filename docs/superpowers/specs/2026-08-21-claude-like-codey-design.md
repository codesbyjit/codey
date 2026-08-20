# Codey: A Claude-like Coding Agent (OpenRouter Free)

**Date:** 2026-08-21
**Status:** Approved direction (Approach A)

## Goal

Evolve Codey from a working MVP into a coding agent that *feels like Claude Code*
— friendly, proactive, and reliable — while staying on **OpenRouter free** models.
The first priority is the **TUI/UX**, then personality and agent behavior. The
foundation for all of it is **robustness**: free models emit inconsistent output
formats, so the parser and streaming must tolerate them.

This builds on the existing architecture (provider trait, tool registry, agent
loop, sessions, skills, subagents, TUI). It does **not** rewrite those; it
hardens and polishes them.

## Constraints

- Stay on OpenRouter (free) by default. No requirement for paid/Claude keys.
- Rust + Tokio + Ratatui. No database for local persistence (JSON files only).
- No hardcoded secrets. Never block the TUI thread.
- Avoid over-engineering. Traits only where they earn their keep.
- Preserve the working MVP behavior (headless `codey "task"`, `codey setup`, sessions).

## Approach A — Robust text-protocol (chosen)

Keep the model-returns-structured-text design (no native function calling yet),
but make it tolerant and live:

1. **Multi-format decision parser** — `parse_decision` understands every shape
   free models actually produce.
2. **Incremental answer streaming** — answers render token-by-token as Claude does,
   by extracting the growing answer from the partial response while it streams.

Rationale: free models frequently ignore the instructed JSON and emit Claude-style
XML (`<tool_call>`), markdown, or prose. Approach A fixes that with zero API-shape
changes and is the lowest-risk path on free tiers.

---

## Phase 1 — Robustness (foundation, do first)

### 1.1 Multi-format `parse_decision`
Extend `src/provider/mod.rs` `parse_decision` to try, in order:
- Strict JSON (`{"type":"final"|"tool", ...}`)
- **Claude-style XML tool call**: `<tool_call>name<arg_key>k</arg_key><arg_value>v</arg_value>...</tool_call>`,
  including the multi-arg and multi-call variants; map to `FinishDecision::ToolCall`.
- Fenced ```json block
- First `{...}` object substring
- Python-style `tool(arg="...")` marker
- Plain text → `FinishDecision::Final`

Add a dedicated `parse_tool_call_xml` helper and unit tests for each format,
including the exact `<tool_call>list_files<arg_key>path</arg_key><arg_value>src</arg_value></tool_call>`
shape seen in the wild.

### 1.2 Incremental streaming extractor
Add `StreamingAnswerExtractor` (in `provider/mod.rs` or a new `provider/stream.rs`):
- Fed raw deltas as they arrive.
- Best-effort extracts the **current answer text** so far:
  - If the buffer looks like a `Final` JSON, pull the growing `"content":"…"` value
    (handles incremental quote/brace completion; falls back to whole buffer if not JSON).
  - Emits only the *new* substring each update so the UI appends live.
- When the final `FinishDecision` is parsed, the loop emits the clean `AssistantText`
  (already implemented) — the extractor is only for live display during streaming.

Wire it into `OpenRouterProvider::stream` so `StreamDelta::Text` carries the
live answer substring (re-enabling the streaming we previously disabled).

### 1.3 Error resilience
- On `Empty response`, retry once with a nudge prompt ("respond using the JSON
  format: ..."). If still empty, surface a clear, non-crashing message.
- Unknown tool name still returns a model-readable error (existing behavior), so
  the loop can self-correct.

## Phase 2 — TUI overhaul (primary focus)

Improve `src/tui/` so it reads like Claude Code:

- **Layout**: persistent header (title, session #, model, live token usage %);
  scrollable conversation; bottom input box; single status line.
- **Live streaming**: `AssistantText` renders incrementally (enabled by 1.2),
  with a subtle "thinking…" state before the first token.
- **Role styling**: distinct colors/borders for user, assistant, tool, system, error.
- **Tool-call blocks**: show tool name + collapsed args; expandable result.
  For `edit_file`/`write_file`, render a **unified diff** (added/removed lines)
  instead of dumping file contents.
- **Permission prompts**: clear yes/no panel for destructive tools
  (`ConfirmationMode`), keyboard `y`/`n`, non-blocking.
- **Token budget indicator**: show usage % with color states at 80/90/95% and a
  "compacting…" note when context is truncated (context tracking already exists).
- **Help & commands**: keep `/help`, add quick legend in empty state.
- Keep full keyboard nav (history, scroll, cancel via Ctrl-C) working.

## Phase 3 — Personality & behavior ("everything")

- **System prompt (`agent/prompt.rs`)**: Claude-like persona — proactive,
  concise, explains the plan before big changes, asks clarifying questions when
  ambiguous, admits uncertainty, uses tools deliberately, no filler.
- **Agent loop (`agent/loop_.rs`)**: before mutating tools, the model is prompted
  to state intent; on tool error it retries/self-corrects rather than looping;
  uses subagents (`delegate_subagent`) for broad exploration.
- **Response style**: markdown-friendly, short, structured.

## Phase 4 — Documentation (still pending from prior work)

- Write `AGENTS.md` (architecture, how to add tool/provider/command/skill/subagent,
  invariants, testing).
- Write `README.md` (install, `codey setup`, usage, TUI keys).

## Testing

- Unit: each new parser format; `StreamingAnswerExtractor` grows correctly;
  token-budget warnings at thresholds.
- Integration (lib crate): build a `FakeProvider` returning canned responses and
  assert the loop parses tool calls, executes, and returns final answers for
  JSON, XML, and prose formats without network.
- Manual: `codey "list files"` and a multi-turn TUI session; verify streaming,
  diffs, permissions, session persistence.

## Definition of done (this design)

- `cargo check` / `test` / `clippy` clean.
- Free model's JSON, XML, and prose outputs all parsed; no "empty response" crashes.
- Answers stream live in the TUI.
- TUI shows diffs, permission prompts, token usage; feels Claude-like.
- AGENTS.md + README.md written.

## Out of scope (deferred)

- Native function calling (Approach B/C) — revisit if free models improve.
- MCP transport wiring (currently a stub; documented as such).
- Multi-model routing / cost tracking beyond token %.
