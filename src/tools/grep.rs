//! Grep tool — content search using regex

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use regex::Regex;
use std::path::PathBuf;

/// Tool for searching file contents using regex
pub struct GrepTool;

/// A single match found by grep
#[derive(Debug, Clone)]
pub struct GrepMatch {
    /// File path
    pub path: PathBuf,
    /// Line number (1-indexed)
    pub line_number: usize,
    /// The matching line content
    pub line: String,
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }

    /// Search a single file for matches
    ///
    /// Note: case sensitivity should be handled in the regex pattern itself
    /// (e.g., using `(?i)` prefix). The `_case_insensitive` parameter is
    /// retained for API compatibility but is not used -- the regex controls matching.
    pub fn search_file(
        path: &std::path::Path,
        pattern: &Regex,
        _case_insensitive: bool,
    ) -> Vec<GrepMatch> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(), // Skip binary/unreadable files
        };

        let mut matches = Vec::new();
        for (i, line) in content.lines().enumerate() {
            if pattern.is_match(line) {
                matches.push(GrepMatch {
                    path: path.to_path_buf(),
                    line_number: i + 1,
                    line: line.to_string(),
                });
            }
        }
        matches
    }

    /// Search directory recursively
    pub fn search_dir(
        dir: &std::path::Path,
        pattern: &Regex,
        file_glob: Option<&str>,
        case_insensitive: bool,
        max_results: usize,
    ) -> Vec<GrepMatch> {
        let mut all_matches = Vec::new();

        // If a file_glob is provided, use it to filter files
        let files = if let Some(glob_pattern) = file_glob {
            crate::tools::GlobTool::find_matches(glob_pattern, dir).unwrap_or_default()
        } else {
            // Walk directory recursively
            Self::walk_dir(dir)
        };

        for file_path in &files {
            if !file_path.is_file() {
                continue;
            }

            // Skip binary files (basic heuristic: check extension)
            if Self::is_likely_binary(file_path) {
                continue;
            }

            let file_matches = Self::search_file(file_path, pattern, case_insensitive);
            all_matches.extend(file_matches);

            if all_matches.len() >= max_results {
                all_matches.truncate(max_results);
                break;
            }
        }

        all_matches
    }

    /// Walk directory recursively and collect file paths
    fn walk_dir(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories and common ignores
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string());
                    if let Some(ref n) = name {
                        if n.starts_with('.') || n == "node_modules" || n == "target" || n == "__pycache__" {
                            continue;
                        }
                    }
                    files.extend(Self::walk_dir(&path));
                } else if path.is_file() {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    /// Check if a file is likely binary based on extension
    fn is_likely_binary(path: &std::path::Path) -> bool {
        let binary_exts = [
            "exe", "dll", "so", "dylib", "bin", "obj", "o", "a", "lib",
            "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svg",
            "zip", "tar", "gz", "bz2", "xz", "7z", "rar",
            "pdf", "doc", "docx", "xls", "xlsx",
            "wasm", "class", "pyc", "pyo",
        ];

        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| binary_exts.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents using a regular expression pattern. Returns matching lines with file paths and line numbers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (default: current directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., '*.rs', '**/*.ts')"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case insensitive search (default: false)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 100)"
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
        let pattern_str = params["pattern"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("'pattern' is required".to_string()))?;

        let case_insensitive = params["case_insensitive"].as_bool().unwrap_or(false);
        let max_results = params["max_results"].as_u64().unwrap_or(100) as usize;
        let file_glob = params["glob"].as_str();

        let search_path = if let Some(path) = params["path"].as_str() {
            let p = PathBuf::from(path);
            if p.is_absolute() {
                p
            } else {
                ctx.cwd.join(p)
            }
        } else {
            ctx.cwd.clone()
        };

        // Build regex with case sensitivity flag
        let regex_pattern = if case_insensitive {
            format!("(?i){}", pattern_str)
        } else {
            pattern_str.to_string()
        };

        let regex = Regex::new(&regex_pattern).map_err(|e| {
            ToolError::InvalidParams(format!("Invalid regex pattern '{}': {}", pattern_str, e))
        })?;

        let matches = if search_path.is_file() {
            Self::search_file(&search_path, &regex, false)
        } else {
            Self::search_dir(&search_path, &regex, file_glob, false, max_results)
        };

        if matches.is_empty() {
            Ok(ToolResult::success("No matches found."))
        } else {
            let mut output = String::new();
            let mut current_file = PathBuf::new();

            for m in &matches {
                if m.path != current_file {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&format!("{}:\n", m.path.display()));
                    current_file = m.path.clone();
                }
                output.push_str(&format!("  {}:{}\n", m.line_number, m.line));
            }

            Ok(ToolResult::success(format!(
                "{} matches found:\n{}",
                matches.len(),
                output
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_tool_properties() {
        let tool = GrepTool::new();
        assert_eq!(tool.name(), "grep");
        assert_eq!(tool.permission_level(), PermissionLevel::Allow);
    }

    #[test]
    fn test_search_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world\ngoodbye world\nhello again\n").unwrap();

        let regex = Regex::new("hello").unwrap();
        let matches = GrepTool::search_file(&file, &regex, false);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 1);
        assert_eq!(matches[1].line_number, 3);
    }

    #[test]
    fn test_search_file_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "nothing here\n").unwrap();

        let regex = Regex::new("xyz").unwrap();
        let matches = GrepTool::search_file(&file, &regex, false);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn test() {}\nfn main() {}\n").unwrap();

        let regex = Regex::new("fn main").unwrap();
        let matches = GrepTool::search_dir(dir.path(), &regex, None, false, 100);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_search_dir_with_glob() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "fn main() {}\n").unwrap();

        let regex = Regex::new("fn main").unwrap();
        let matches = GrepTool::search_dir(dir.path(), &regex, Some("*.rs"), false, 100);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_search_max_results() {
        let dir = tempfile::tempdir().unwrap();
        let mut content = String::new();
        for i in 0..50 {
            content.push_str(&format!("match line {}\n", i));
        }
        std::fs::write(dir.path().join("big.txt"), &content).unwrap();

        let regex = Regex::new("match").unwrap();
        let matches = GrepTool::search_dir(dir.path(), &regex, None, false, 10);
        assert!(matches.len() <= 10);
    }

    #[test]
    fn test_is_likely_binary() {
        assert!(GrepTool::is_likely_binary(std::path::Path::new("file.png")));
        assert!(GrepTool::is_likely_binary(std::path::Path::new("file.exe")));
        assert!(!GrepTool::is_likely_binary(std::path::Path::new("file.rs")));
        assert!(!GrepTool::is_likely_binary(std::path::Path::new("file.txt")));
    }

    #[tokio::test]
    async fn test_execute_grep() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let tool = GrepTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({"pattern": "println"});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("println"));
    }

    #[tokio::test]
    async fn test_execute_grep_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "nothing\n").unwrap();

        let tool = GrepTool::new();
        let ctx = ToolContext {
            cwd: dir.path().to_path_buf(),
            auto_approve: false,
        };
        let params = serde_json::json!({"pattern": "xyz123"});
        let result = tool.execute(params, &ctx).await.unwrap();
        assert!(result.output.contains("No matches"));
    }

    #[tokio::test]
    async fn test_execute_invalid_regex() {
        let tool = GrepTool::new();
        let ctx = ToolContext::default();
        let params = serde_json::json!({"pattern": "[invalid"});
        let result = tool.execute(params, &ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_grep_schema() {
        let tool = GrepTool::new();
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["pattern"].is_object());
    }

    #[test]
    fn test_walk_dir_skips_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let hidden = dir.path().join(".hidden");
        std::fs::create_dir(&hidden).unwrap();
        std::fs::write(hidden.join("file.txt"), "").unwrap();
        std::fs::write(dir.path().join("visible.txt"), "").unwrap();

        let files = GrepTool::walk_dir(dir.path());
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("visible"));
    }
}
