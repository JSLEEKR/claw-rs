//! Read tool — read files with line number support

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use std::path::PathBuf;

/// Tool for reading file contents
pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self {
        Self
    }

    /// Resolve a file path (relative to cwd if not absolute)
    fn resolve_path(path: &str, cwd: &std::path::Path) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            cwd.join(p)
        }
    }

    /// Format file content with line numbers
    pub fn format_with_line_numbers(content: &str, offset: usize, limit: Option<usize>) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.min(total_lines);
        let end = if let Some(limit) = limit {
            (start + limit).min(total_lines)
        } else {
            total_lines
        };

        if start >= total_lines {
            return format!("(file has {} lines, offset {} is past end)", total_lines, offset);
        }

        let width = format!("{}", end).len();
        let mut output = String::new();

        for (i, line) in lines[start..end].iter().enumerate() {
            let line_num = start + i + 1; // 1-indexed
            output.push_str(&format!("{:>width$}\t{}\n", line_num, line, width = width));
        }

        output
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read a file's contents with line numbers. Supports reading specific ranges with offset and limit parameters."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line offset to start reading from (0-indexed, default: 0)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read"
                }
            },
            "required": ["file_path"]
        })
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Allow
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'file_path' is required".to_string()))?;

        let offset = params["offset"].as_u64().unwrap_or(0) as usize;
        let limit = params["limit"].as_u64().map(|v| v as usize);

        let path = Self::resolve_path(file_path, &ctx.cwd);

        if !path.exists() {
            return Ok(ToolResult::error(format!("File not found: {}", path.display())));
        }

        if !path.is_file() {
            return Ok(ToolResult::error(format!("Not a file: {}", path.display())));
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::Io(e))?;

        let formatted = Self::format_with_line_numbers(&content, offset, limit);
        Ok(ToolResult::success(formatted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_tool_properties() {
        let tool = ReadTool::new();
        assert_eq!(tool.name(), "read");
        assert_eq!(tool.permission_level(), PermissionLevel::Allow);
    }

    #[test]
    fn test_format_with_line_numbers() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let formatted = ReadTool::format_with_line_numbers(content, 0, None);
        assert!(formatted.contains("1\tline1"));
        assert!(formatted.contains("5\tline5"));
    }

    #[test]
    fn test_format_with_offset() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let formatted = ReadTool::format_with_line_numbers(content, 2, None);
        assert!(!formatted.contains("1\tline1"));
        assert!(formatted.contains("3\tline3"));
        assert!(formatted.contains("5\tline5"));
    }

    #[test]
    fn test_format_with_limit() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let formatted = ReadTool::format_with_line_numbers(content, 0, Some(2));
        assert!(formatted.contains("1\tline1"));
        assert!(formatted.contains("2\tline2"));
        assert!(!formatted.contains("3\tline3"));
    }

    #[test]
    fn test_format_with_offset_and_limit() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let formatted = ReadTool::format_with_line_numbers(content, 1, Some(2));
        assert!(!formatted.contains("1\tline1"));
        assert!(formatted.contains("2\tline2"));
        assert!(formatted.contains("3\tline3"));
        assert!(!formatted.contains("4\tline4"));
    }

    #[test]
    fn test_format_offset_past_end() {
        let content = "line1\nline2";
        let formatted = ReadTool::format_with_line_numbers(content, 10, None);
        assert!(formatted.contains("past end"));
    }

    #[test]
    fn test_format_empty_content() {
        let content = "";
        let formatted = ReadTool::format_with_line_numbers(content, 0, None);
        // Empty file has 0 lines, offset 0 is past end
        assert!(formatted.contains("past end") || formatted.is_empty());
    }

    #[test]
    fn test_resolve_path_absolute() {
        let cwd = std::path::Path::new("/home/user");
        let path = ReadTool::resolve_path("/etc/config", cwd);
        assert_eq!(path, PathBuf::from("/etc/config"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let cwd = std::path::Path::new("/home/user");
        let path = ReadTool::resolve_path("file.txt", cwd);
        assert_eq!(path, PathBuf::from("/home/user/file.txt"));
    }

    #[tokio::test]
    async fn test_execute_nonexistent_file() {
        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let params = serde_json::json!({"file_path": "/nonexistent/file.txt"});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("not found"));
    }

    #[tokio::test]
    async fn test_execute_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello\nworld\n").unwrap();

        let tool = ReadTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({"file_path": file_path.to_str().unwrap()});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("hello"));
        assert!(result.output.contains("world"));
    }

    #[test]
    fn test_read_schema() {
        let tool = ReadTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["file_path"].is_object());
    }
}
