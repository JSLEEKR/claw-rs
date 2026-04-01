# Three-Way Comparison: Python (Original) vs Rust Branch vs claw-rs

## 1. Project Identity

| | Python (main branch) | Rust (dev/rust branch) | claw-rs (ours) |
|-|---------------------|----------------------|----------------|
| Repo | instructkr/claw-code | instructkr/claw-code (dev/rust) | JSLEEKR/claw-rs |
| Language | Python 3 | Rust 2021 | Rust 2021 |
| LOC | ~3,000 | ~15,500 | ~4,500 |
| Files | ~20 .py + ~70 JSON | 35 .rs | 17 .rs |
| Architecture | Flat modules | 6-crate workspace | 1 crate, 7 modules |
| Dependencies | 0 (stdlib only) | ~15 crates | ~15 crates |
| Tests | ~30 | Integration-heavy | 177 unit tests |
| Stars | 35K | (same repo) | — |
| License | None | None | MIT |
| Status | Stub (nothing executes) | Partially working | Fully working |

## 2. What Each Version Actually Does

### Python: Structural Mirror (ALL STUBS)
- Loads 150+ commands and 100+ tools from JSON snapshots
- Routes prompts via keyword matching — returns stub results
- Tracks sessions — but with fake token counts (word count)
- Emits streaming events — but with fake data
- **Nothing executes.** Every tool returns "would handle X"

### Rust Branch: Ambitious Full Reimplementation (PARTIALLY WORKING)
- Full Anthropic API client with OAuth, retry, SSE streaming
- 18 tool implementations (bash, file ops, web, agent spawning, etc.)
- MCP stdio server manager with JSON-RPC 2.0
- Markdown terminal renderer with syntax highlighting
- Session compaction with summary generation
- Sandbox isolation (Linux only)
- **Most tools work, but 2 monolithic files (149KB + 118KB)**

### claw-rs: Focused & Hardened Runtime (FULLY WORKING)
- Real LLM API calls (Anthropic Messages API + SSE)
- 6 real tool implementations (bash, read, write, edit, glob, grep)
- Multi-turn agent loop (tool_use → execute → tool_result → repeat)
- Permission system with destructive command detection
- Session persistence with path injection protection
- **Everything works. 24 bugs found and fixed through 6 rounds of hostile debugging**

## 3. Module-by-Module Three-Way Comparison

### 3.1 LLM / API Client

| | Python | Rust Branch | claw-rs |
|-|--------|-------------|---------|
| Implementation | None (no network calls) | `crates/api/` (1,400 LOC) | `src/llm/` (1,100 LOC) |
| Auth methods | N/A | ApiKey, Bearer, OAuth, Combined | ApiKey only |
| Retry | N/A | Exponential backoff (200ms-2s), 408/429/5xx detection | None (noted limitation) |
| Streaming | Fake events (generator yields) | SSE parser with chunked handling | SSE parser with CRLF + multi-line |
| OAuth | N/A | Full PKCE flow with browser callback | Not implemented |
| Token counting | `len(text.split())` (word count) | Bytes/4 estimation | `chars().count() / 4` estimation |
| Tests | 0 | Integration with mock HTTP | 32 unit tests |

### 3.2 Tool Implementations

| Tool | Python | Rust Branch | claw-rs |
|------|--------|-------------|---------|
| bash | Stub | Real + sandbox (Linux) | Real + timeout + 10MB limit + destructive detection |
| read file | Stub | Real + path normalization | Real + path safety + 50MB guard |
| write file | Stub | Real | Real + path safety + dir creation |
| edit file | Stub | Real + diff generation | Real + path safety + uniqueness check |
| glob | Stub | Real + mtime sort + limit 100 | Real + path safety |
| grep | Stub | Real + context + type filter | Real + path safety + 50MB guard + regex |
| WebFetch | Stub | Real (HTTP→text) | Not implemented |
| WebSearch | Stub | Real (DuckDuckGo) | Not implemented |
| TodoWrite | Stub | Real (JSON persistence) | Not implemented |
| Agent | Stub | Real (sub-agent spawn) | Not implemented |
| ToolSearch | Stub | Real (keyword scoring) | Not implemented |
| NotebookEdit | Stub | Real (Jupyter cells) | Not implemented |
| REPL | Stub | Real (Python/JS/shell) | Not implemented |
| PowerShell | Stub | Real (Windows) | Not implemented |
| Sleep | N/A | Real | Not implemented |
| SendUserMessage | N/A | Real | Not implemented |
| Config | N/A | Real | Not implemented |
| StructuredOutput | N/A | Real | Not implemented |
| **Total** | **0 working** | **18 working** | **6 working** |
| **Security** | N/A | Minimal | Comprehensive |

### 3.3 Permission System

| | Python | Rust Branch | claw-rs |
|-|--------|-------------|---------|
| Model | Deny-list (name + prefix) | 5 modes (ReadOnly → DangerFullAccess) | 3 tiers (Allow/Ask/Deny) |
| Destructive detection | Auto-deny "bash" tools | Not in permission module | 20+ patterns, whitespace normalization, evasion detection |
| Bypass protection | None | None | eval/pipe/base64/xargs detection |
| Windows patterns | N/A | N/A | rmdir /s, del /f, format, any drive letter |
| CRLF handling | N/A | N/A | Yes |
| Per-tool config | By name/prefix | By permission mode | By PermissionLevel enum |

### 3.4 Session Management

| | Python | Rust Branch | claw-rs |
|-|--------|-------------|---------|
| Storage | JSON files (`.port_sessions/`) | JSON files (custom parser) | JSON files (serde) |
| Serializer | stdlib json | Hand-rolled JSON parser (no serde, no floats) | serde_json |
| Compaction | Sliding window (keep last N) | Full: summary generation, tag extraction, file candidates | Simple: keep last N messages |
| ID format | String | UUID | UUID + sanitization (path chars stripped, 255 limit) |
| Path safety | None | None | Path injection protection |
| Transcript | Separate TranscriptStore with flush/replay | Part of ConversationMessage | Part of Session |

### 3.5 Configuration

| | Python | Rust Branch | claw-rs |
|-|--------|-------------|---------|
| Format | Hardcoded defaults | YAML + multi-scope merging (user/project/local) | YAML + env vars + CLI flags |
| MCP | N/A | 6 transport types (stdio, SSE, HTTP, WS, SDK, proxy) | Not implemented |
| Scope | N/A | User → Project → Local (deep merge) | Single file + overrides |
| CLAUDE.md | N/A | Recursive discovery from ancestors | Not implemented |
| Env expansion | N/A | Not mentioned | `${VAR}` and `$VAR` syntax |
| Validation | N/A | Some | Full (empty key, invalid URL, etc.) |

### 3.6 Agent / Conversation Loop

| | Python | Rust Branch | claw-rs |
|-|--------|-------------|---------|
| Location | `runtime.py` + `query_engine.py` | `conversation.rs` + `main.rs` (3,200 LOC) | `agent/mod.rs` (486 LOC) |
| Design | Prompt routing → stub execution | Generic `ConversationRuntime<C, T>` | Concrete `Agent` struct |
| Tool cycle | Single turn, no tool_use | Full tool_use → tool_result cycle | Full tool_use → tool_result cycle |
| Budget | Word-count token budget | Usage tracking per turn | Turn count + token budget |
| Max turns | 8 (configurable) | Configurable | 30 (configurable) |
| Stop conditions | max_turns, max_budget | Tool errors, stop_reason | max_turns, budget, no tool_use |

### 3.7 CLI / REPL

| | Python | Rust Branch | claw-rs |
|-|--------|-------------|---------|
| Entry | argparse (20+ subcommands) | clap + custom arg parser | clap derive |
| REPL | N/A | rustyline with slash completion | Basic stdin readline |
| Rendering | print() | Full markdown with syntect highlighting | Basic println |
| Spinner | N/A | Braille animation (10 frames) | Not implemented |
| Init | N/A | Project initialization + stack detection | Not implemented |
| Slash commands | N/A | 15 commands | 7 commands |
| Output formats | Text | JSON + Text | Text only |

### 3.8 Additional Features

| Feature | Python | Rust Branch | claw-rs |
|---------|--------|-------------|---------|
| MCP Server Manager | N/A | Full stdio (JSON-RPC 2.0, multi-server) | N/A |
| OAuth 2.0 + PKCE | N/A | Full browser login flow | N/A |
| System Prompt Builder | N/A | CLAUDE.md discovery, git context, dedup | N/A |
| Sandbox | N/A | Linux unshare (namespace isolation) | N/A |
| Markdown Renderer | N/A | syntect, tables, code blocks | N/A |
| Custom JSON Parser | N/A | Hand-rolled (no serde for sessions) | N/A |
| Remote/Proxy | N/A | WebSocket, env inheritance | N/A |
| Parity Audit | TS ↔ Python coverage comparison | TS manifest extraction | N/A |
| Bootstrap Pipeline | Prefetch → Setup → DeferredInit | 12-phase BootstrapPlan | N/A |
| Workspace Discovery | Source/test/asset root detection | Via config + CLAUDE.md | N/A |

## 4. Code Quality Comparison

| Metric | Python | Rust Branch | claw-rs |
|--------|--------|-------------|---------|
| unsafe_code | N/A | `forbid` (workspace) | Not set (but none used) |
| Clippy | N/A | `pedantic` enabled | Standard warnings |
| Largest file | ~10KB (main.py) | **149KB** (tools/lib.rs) | 7KB (agent/mod.rs) |
| Monoliths | None | 2 (149KB + 118KB) | None |
| Error handling | try/except | Custom Display/Error impls | thiserror derive |
| Path safety | None | None | Comprehensive (symlink, null byte, sensitive dirs) |
| File size guards | None | None | 50MB read/grep, 10MB bash |
| Destructive detection | "bash" auto-deny | Not in permission layer | 20+ patterns + evasion |
| Cross-platform | Python (inherently) | Linux-centric | Windows/macOS/Linux |
| Hostile debug rounds | 0 | 0 | 6 rounds, 24 bugs fixed |

## 5. Size & Complexity

```
Python (main):     ████░░░░░░░░░░░░░░░░░░░░░░░░░░  3,000 LOC (all stubs)
claw-rs (ours):    ██████████████░░░░░░░░░░░░░░░░░  4,500 LOC (all working)
Rust branch:       ██████████████████████████████████████████████████  15,500 LOC (mostly working)
```

## 6. Summary Matrix

| Dimension | Python | Rust Branch | claw-rs | Winner |
|-----------|--------|-------------|---------|--------|
| **Functionality** | 0% (all stubs) | 85% (most works) | 40% (6 tools work) | Rust Branch |
| **Code organization** | Good (flat) | Poor (2 monoliths) | Excellent (1 per file) | claw-rs |
| **Security** | None | Minimal | Comprehensive | claw-rs |
| **Platform support** | Python (all) | Linux-centric | Cross-platform Rust | Tie (Python/claw-rs) |
| **Test quality** | ~30 stubs | Integration-heavy | 177 + 6 hostile rounds | claw-rs |
| **API client** | None | Production-grade (OAuth+retry) | Basic (no retry) | Rust Branch |
| **Tool count** | 100+ (all stub) | 18 (all real) | 6 (all real + secured) | Rust Branch |
| **Tool security** | N/A | None | Path/size/injection/evasion | claw-rs |
| **Session safety** | None | None | ID sanitization + path protection | claw-rs |
| **TUI/UX** | print() | Rich markdown + spinner | Basic println | Rust Branch |
| **Maintainability** | Medium | Low (monoliths) | High (modular) | claw-rs |
| **Production readiness** | None | Medium | Medium-High (for 6 tools) | claw-rs |

## 7. Conclusion

**Three fundamentally different approaches to the same goal:**

1. **Python (main)** = "Map the territory" — Catalog everything Claude Code has, execute nothing. Value: reference documentation of the architecture.

2. **Rust (dev/rust)** = "Build everything" — Reimplement the entire Claude Code feature set in Rust. 18 tools, OAuth, MCP, TUI. Value: feature completeness. Risk: 2 monolithic files, no security hardening, Linux-only.

3. **claw-rs (ours)** = "Build less, build right" — Implement 6 core tools with real security, cross-platform support, and exhaustive hostile testing. Value: reliability and safety. Tradeoff: fewer features.

**If you want features** → Rust Branch (18 tools, OAuth, MCP, markdown rendering)
**If you want safety** → claw-rs (24 security bugs found and fixed, path validation, destructive detection)
**If you want to study the architecture** → Python (clean structural mirror)
