//! Edit tool — string replacement in files

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use std::path::PathBuf;

/// Tool for editing files via exact string replacement
pub struct EditTool;

impl EditTool {
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

    /// Perform exact string replacement in content
    /// Returns the new content or an error message
    pub fn apply_edit(
        content: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<String, String> {
        if old_string.is_empty() {
            return Err("old_string cannot be empty".to_string());
        }

        if old_string == new_string {
            return Err("old_string and new_string are identical".to_string());
        }

        let count = content.matches(old_string).count();

        if count == 0 {
            return Err(format!(
                "old_string not found in file. Make sure it matches exactly (including whitespace and indentation)."
            ));
        }

        if count > 1 && !replace_all {
            return Err(format!(
                "old_string found {} times. Use replace_all=true to replace all occurrences, or provide more context to make it unique.",
                count
            ));
        }

        if replace_all {
            Ok(content.replace(old_string, new_string))
        } else {
            // Replace only the first occurrence
            Ok(content.replacen(old_string, new_string, 1))
        }
    }
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing an exact string with a new string. The old_string must be unique in the file unless replace_all is true."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to replace (must match exactly including whitespace)"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
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

        let old_string = params["old_string"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'old_string' is required".to_string()))?;

        let new_string = params["new_string"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'new_string' is required".to_string()))?;

        let replace_all = params["replace_all"].as_bool().unwrap_or(false);

        let path = Self::resolve_path(file_path, &ctx.cwd);

        // Validate path safety
        if let Err(msg) = super::validate_path_safety(&path, &ctx.cwd) {
            return Ok(ToolResult::error(format!("Unsafe path: {}", msg)));
        }

        if !path.exists() {
            return Ok(ToolResult::error(format!("File not found: {}", path.display())));
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::Io(e))?;

        match Self::apply_edit(&content, old_string, new_string, replace_all) {
            Ok(new_content) => {
                tokio::fs::write(&path, &new_content)
                    .await
                    .map_err(|e| ToolError::Io(e))?;

                let old_lines = old_string.lines().count();
                let new_lines = new_string.lines().count();
                Ok(ToolResult::success(format!(
                    "Edited {} ({} lines replaced with {} lines)",
                    path.display(),
                    old_lines,
                    new_lines
                )))
            }
            Err(msg) => Ok(ToolResult::error(msg)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_tool_properties() {
        let tool = EditTool::new();
        assert_eq!(tool.name(), "edit");
        assert_eq!(tool.permission_level(), PermissionLevel::Ask);
    }

    #[test]
    fn test_apply_edit_simple() {
        let content = "hello world";
        let result = EditTool::apply_edit(content, "hello", "goodbye", false);
        assert_eq!(result.unwrap(), "goodbye world");
    }

    #[test]
    fn test_apply_edit_not_found() {
        let content = "hello world";
        let result = EditTool::apply_edit(content, "xyz", "abc", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_apply_edit_ambiguous() {
        let content = "hello hello world";
        let result = EditTool::apply_edit(content, "hello", "bye", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("2 times"));
    }

    #[test]
    fn test_apply_edit_replace_all() {
        let content = "hello hello world";
        let result = EditTool::apply_edit(content, "hello", "bye", true);
        assert_eq!(result.unwrap(), "bye bye world");
    }

    #[test]
    fn test_apply_edit_empty_old() {
        let result = EditTool::apply_edit("content", "", "new", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_edit_identical_strings() {
        let result = EditTool::apply_edit("content", "same", "same", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("identical"));
    }

    #[test]
    fn test_apply_edit_multiline() {
        let content = "line1\nline2\nline3\n";
        let result = EditTool::apply_edit(content, "line2\nline3", "new2\nnew3\nnew4", false);
        assert_eq!(result.unwrap(), "line1\nnew2\nnew3\nnew4\n");
    }

    #[test]
    fn test_apply_edit_preserves_indentation() {
        let content = "    indented line\n    another line";
        let result = EditTool::apply_edit(content, "    indented line", "    replaced line", false);
        assert_eq!(result.unwrap(), "    replaced line\n    another line");
    }

    #[tokio::test]
    async fn test_execute_edit_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let tool = EditTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_string": "hello",
            "new_string": "goodbye"
        });

        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "goodbye world");
    }

    #[tokio::test]
    async fn test_execute_edit_nonexistent() {
        let tool = EditTool::new();
        let ctx = ToolContext::default();
        let params = serde_json::json!({
            "file_path": "/nonexistent/file.txt",
            "old_string": "a",
            "new_string": "b"
        });

        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn test_edit_schema() {
        let tool = EditTool::new();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 3);
    }
}
