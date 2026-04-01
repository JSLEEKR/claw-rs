# Line-by-Line Comparison: claw-rs vs claw-code (Rust branch)

## Scale Comparison

| Metric | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Total Rust LOC | ~15,500 | ~4,500 |
| Source files | 35 | 17 |
| Crates/Modules | 6 crates (workspace) | 1 crate (7 modules) |
| Dependencies | ~15 crates | ~15 crates |
| Tests | Integration-heavy | 177 unit tests |
| Largest file | 149KB (tools/lib.rs) | ~7KB (agent/mod.rs) |
| Binary name | "claw" | "claw-rs" |

claw-code/rust is **~3.4x larger** in raw LOC.

## Architecture Comparison

### claw-code/rust: 6-Crate Workspace
```
crates/
  api/           (5 files, ~1,400 LOC) — Anthropic client + OAuth
  commands/      (1 file, ~450 LOC) — Slash command registry
  compat-harness/ (1 file, ~350 LOC) — TS parity tracking
  runtime/       (14 files, ~7,000 LOC) — Config, session, MCP, sandbox, etc.
  rusty-claude-cli/ (6 files, ~5,000 LOC) — CLI binary
  tools/         (1 file, ~4,000 LOC) — 18 tool implementations
```

### claw-rs: Single Crate, 7 Modules
```
src/
  agent/     (1 file, ~486 LOC) — Agent loop
  config/    (1 file, ~303 LOC) — Config
  llm/       (4 files, ~1,100 LOC) — LLM client + SSE + types
  permissions/ (1 file, ~489 LOC) — Permissions
  session/   (1 file, ~384 LOC) — Session persistence
  tools/     (7 files, ~2,100 LOC) — 6 tool implementations
  main.rs    (~362 LOC) — CLI + REPL
```

## Module-by-Module Comparison

### 1. LLM API Client

| Aspect | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Location | `crates/api/` (5 files, ~1,400 LOC) | `src/llm/` (4 files, ~1,100 LOC) |
| Auth | ApiKey + Bearer + OAuth + Combined | ApiKey only |
| Retry | Exponential backoff (200ms-2s), retryable error detection (408/409/429/5xx) | No retry (noted as limitation) |
| Streaming | SSE parser with chunked handling | SSE parser with CRLF + multi-line support |
| Token types | Full StreamEvent lifecycle events | ContentBlock-based (text, tool_use) |
| OAuth | Full PKCE flow with browser callback | Not implemented |
| Tests | Integration tests with mock HTTP server | Unit tests for request building, SSE parsing |

**Verdict**: claw-code/rust has production-grade API client with OAuth and retry. claw-rs is simpler but functional for basic usage.

### 2. Tool Implementations

| Aspect | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Location | `crates/tools/lib.rs` (149KB, ~4,000 LOC, 1 MONOLITHIC file) | `src/tools/` (7 files, ~2,100 LOC, 1 file per tool) |
| Tool count | 18 tools | 6 tools |
| Bash | Sandbox integration (Linux unshare), background exec | Timeout, output truncation (10MB), destructive detection |
| File ops | Read/write/edit with path normalization | Read/write/edit with path safety validation, size guards (50MB) |
| Glob | Sorted by mtime, limit 100 | Pattern matching with path safety |
| Grep | Context lines, glob/type filtering, walkdir | Regex with case insensitive, line numbers, file size guard |
| WebFetch | HTTP with HTML-to-text conversion | Not implemented |
| WebSearch | DuckDuckGo integration with dedup | Not implemented |
| TodoWrite | JSON task persistence | Not implemented |
| Agent | Sub-agent spawning by type | Not implemented |
| ToolSearch | Keyword scoring + relevance | Not implemented |
| NotebookEdit | Jupyter cell manipulation | Not implemented |
| REPL/PowerShell | Code execution | Not implemented |
| Architecture | ALL in 1 file (149KB!) | 1 file per tool (200-400 LOC each) |

**Verdict**: claw-code/rust has 3x more tools but in 1 monolithic 149KB file. claw-rs has fewer tools but better file organization, security hardening (path validation, size guards, destructive detection), and cleaner code structure.

### 3. Permission System

| Aspect | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Location | `crates/runtime/permissions.rs` (~210 LOC) | `src/permissions/mod.rs` (~489 LOC) |
| Modes | 5 levels (ReadOnly → DangerFullAccess + Allow) | 3 tiers (Allow/Ask/Deny) |
| Destructive detection | Not in permission module (in bash tool) | 20+ patterns with whitespace normalization, evasion detection |
| Per-tool policy | PermissionPolicy with escalation | PermissionLevel per tool + destructive_patterns list |
| Auto-approve | Via permission mode | Configurable auto_approve flag |

**Verdict**: claw-code/rust has more granular permission modes. claw-rs has more robust destructive command detection (whitespace bypass, eval/pipe evasion, Windows patterns).

### 4. Session Management

| Aspect | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Location | `crates/runtime/session.rs` (~400 LOC) + `compact.rs` (~420 LOC) | `src/session/mod.rs` (~384 LOC) |
| Serialization | Custom JSON parser (json.rs, ~320 LOC, no serde) | serde_json |
| Compaction | Full implementation: preserve recent, generate summary, extract tags/files | Simple: keep last N messages, clear old |
| ID safety | UUID-based | UUID + sanitization (strip path chars, 255 limit) |
| Format | Custom JSON with ContentBlock types | Standard serde JSON |

**Verdict**: claw-code/rust has sophisticated compaction (summary generation, tag extraction). claw-rs has simpler but safer session handling (path injection protection).

### 5. Configuration

| Aspect | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Location | `crates/runtime/config.rs` (~900 LOC) | `src/config/mod.rs` (~303 LOC) |
| Scope | User + project + local config merging | Single config file + env vars + CLI |
| MCP | 6 transport types (stdio, SSE, HTTP, WS, SDK, proxy) | Not implemented |
| Features | Feature flags, deep object merging, CLAUDE.md discovery | Basic YAML + env expansion |

**Verdict**: claw-code/rust has enterprise-grade config management. claw-rs is minimal but sufficient.

### 6. Agent/Conversation Loop

| Aspect | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Location | `crates/runtime/conversation.rs` (~500 LOC) + `rusty-claude-cli/main.rs` (~3,200 LOC) | `src/agent/mod.rs` (~486 LOC) |
| Generic | `ConversationRuntime<C, T>` generic over ApiClient + ToolExecutor | Concrete `Agent` struct |
| Turn tracking | Yes, with usage per turn | Yes, with turn count + token budget |
| Tool dispatch | Via trait-based ToolExecutor | Via ToolRegistry HashMap lookup |
| Compaction | Automatic mid-conversation compaction | Manual compact_messages() |

**Verdict**: claw-code/rust has more sophisticated generic design. claw-rs is simpler and more direct.

### 7. CLI & REPL

| Aspect | claw-code/rust | claw-rs |
|--------|---------------|---------|
| Location | `rusty-claude-cli/` (6 files, ~5,000 LOC) | `src/main.rs` (~362 LOC) |
| Rendering | Full markdown renderer with syntax highlighting (syntect) | Basic println output |
| Line editor | rustyline with slash command completion | Basic stdin readline |
| Init | Project initialization with stack detection | Not implemented |
| Spinner | Braille animation (10 frames) | Not implemented |
| Slash commands | 15 commands (help, status, compact, model, etc.) | 7 commands (help, clear, history, session, model, compact, exit) |

**Verdict**: claw-code/rust has a polished TUI experience. claw-rs is functional but minimal.

### 8. Features ONLY in claw-code/rust (not in claw-rs)

| Feature | LOC | Notes |
|---------|-----|-------|
| OAuth 2.0 + PKCE | ~560 | Full browser login flow |
| MCP stdio server manager | ~1,700 | JSON-RPC 2.0, multi-server |
| System prompt builder | ~750 | CLAUDE.md discovery, git context |
| Custom JSON parser | ~320 | No serde dependency for sessions |
| Sandbox (Linux) | ~350 | unshare-based isolation |
| Markdown renderer | ~700 | syntect highlighting, tables |
| Session compaction | ~420 | Summary, tag extraction |
| Remote/proxy | ~370 | WebSocket, env inheritance |
| compat-harness | ~350 | TS parity tracking |
| 12 additional tools | ~2,000+ | WebFetch, WebSearch, Agent, REPL, etc. |

### 9. Features ONLY in claw-rs (not in claw-code/rust)

| Feature | Notes |
|---------|-------|
| Path traversal protection | validate_path_safety with symlink canonicalization |
| File size guards | 50MB read/grep, 10MB bash output |
| Destructive command evasion detection | Whitespace, eval, pipe, base64, xargs |
| Windows-aware security | Any drive letter, UNC paths, Windows destructive commands |
| Session ID sanitization | Strip path chars, 255 char limit |
| SSE CRLF handling | Windows proxy compatibility |
| UTF-8 safe output truncation | char_boundary detection |

## Code Quality Comparison

| Metric | claw-code/rust | claw-rs |
|--------|---------------|---------|
| `unsafe_code = "forbid"` | Yes (workspace) | No (but no unsafe used) |
| Clippy pedantic | Yes | No (standard warnings only) |
| Largest file | 149KB (tools) | ~7KB (agent) |
| File organization | 2 monoliths (main.rs 118KB, tools 149KB) | Well-decomposed (1 file per concern) |
| Error handling | Custom Display/Error impls | thiserror derive macros |
| Security hardening | Minimal (sandbox-focused) | Extensive (path, size, destructive, injection) |
| Platform support | Linux-focused (sandbox, OAuth, container) | Cross-platform (Windows paths, CRLF, drives) |

## Summary

**claw-code/rust** is a **feature-rich, large-scale** reimplementation (~15,500 LOC) that covers the full Claude Code surface area: 18 tools, OAuth, MCP, markdown rendering, session compaction, and system prompt building. It suffers from two massive monolithic files and Linux-only assumptions.

**claw-rs** is a **focused, security-hardened** runtime (~4,500 LOC) that does less but does it better: 6 tools that actually execute safely, comprehensive path/injection/OOM protection, cross-platform support, and clean 1-file-per-module architecture. It's 3.4x smaller but passed 6 rounds of hostile debugging with 24 bugs found and fixed.

### TL;DR

| | claw-code/rust | claw-rs |
|-|---------------|---------|
| **Breadth** | Wide (18 tools, OAuth, MCP, TUI) | Narrow (6 tools, basic REPL) |
| **Depth** | Shallow on security | Deep on security |
| **Code org** | 2 monoliths (149KB + 118KB) | Well-decomposed (max 7KB/file) |
| **Platform** | Linux-centric | Cross-platform |
| **Maturity** | Early but ambitious | Focused and hardened |
