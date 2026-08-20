# Codey

A fast terminal coding agent that helps you understand, edit, and test code. Codey
runs in your terminal as an interactive TUI or a one-shot headless command, and it
talks to an OpenRouter-compatible model.

## Features

- **Interactive TUI** with live streaming answers, tool-call blocks with diffs,
  permission prompts, and a token-usage indicator.
- **Headless mode** — `codey "your task"` streams the answer to stdout and saves
  the session.
- **Tool use** — list/read/write/edit files, search the codebase, and run shell
  commands, with confirmation before risky actions.
- **Sessions** — persistent conversation history you can switch between
  (`/new`, `/sessions`, `/session`, `/prev`, `/next`, `/clear`).
- **Skills & AGENTS.md** — repository instructions and skills are discovered
  automatically and injected into the system prompt.
- **Subagents** — delegate exploration/review/testing to specialized subagents.
- **Provider abstraction** — OpenRouter by default; swap in other
  OpenAI-compatible or Anthropic providers via a single trait.

## Install

```sh
cargo build --release
# the binary is at target/release/codey
```

## Setup

```sh
codey setup
```

This writes `~/.config/codey/config.toml` with your provider, API key, model, and
preferences. A `.env` file in the project (or any parent directory) is also read
and supports `API_KEY`, `MODEL_NAME`, `API_URL`, `PROVIDER`, `CONTEXT_WINDOW`,
`CONFIRMATION_MODE`, and `WORKSPACE` (mapped to the canonical `CODEY_*` env vars).

Environment variables override the saved config:

```sh
export CODEY_API_KEY=sk-or-...
export CODEY_MODEL=openrouter/free
```

> Secrets are never hardcoded. The `.env` file is gitignored.

## Usage

```sh
codey                 # launch the interactive TUI
codey "refactor main.rs"   # run a single task and exit
codey setup          # configure providers and keys
codey --help         # show help
codey --version      # show version
```

### TUI keys

| Key | Action |
| --- | --- |
| `Enter` | submit prompt |
| `Ctrl-C` | cancel a running agent / quit when idle |
| `Ctrl-L` | clear the input line |
| `PageUp` / `PageDn` | scroll the conversation |
| `y` / `n` | answer a permission prompt |
| `/help` | show available commands |

### Slash commands

`/help` `/new` `/sessions` `/session <n>` `/prev` `/next` `/model [name]`
`/context` `/clear` `/skills` `/agents` `/tools` `/mcp` `/quit`

## How it works

1. Your prompt and the system prompt (persona + tool definitions + AGENTS.md +
   skills) are sent to the model.
2. The model returns either a **tool call** (`{"type":"tool",...}`) or a **final
   answer** (`{"type":"final",...}`). Codey is tolerant: it also understands
   Claude-style XML tool calls and plain prose.
3. Tool results are observed, and the loop continues until a final answer.
4. The conversation is saved as a JSON session on disk.

## Configuration

| Field | Default | Meaning |
| --- | --- | --- |
| `provider` | `openrouter` | `openrouter`, `openai`, or `anthropic` |
| `api_key` | — | your provider key |
| `model` | `openrouter/free` | model identifier |
| `base_url` | OpenRouter chat completions | API endpoint |
| `context_window` | `128000` | token budget for context compaction |
| `workspace` | current dir | root the agent operates in |
| `confirmation_mode` | `dangerous` | `always`, `never`, `dangerous` |

## Development

```sh
cargo check
cargo test
cargo clippy --all-targets
cargo build --release
```

See [`AGENTS.md`](./AGENTS.md) for the architecture and how to extend Codey with
new tools, providers, commands, skills, and subagents.
