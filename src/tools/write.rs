//! Write tool — write files to disk

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use std::path::PathBuf;

/// Tool for writing files
pub struct WriteTool;

impl WriteTool {
    pub fn new() -> Self {
        Self
    }

    fn resolve_path(path: &str, cwd: &std::path::Path) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            cwd.join(p)
        }
    }
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, creates parent directories as needed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
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
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'file_path' is required".to_string()))?;

        let content = params["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'content' is required".to_string()))?;

        let path = Self::resolve_path(file_path, &ctx.cwd);

        // Validate path safety
        if let Err(msg) = super::validate_path_safety(&path, &ctx.cwd) {
            return Ok(ToolResult::error(format!("Unsafe path: {}", msg)));
        }

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| ToolError::Io(e))?;
            }
        }

        let existed = path.exists();
        let bytes_written = content.len();

        tokio::fs::write(&path, content)
            .await
            .map_err(|e| ToolError::Io(e))?;

        let action = if existed { "Updated" } else { "Created" };
        Ok(ToolResult::success(format!(
            "{} {} ({} bytes)",
            action,
            path.display(),
            bytes_written
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_tool_properties() {
        let tool = WriteTool::new();
        assert_eq!(tool.name(), "write");
        assert_eq!(tool.permission_level(), PermissionLevel::Ask);
    }

    #[test]
    fn test_write_schema() {
        let tool = WriteTool::new();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("file_path")));
        assert!(required.contains(&serde_json::json!("content")));
    }

    #[tokio::test]
    async fn test_write_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("new_file.txt");

        let tool = WriteTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "content": "hello world"
        });

        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("Created"));

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_write_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("existing.txt");
        std::fs::write(&file_path, "old content").unwrap();

        let tool = WriteTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "content": "new content"
        });

        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("Updated"));

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("a").join("b").join("c").join("file.txt");

        let tool = WriteTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "content": "nested"
        });

        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_write_missing_params() {
        let tool = WriteTool::new();
        let ctx = ToolContext::default();

        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());

        let result = tool
            .execute(serde_json::json!({"file_path": "test.txt"}), &ctx)
            .await;
        assert!(result.is_err());
    }
}
