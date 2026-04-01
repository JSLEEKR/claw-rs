//! Ask user tool — interactive prompt for user input

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;

/// Tool for asking the user for input during agent execution
pub struct AskUserTool {
    /// Callback for getting user input (returns None if user cancels)
    input_callback: Box<dyn Fn(&str) -> Option<String> + Send + Sync>,
}

impl AskUserTool {
    /// Create a new ask user tool with a custom input callback
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(&str) -> Option<String> + Send + Sync + 'static,
    {
        Self {
            input_callback: Box::new(callback),
        }
    }

    /// Create a tool that uses stdin for input (default)
    pub fn with_stdin() -> Self {
        Self::new(|prompt| {
            use std::io::{self, Write};
            eprint!("{} ", prompt);
            io::stderr().flush().ok()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input).ok()?;
            let trimmed = input.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
    }

    /// Create a tool that always returns a fixed response (for testing)
    pub fn with_fixed_response(response: String) -> Self {
        Self::new(move |_| Some(response.clone()))
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the user a question and get their response. Use this when you need clarification or a decision from the user."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let question = params["question"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing 'question' parameter".into()))?;

        match (self.input_callback)(question) {
            Some(response) => Ok(ToolResult::success(response)),
            None => Ok(ToolResult::success("(no response from user)")),
        }
    }

    fn permission_level(&self) -> PermissionLevel {
        // Always allowed — the tool IS user interaction
        PermissionLevel::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = AskUserTool::with_fixed_response("yes".into());
        assert_eq!(tool.name(), "ask_user");
    }

    #[test]
    fn test_tool_description() {
        let tool = AskUserTool::with_fixed_response("ok".into());
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_tool_schema() {
        let tool = AskUserTool::with_fixed_response("ok".into());
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["question"].is_object());
        assert_eq!(schema["required"][0], "question");
    }

    #[test]
    fn test_permission_level() {
        let tool = AskUserTool::with_fixed_response("ok".into());
        assert_eq!(tool.permission_level(), PermissionLevel::Allow);
    }

    #[tokio::test]
    async fn test_execute_with_fixed_response() {
        let tool = AskUserTool::with_fixed_response("yes".into());
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"question": "Continue?"}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.output, "yes");
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_execute_with_custom_callback() {
        let tool = AskUserTool::new(|q| {
            if q.contains("name") {
                Some("Alice".into())
            } else {
                Some("ok".into())
            }
        });
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"question": "What is your name?"}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.output, "Alice");
    }

    #[tokio::test]
    async fn test_execute_no_response() {
        let tool = AskUserTool::new(|_| None);
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"question": "Hello?"}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.output, "(no response from user)");
    }

    #[tokio::test]
    async fn test_missing_question() {
        let tool = AskUserTool::with_fixed_response("ok".into());
        let ctx = ToolContext::default();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }
}
