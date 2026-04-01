//! Bash tool — execute shell commands via process::Command

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use std::time::Duration;

/// Tool for executing shell commands
pub struct BashTool {
    /// Default timeout in seconds
    default_timeout: u64,
}

impl BashTool {
    /// Create a new BashTool with default settings
    pub fn new() -> Self {
        Self {
            default_timeout: 120,
        }
    }

    /// Create a BashTool with custom timeout
    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self {
            default_timeout: timeout_secs,
        }
    }

    /// Maximum output size in bytes (10 MB)
    const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

    /// Check if a command is potentially destructive
    ///
    /// Normalizes whitespace to collapse spaces, preventing bypass via extra spaces.
    /// Also checks for shell metacharacter wrappers like eval, base64 piping, etc.
    pub fn is_destructive(command: &str) -> bool {
        let patterns = [
            "rm -rf",
            "rm -r /",
            "rmdir /s",
            "del /f",
            "format ",
            "mkfs.",
            "dd if=",
            "git push --force",
            "git push -f",
            "git reset --hard",
            "git clean -f",
            "DROP TABLE",
            "DROP DATABASE",
            "TRUNCATE",
            "DELETE FROM",
            "shutdown",
            "reboot",
            "chmod 777",
            ":(){ :|:& };:",
        ];

        // Normalize: collapse whitespace to single space, lowercase
        let normalized: String = command
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();

        // Check direct patterns against normalized command
        if patterns.iter().any(|p| normalized.contains(&p.to_lowercase())) {
            return true;
        }

        // Check for shell metacharacter wrappers that could hide destructive commands
        let evasion_patterns = [
            "eval ",       // eval "rm -rf /"
            "base64",      // echo ... | base64 -d | sh
            "| sh",        // pipe to shell
            "| bash",      // pipe to bash
            "| zsh",       // pipe to zsh
            "xargs rm",    // xargs-based deletion
            "find.*-delete", // find with -delete
        ];

        for pattern in &evasion_patterns {
            if pattern.contains(".*") {
                // Treat as simple regex-like pattern
                let parts: Vec<&str> = pattern.split(".*").collect();
                if parts.len() == 2 {
                    if let (Some(start_pos), true) = (
                        normalized.find(parts[0]),
                        normalized.contains(parts[1]),
                    ) {
                        if let Some(end_pos) = normalized.find(parts[1]) {
                            if end_pos > start_pos {
                                return true;
                            }
                        }
                    }
                }
            } else if normalized.contains(pattern) {
                return true;
            }
        }

        false
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Use for running build commands, tests, git operations, and other CLI tools."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 120)"
                },
                "description": {
                    "type": "string",
                    "description": "Brief description of what this command does"
                }
            },
            "required": ["command"]
        })
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Ask
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'command' is required".to_string()))?;

        let timeout_secs = params["timeout"]
            .as_u64()
            .unwrap_or(self.default_timeout);

        // Build the command
        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };
        let shell_flag = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            tokio::process::Command::new(shell)
                .arg(shell_flag)
                .arg(command)
                .current_dir(&ctx.cwd)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result_text = String::new();
                if !stdout.is_empty() {
                    result_text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result_text.is_empty() {
                        result_text.push('\n');
                    }
                    result_text.push_str(&stderr);
                }

                if result_text.is_empty() {
                    result_text = "(no output)".to_string();
                }

                // Truncate excessively large output to prevent memory issues
                if result_text.len() > Self::MAX_OUTPUT_BYTES {
                    // Find a safe UTF-8 char boundary at or before MAX_OUTPUT_BYTES.
                    // We cannot slice at an arbitrary byte offset because it may
                    // land in the middle of a multi-byte UTF-8 character, which
                    // would panic in Rust.
                    let mut safe_end = Self::MAX_OUTPUT_BYTES;
                    while safe_end > 0 && !result_text.is_char_boundary(safe_end) {
                        safe_end -= 1;
                    }
                    let total_len = result_text.len();
                    result_text = format!(
                        "{}\n\n[Output truncated: {} bytes total, showing first {} bytes]",
                        &result_text[..safe_end],
                        total_len,
                        safe_end
                    );
                }

                // Add exit code info if non-zero
                if !output.status.success() {
                    let code = output.status.code().unwrap_or(-1);
                    result_text = format!("Exit code: {}\n{}", code, result_text);
                    Ok(ToolResult::error(result_text))
                } else {
                    Ok(ToolResult::success(result_text))
                }
            }
            Ok(Err(e)) => Err(ToolError::ExecutionFailed(e.to_string())),
            Err(_) => Err(ToolError::Timeout(timeout_secs)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_tool_properties() {
        let tool = BashTool::new();
        assert_eq!(tool.name(), "bash");
        assert_eq!(tool.permission_level(), PermissionLevel::Ask);
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_bash_schema() {
        let tool = BashTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("command")));
    }

    #[test]
    fn test_is_destructive() {
        assert!(BashTool::is_destructive("rm -rf /"));
        assert!(BashTool::is_destructive("git push --force"));
        assert!(BashTool::is_destructive("DROP TABLE users"));
        assert!(BashTool::is_destructive("git reset --hard"));
        assert!(!BashTool::is_destructive("ls -la"));
        assert!(!BashTool::is_destructive("git status"));
        assert!(!BashTool::is_destructive("cargo test"));
    }

    #[test]
    fn test_is_destructive_extra_whitespace() {
        // Extra spaces should still be detected after normalization
        assert!(BashTool::is_destructive("rm  -rf  /"));
        assert!(BashTool::is_destructive("git  push  --force"));
    }

    #[test]
    fn test_is_destructive_eval_wrapper() {
        assert!(BashTool::is_destructive("eval \"rm -rf /\""));
    }

    #[test]
    fn test_is_destructive_pipe_to_shell() {
        assert!(BashTool::is_destructive("echo something | sh"));
        assert!(BashTool::is_destructive("cat script.sh | bash"));
    }

    #[test]
    fn test_is_destructive_find_delete() {
        assert!(BashTool::is_destructive("find / -name '*.log' -delete"));
    }

    #[test]
    fn test_custom_timeout() {
        let tool = BashTool::with_timeout(60);
        assert_eq!(tool.default_timeout, 60);
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let tool = BashTool::new();
        let ctx = ToolContext::default();
        let params = serde_json::json!({"command": "echo hello"});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("hello"));
    }

    #[tokio::test]
    async fn test_execute_missing_command() {
        let tool = BashTool::new();
        let ctx = ToolContext::default();
        let params = serde_json::json!({});
        let result = tool.execute(params, &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_failing_command() {
        let tool = BashTool::new();
        let ctx = ToolContext::default();
        let params = serde_json::json!({"command": "exit 1"});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("Exit code: 1"));
    }

    #[tokio::test]
    async fn test_execute_with_timeout() {
        let tool = BashTool::with_timeout(1);
        let ctx = ToolContext::default();
        // Use a command that should complete quickly
        let params = serde_json::json!({"command": "echo fast", "timeout": 5});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_execute_captures_stderr() {
        let tool = BashTool::new();
        let ctx = ToolContext::default();
        let params = serde_json::json!({"command": "echo error >&2"});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(result.output.contains("error"));
    }

    #[test]
    fn test_truncation_utf8_boundary_safety() {
        // Bug fix R4: slicing at an arbitrary byte offset could panic if it
        // lands in the middle of a multi-byte UTF-8 character. Verify the
        // truncation logic finds a safe char boundary.
        let emoji = "\u{1F600}"; // 4-byte emoji
        assert_eq!(emoji.len(), 4);

        // Build a string where MAX_OUTPUT_BYTES would land inside a multi-byte char
        // We simulate by using a smaller limit for testing purposes
        let test_str = "a".repeat(BashTool::MAX_OUTPUT_BYTES - 2) + emoji;
        assert!(test_str.len() > BashTool::MAX_OUTPUT_BYTES);

        // The truncation code should find the safe boundary (MAX_OUTPUT_BYTES - 2)
        // rather than panicking at MAX_OUTPUT_BYTES (which is inside the emoji)
        let mut safe_end = BashTool::MAX_OUTPUT_BYTES;
        while safe_end > 0 && !test_str.is_char_boundary(safe_end) {
            safe_end -= 1;
        }
        // Should have backed off to before the emoji
        assert!(test_str.is_char_boundary(safe_end));
        assert_eq!(safe_end, BashTool::MAX_OUTPUT_BYTES - 2);
        // Slicing should not panic
        let _truncated = &test_str[..safe_end];
    }
}
