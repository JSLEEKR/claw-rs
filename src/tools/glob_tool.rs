//! Glob tool — file pattern matching

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use std::path::PathBuf;

/// Tool for finding files by glob pattern
pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }

    /// Execute a glob pattern and return matching paths
    pub fn find_matches(pattern: &str, base_dir: &std::path::Path) -> Result<Vec<PathBuf>, String> {
        // If pattern is absolute, use it directly; otherwise join with base_dir
        let full_pattern = if PathBuf::from(pattern).is_absolute() {
            pattern.to_string()
        } else {
            let base = base_dir.to_string_lossy();
            // Normalize separators for glob
            let base = base.replace('\\', "/");
            format!("{}/{}", base.trim_end_matches('/'), pattern)
        };

        let entries = glob::glob(&full_pattern).map_err(|e| format!("Invalid glob pattern: {}", e))?;

        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries {
            match entry {
                Ok(path) => paths.push(path),
                Err(e) => {
                    // Skip individual errors (permission denied, etc.)
                    tracing::debug!("Glob entry error: {}", e);
                }
            }
        }

        // Sort by path for deterministic output
        paths.sort();
        Ok(paths)
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Supports patterns like '**/*.rs', 'src/**/*.ts', etc."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g., '**/*.rs')"
                },
                "path": {
                    "type": "string",
                    "description": "The base directory to search in (default: current directory)"
                }
            },
            "required": ["pattern"]
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
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'pattern' is required".to_string()))?;

        let base_dir = if let Some(path) = params["path"].as_str() {
            let p = PathBuf::from(path);
            if p.is_absolute() {
                p
            } else {
                ctx.cwd.join(p)
            }
        } else {
            ctx.cwd.clone()
        };

        match Self::find_matches(pattern, &base_dir) {
            Ok(paths) => {
                if paths.is_empty() {
                    Ok(ToolResult::success("No files matched the pattern."))
                } else {
                    let output = paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(ToolResult::success(format!(
                        "{} files matched:\n{}",
                        paths.len(),
                        output
                    )))
                }
            }
            Err(e) => Ok(ToolResult::error(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_tool_properties() {
        let tool = GlobTool::new();
        assert_eq!(tool.name(), "glob");
        assert_eq!(tool.permission_level(), PermissionLevel::Allow);
    }

    #[test]
    fn test_glob_schema() {
        let tool = GlobTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("pattern")));
    }

    #[test]
    fn test_find_matches_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.rs"), "").unwrap();
        std::fs::write(dir.path().join("test.txt"), "").unwrap();
        std::fs::write(dir.path().join("other.rs"), "").unwrap();

        let matches = GlobTool::find_matches("*.rs", dir.path()).unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_find_matches_nested() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(dir.path().join("root.rs"), "").unwrap();
        std::fs::write(sub.join("nested.rs"), "").unwrap();

        let matches = GlobTool::find_matches("**/*.rs", dir.path()).unwrap();
        assert!(matches.len() >= 2);
    }

    #[test]
    fn test_find_matches_no_results() {
        let dir = tempfile::tempdir().unwrap();
        let matches = GlobTool::find_matches("*.xyz", dir.path()).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_matches_invalid_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let result = GlobTool::find_matches("[invalid", dir.path());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_glob() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();

        let tool = GlobTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({"pattern": "*.txt"});

        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("2 files matched"));
    }

    #[tokio::test]
    async fn test_execute_glob_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GlobTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({"pattern": "*.nonexistent"});

        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("No files matched"));
    }
}
