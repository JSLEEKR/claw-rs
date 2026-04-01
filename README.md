# claw-rs

[![Rust](https://img.shields.io/badge/Rust-2021-orange?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/Tests-339-success?style=for-the-badge)](https://github.com/JSLEEKR/claw-rs)
[![License](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)](LICENSE)

> **Educational Purpose Only. Non-Commercial Use.**
>
> Inspired by [claw-code](https://github.com/instructkr/claw-code). Reimplemented from scratch in Rust.
> This project is for learning about AI agent runtime architecture. Not affiliated with Anthropic.

A **working** AI agent runtime in Rust. Unlike the original Python version (which uses stubs for all tool execution), claw-rs actually executes tools, calls LLM APIs, manages sessions, and runs the full agent loop.

## Why This Exists

claw-code (35K stars) mirrors the architecture of an AI coding assistant but every command and tool is a stub — nothing actually runs. claw-rs takes that architectural blueprint and builds a real, working agent runtime:

| Feature | claw-code (Python) | claw-rs (Rust) |
|---------|-------------------|----------------|
| LLM API calls | Stub (no network) | Real HTTP + SSE streaming |
| Tool execution | Stub (returns fake output) | Real (bash, file I/O, grep, glob) |
| Session persistence | JSON files | JSON with proper error handling |
| Token counting | Word count estimate | Character-based estimation |
| Permission system | Static deny list | Dynamic allow/ask/deny with auto-approve |
| Agent loop | Single turn, no tool use | Multi-turn with tool_use → tool_result cycle |
| Streaming | Fake events | Real SSE parsing from LLM API |
| Error handling | Minimal | thiserror-based typed errors throughout |

## Quick Start

### Build

```bash
cargo build --release
```

### Configure

Set your API key:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
# or
export CLAW_API_KEY=sk-ant-...
```

Or use a config file (`~/.claw-rs/config.yaml`):

```yaml
llm:
  api_key: ${ANTHROPIC_API_KEY}
  model: claude-sonnet-4-20250514
  base_url: https://api.anthropic.com/v1
  max_tokens: 4096
  system_prompt: "You are a helpful coding assistant."
```

### Run

```bash
# Single prompt
claw-rs --prompt "List all Rust files in the current directory"

# Interactive REPL
claw-rs

# With custom config
claw-rs --config ./my-config.yaml

# With specific model
claw-rs --model claude-sonnet-4-20250514
```

## Architecture

```
User Input
    │
    ▼
┌──────────┐
│  Config   │  API key, model, base URL, permissions
└────┬─────┘
     │
     ▼
┌──────────┐
│  Agent   │  Orchestrates the tool-use loop
│  Loop    │
└────┬─────┘
     │
     ├──► LLM Client ──► OpenAI-compatible API (streaming)
     │         │
     │         ▼
     │    Parse Response
     │         │
     │    ┌────┴────┐
     │    │ Text    │ Tool Use
     │    │ Output  │    │
     │    └─────────┘    ▼
     │              Permission Check
     │                   │
     │              ┌────┴────┐
     │              │ Allow   │ Deny
     │              │         │
     │              ▼         ▼
     │         Execute Tool   Skip
     │              │
     │              ▼
     │         Tool Result ──► Add to messages ──► Loop back to LLM
     │
     └──► Session Store (save/load conversation history)
```

## Modules

### `src/llm/` — LLM Client

- OpenAI-compatible chat completion API
- SSE (Server-Sent Events) streaming support
- Message types: system, user, assistant, tool_use, tool_result
- Rate limit detection (429 with retry-after)
- Configurable base URL, model, temperature, max_tokens
- Anthropic Messages API format

### `src/tools/` — Tool Implementations

All tools implement the `Tool` trait:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;  // JSON Schema
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext)
        -> Result<ToolResult, ToolError>;
    fn permission_level(&self) -> PermissionLevel;
}
```

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands with timeout (120s default) |
| `read` | Read files with optional offset/limit (line numbers) |
| `write` | Write content to files (creates directories) |
| `edit` | Find-and-replace in files (old_string → new_string) |
| `glob` | Find files by pattern (e.g., `**/*.rs`) |
| `grep` | Search file contents with regex |
| `web_fetch` | Fetch URLs and extract text (SSRF-protected) |
| `web_search` | Search the web via DuckDuckGo |
| `ask_user` | Interactive user prompt during agent execution |
| `notebook` | Read and display Jupyter notebook (.ipynb) files |

### `src/agent/` — Agent Loop

The core orchestration loop:

```
1. Send messages to LLM
2. Parse response content blocks
3. For each tool_use block:
   a. Check permissions
   b. Execute tool
   c. Add tool_result to messages
4. If any tool was used → go to 1
5. If text only → done
```

Configurable max turns (default: 30) prevents infinite loops.

### `src/permissions/` — Permission System

Three-tier permission model:

- **Allow**: Tool executes without confirmation
- **Ask**: Requires user confirmation (auto-approved in non-interactive mode)
- **Deny**: Tool is blocked entirely

Default deny list includes destructive commands: `rm -rf /`, `DROP TABLE`, `git push --force`, etc.

### `src/session/` — Session Management

- UUID-based session IDs
- JSON file persistence in `~/.claw-rs/sessions/`
- Full message history with timestamps
- Session metadata (model, created/updated times, turn count)

### `src/config/` — Configuration

Priority order: CLI flags > environment variables > config file > defaults.

```yaml
llm:
  api_key: ${ANTHROPIC_API_KEY}     # or set CLAW_API_KEY env var
  model: claude-sonnet-4-20250514        # LLM model
  base_url: https://api.anthropic.com/v1
  max_tokens: 4096
  temperature: 0.0
  system_prompt: "You are a helpful assistant."
agent:
  max_turns: 30                   # agent loop limit
  max_budget_tokens: 200000       # total token budget
permissions:
  auto_approve: false
```

## Tests

339 tests across 14 modules:

| Module | Tests | Coverage |
|--------|-------|----------|
| tools/bash | 14 | Command execution, stderr, timeout, destructive detection, UTF-8 truncation |
| tools/read | 14 | File reading, offset/limit, missing files, line numbers, path safety, size guard |
| tools/write | 6 | File creation, overwrite, directory creation |
| tools/edit | 12 | Replace, not found, empty, multiple matches, multiline |
| tools/glob | 8 | Patterns, empty results, nested dirs |
| tools/grep | 12 | Regex, case insensitive, line numbers, walk dir, size guard |
| tools/web_fetch | 20 | HTML stripping, SSRF protection, entities, UTF-8 truncation |
| tools/web_search | 14 | DDG parsing, percent decode, UTF-8, body size limits |
| tools/ask_user | 8 | Callbacks, fixed response, missing params |
| tools/notebook | 14 | Cell formatting, ANSI stripping, file size guard |
| tools/mod | 14 | Registry, definitions, permissions, path validation |
| llm/types | 16 | Messages, content blocks, serde, usage, tokens, multibyte |
| llm/client | 5 | Request building, serialization |
| llm/streaming | 13 | SSE parsing, partial chunks, tool use blocks, CRLF |
| config | 8 | YAML loading, defaults, validation |
| permissions | 22 | Allow/deny/ask, destructive detection, auto-approve, evasion |
| session | 15 | Save/load, metadata, turn counting, compaction, path traversal |
| agent | 8 | Agent creation, compact, clear, callbacks |
| bootstrap | 10 | Platform detection, project scanning, prompt generation |
| commands | 29 | Slash commands, aliases, case-insensitive lookup, all commands |
| streaming | 12 | Event types, renderer, colors, serde |
| transcript | 13 | Append, compact, replay, flush, dirty tracking |
| usage | 19 | Token tracking, cost calculation, pricing models |
| context | 23 | Project detection, git info, file counting, workspace discovery |
| main | 10 | CLI parsing, slash commands |

## Comparison with Original

| Aspect | claw-code (Python) | claw-rs (Rust) |
|--------|-------------------|----------------|
| Language | Python 3 | Rust 2021 |
| Stars | ~35K | — |
| Lines of Code | ~3,000 | ~4,500 |
| Dependencies | 0 (stdlib only) | 15 (tokio, reqwest, serde, etc.) |
| Tool execution | All stubs | All real |
| LLM integration | None | Full (OpenAI-compatible + streaming) |
| Tests | ~30 | 339 |
| Binary | N/A (Python script) | Single binary (~5MB) |
| Performance | Python interpreter | Native compiled |

## Limitations

- No web browser tool (would need headless browser integration)
- No MCP server support
- SSE streaming display is basic (no rich terminal formatting)
- Token counting is estimation-based (not exact tiktoken)

## License

MIT License. See [LICENSE](LICENSE).

---

*For educational purposes only. Not affiliated with Anthropic or Claude Code.*
*Inspired by [claw-code](https://github.com/instructkr/claw-code). Reimplemented from scratch in Rust.*
