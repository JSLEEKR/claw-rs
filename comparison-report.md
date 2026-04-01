# Comparison Report: claw-rs vs claw-code

## Overview

| Aspect | claw-code (Original) | claw-rs (Ours) |
|--------|---------------------|----------------|
| Language | Python 3 | Rust 2021 |
| Stars | ~35K | — |
| Dependencies | 0 (stdlib) | 15 (tokio, reqwest, serde, etc.) |
| Tool Execution | All stubs | All real |
| LLM Integration | None | Full (OpenAI-compatible + SSE) |
| Tests | ~30 | 339 |
| Binary | Python script | Single binary (~5MB) |

## What We Reimplemented

| Module | Original (Stub) | Ours (Real) |
|--------|-----------------|-------------|
| LLM Client | No network calls | HTTP + SSE streaming, retry, message formatting |
| Bash Tool | Returns fake output | `std::process::Command`, timeout, stderr capture |
| Read Tool | Returns stub text | Real file I/O with offset/limit |
| Write Tool | No-op | Creates dirs, writes files |
| Edit Tool | No-op | String replacement with validation |
| Glob Tool | Returns empty list | Real glob pattern matching |
| Grep Tool | Returns empty list | Regex-based file content search |
| Agent Loop | Single turn | Multi-turn tool_use → tool_result cycle |
| Session | JSON stub | UUID-based persistence with metadata |
| Permissions | Static list | Dynamic allow/ask/deny with destructive detection |
| Config | Hardcoded | YAML + env vars + CLI flags |

## Key Differences

### 1. Everything Actually Works
The original returns shim messages like "would handle prompt X". Our version executes real shell commands, reads real files, calls real LLM APIs, and runs the full agent loop.

### 2. Real Agent Loop
The original has no tool_use cycle. Our agent loop: send to LLM → parse tool_use → execute → feed result back → repeat until text response.

### 3. SSE Streaming
Real Server-Sent Events parsing from LLM API responses for streaming output.

### 4. Type-Safe Error Handling
`thiserror`-based error types throughout vs Python's generic exceptions.

### 5. Single Binary Deployment
`cargo build --release` produces one binary. No Python runtime needed.

## Limitations

- More dependencies than original (15 vs 0) — tradeoff for real functionality
- No web browser or MCP support
- Token counting is estimation-based

## Conclusion

claw-rs transforms a structural study tool into a working AI agent runtime. Every stub in the original is replaced with real implementation. The Rust version adds 11x more test coverage (339 vs ~30) while delivering actual tool execution, LLM integration, and multi-turn agent orchestration.
