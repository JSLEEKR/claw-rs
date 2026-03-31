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

    /// Check if a command is potentially destructive
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

        let lower = command.to_lowercase();
        patterns.iter().any(|p| lower.contains(&p.to_lowercase()))
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
}
