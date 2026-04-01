//! Notebook tool — read and display Jupyter notebooks

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;

/// A Jupyter notebook cell
#[derive(Debug, Deserialize)]
struct NotebookCell {
    /// Cell type: "code", "markdown", "raw"
    cell_type: String,
    /// Cell source lines
    source: Vec<String>,
    /// Cell outputs (for code cells)
    #[serde(default)]
    outputs: Vec<CellOutput>,
    /// Execution count (for code cells)
    #[serde(default)]
    execution_count: Option<u64>,
}

/// A cell output
#[derive(Debug, Deserialize)]
struct CellOutput {
    /// Output type: "stream", "execute_result", "display_data", "error"
    output_type: String,
    /// Text content (for stream outputs)
    #[serde(default)]
    text: Vec<String>,
    /// Data content (for execute_result, display_data)
    #[serde(default)]
    data: Option<serde_json::Value>,
    /// Error traceback
    #[serde(default)]
    traceback: Vec<String>,
}

/// Minimal notebook structure
#[derive(Debug, Deserialize)]
struct Notebook {
    cells: Vec<NotebookCell>,
    #[serde(default)]
    metadata: serde_json::Value,
}

/// Tool for reading Jupyter notebooks
pub struct NotebookTool;

impl NotebookTool {
    pub fn new() -> Self {
        Self
    }

    /// Maximum file size we will read (50 MB).
    /// Prevents OOM on accidental reads of huge notebook files.
    const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

    /// Format a notebook as readable text
    pub fn format_notebook(content: &str) -> Result<String, String> {
        let notebook: Notebook =
            serde_json::from_str(content).map_err(|e| format!("Invalid notebook JSON: {}", e))?;

        let mut output = String::new();

        // Extract kernel info from metadata
        if let Some(kernelspec) = notebook.metadata.get("kernelspec") {
            if let Some(language) = kernelspec.get("language") {
                output.push_str(&format!("Kernel language: {}\n\n", language.as_str().unwrap_or("unknown")));
            }
        }

        for (i, cell) in notebook.cells.iter().enumerate() {
            match cell.cell_type.as_str() {
                "code" => {
                    let exec_label = cell
                        .execution_count
                        .map(|n| format!("In [{}]", n))
                        .unwrap_or_else(|| "In [ ]".to_string());

                    output.push_str(&format!("--- Cell {} ({}) ---\n", i + 1, exec_label));
                    let source: String = cell.source.join("");
                    output.push_str(&source);
                    if !source.ends_with('\n') {
                        output.push('\n');
                    }

                    // Format outputs
                    for out in &cell.outputs {
                        match out.output_type.as_str() {
                            "stream" => {
                                output.push_str(&format!("Output:\n{}", out.text.join("")));
                            }
                            "execute_result" | "display_data" => {
                                if let Some(data) = &out.data {
                                    if let Some(text) = data.get("text/plain") {
                                        match text {
                                            serde_json::Value::Array(lines) => {
                                                output.push_str("Output:\n");
                                                for line in lines {
                                                    if let Some(s) = line.as_str() {
                                                        output.push_str(s);
                                                    }
                                                }
                                            }
                                            serde_json::Value::String(s) => {
                                                output.push_str(&format!("Output:\n{}", s));
                                            }
                                            _ => {}
                                        }
                                    }
                                    if data.get("image/png").is_some() {
                                        output.push_str("[Image: PNG data]\n");
                                    }
                                }
                            }
                            "error" => {
                                output.push_str("Error:\n");
                                for line in &out.traceback {
                                    // Strip ANSI escape codes from traceback
                                    output.push_str(&strip_ansi(line));
                                    output.push('\n');
                                }
                            }
                            _ => {}
                        }
                    }
                    output.push('\n');
                }
                "markdown" => {
                    output.push_str(&format!("--- Cell {} (Markdown) ---\n", i + 1));
                    output.push_str(&cell.source.join(""));
                    output.push_str("\n\n");
                }
                "raw" => {
                    output.push_str(&format!("--- Cell {} (Raw) ---\n", i + 1));
                    output.push_str(&cell.source.join(""));
                    output.push_str("\n\n");
                }
                _ => {}
            }
        }

        Ok(output)
    }
}

/// Strip ANSI escape codes from a string
fn strip_ansi(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\x1b' && i + 1 < chars.len() && chars[i + 1] == '[' {
            // Skip until 'm' or end
            i += 2;
            while i < chars.len() && chars[i] != 'm' {
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip 'm'
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

#[async_trait]
impl Tool for NotebookTool {
    fn name(&self) -> &str {
        "notebook"
    }

    fn description(&self) -> &str {
        "Read and display Jupyter notebook (.ipynb) files. Shows cells with their source code, outputs, and metadata."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the .ipynb file"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing 'file_path' parameter".into()))?;

        let path = PathBuf::from(file_path);
        let resolved = if path.is_absolute() {
            path
        } else {
            ctx.cwd.join(path)
        };

        // Validate path safety
        super::validate_path_safety(&resolved, &ctx.cwd)
            .map_err(|e| ToolError::InvalidParams(e))?;

        if !resolved.exists() {
            return Ok(ToolResult::error(format!(
                "File not found: {}",
                resolved.display()
            )));
        }

        // Check extension
        match resolved.extension().and_then(|e| e.to_str()) {
            Some("ipynb") => {}
            _ => {
                return Ok(ToolResult::error("File must have .ipynb extension"));
            }
        }

        // Guard against reading excessively large files (OOM prevention)
        let metadata = tokio::fs::metadata(&resolved)
            .await
            .map_err(|e| ToolError::Io(e))?;
        if metadata.len() > Self::MAX_FILE_BYTES {
            return Ok(ToolResult::error(format!(
                "Notebook file too large: {} bytes (max {} bytes)",
                metadata.len(),
                Self::MAX_FILE_BYTES
            )));
        }

        let content = tokio::fs::read_to_string(&resolved)
            .await
            .map_err(|e| ToolError::Io(e))?;

        match Self::format_notebook(&content) {
            Ok(formatted) => Ok(ToolResult::success(formatted)),
            Err(e) => Ok(ToolResult::error(e)),
        }
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_notebook() -> &'static str {
        r##"{
            "cells": [
                {
                    "cell_type": "markdown",
                    "metadata": {},
                    "source": ["# Hello Notebook\n", "This is a test."]
                },
                {
                    "cell_type": "code",
                    "execution_count": 1,
                    "metadata": {},
                    "source": ["print('hello world')"],
                    "outputs": [
                        {
                            "output_type": "stream",
                            "name": "stdout",
                            "text": ["hello world\n"]
                        }
                    ]
                },
                {
                    "cell_type": "code",
                    "execution_count": 2,
                    "metadata": {},
                    "source": ["x = 42\n", "x"],
                    "outputs": [
                        {
                            "output_type": "execute_result",
                            "data": {
                                "text/plain": "42"
                            },
                            "metadata": {},
                            "execution_count": 2
                        }
                    ]
                }
            ],
            "metadata": {
                "kernelspec": {
                    "display_name": "Python 3",
                    "language": "python"
                }
            },
            "nbformat": 4,
            "nbformat_minor": 5
        }"##
    }

    #[test]
    fn test_format_notebook() {
        let result = NotebookTool::format_notebook(sample_notebook()).unwrap();
        assert!(result.contains("Kernel language: python"));
        assert!(result.contains("Hello Notebook"));
        assert!(result.contains("print('hello world')"));
        assert!(result.contains("hello world"));
        assert!(result.contains("In [1]"));
        assert!(result.contains("In [2]"));
    }

    #[test]
    fn test_format_notebook_markdown_cell() {
        let result = NotebookTool::format_notebook(sample_notebook()).unwrap();
        assert!(result.contains("Markdown"));
        assert!(result.contains("Hello Notebook"));
    }

    #[test]
    fn test_format_notebook_code_output() {
        let result = NotebookTool::format_notebook(sample_notebook()).unwrap();
        assert!(result.contains("Output:"));
        assert!(result.contains("42"));
    }

    #[test]
    fn test_format_notebook_invalid_json() {
        let result = NotebookTool::format_notebook("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_notebook_error_output() {
        let nb = r#"{
            "cells": [{
                "cell_type": "code",
                "source": ["1/0"],
                "outputs": [{
                    "output_type": "error",
                    "ename": "ZeroDivisionError",
                    "evalue": "division by zero",
                    "traceback": ["\u001b[31mZeroDivisionError\u001b[0m: division by zero"]
                }]
            }],
            "metadata": {},
            "nbformat": 4,
            "nbformat_minor": 5
        }"#;
        let result = NotebookTool::format_notebook(nb).unwrap();
        assert!(result.contains("Error:"));
        assert!(result.contains("ZeroDivisionError"));
        // ANSI codes should be stripped
        assert!(!result.contains("\x1b"));
    }

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
        assert_eq!(strip_ansi("\x1b[1;32mbold green\x1b[0m"), "bold green");
    }

    #[test]
    fn test_tool_name() {
        let tool = NotebookTool::new();
        assert_eq!(tool.name(), "notebook");
    }

    #[test]
    fn test_tool_schema() {
        let tool = NotebookTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["file_path"].is_object());
    }

    #[test]
    fn test_permission_level() {
        let tool = NotebookTool::new();
        assert_eq!(tool.permission_level(), PermissionLevel::Allow);
    }

    #[tokio::test]
    async fn test_missing_file_path() {
        let tool = NotebookTool::new();
        let ctx = ToolContext::default();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_nonexistent_file() {
        let tool = NotebookTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"file_path": "/nonexistent/test.ipynb"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_wrong_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "not a notebook").unwrap();
        let tool = NotebookTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: true,
        };
        let result = tool
            .execute(serde_json::json!({"file_path": path.to_str().unwrap()}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.output.contains(".ipynb"));
    }

    #[test]
    fn test_max_file_bytes_constant() {
        // Bug fix R2: notebook tool must have a file size guard to prevent OOM
        assert!(NotebookTool::MAX_FILE_BYTES > 0);
        assert_eq!(NotebookTool::MAX_FILE_BYTES, 50 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_read_notebook_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ipynb");
        std::fs::write(&path, sample_notebook()).unwrap();
        let tool = NotebookTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: true,
        };
        let result = tool
            .execute(serde_json::json!({"file_path": path.to_str().unwrap()}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("Hello Notebook"));
    }
}
