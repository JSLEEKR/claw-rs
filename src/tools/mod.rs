//! Tool system for the agent runtime
//!
//! Each tool implements the `Tool` trait and provides real functionality.

mod bash;
mod edit;
mod glob_tool;
mod grep;
mod read;
mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use glob_tool::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use write::WriteTool;

use crate::llm::ToolDefinition;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Tool execution error
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Timeout after {0}s")]
    Timeout(u64),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Output text
    pub output: String,

    /// Whether the result represents an error
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: false,
        }
    }

    /// Create an error result
    pub fn error(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
        }
    }
}

/// Permission level for a tool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    /// Always allowed without asking
    Allow,
    /// Requires user confirmation
    Ask,
    /// Always denied
    Deny,
}

/// Context provided to tools during execution
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Current working directory
    pub cwd: std::path::PathBuf,

    /// Whether running in auto-approve mode
    pub auto_approve: bool,
}

/// Validate that a resolved path does not traverse outside the working directory
/// using ".." components. Returns an error message if the path is suspicious.
pub fn validate_path_safety(path: &std::path::Path, cwd: &std::path::Path) -> Result<(), String> {
    // Normalize the path string to check for traversal patterns
    let path_str = path.to_string_lossy();

    // Check for null bytes (can bypass path checks on some systems)
    if path_str.contains('\0') {
        return Err("Path contains null byte".to_string());
    }

    // Check for ".." path traversal components
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            return Err("Path contains '..' traversal component".to_string());
        }
    }

    // For absolute paths, canonicalize both and verify the target is under cwd
    // (or at least not targeting sensitive system directories)
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    // Try to canonicalize to resolve symlinks; fall back to the raw path
    // if the target doesn't exist yet (e.g., write tool creating a new file).
    let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
    let normalized = canonical.to_string_lossy().replace('\\', "/").to_lowercase();

    // Strip UNC/extended-length prefix (\\?\) that Windows canonicalize adds
    let normalized = normalized
        .strip_prefix("//?/")
        .unwrap_or(&normalized)
        .to_string();

    // Check against sensitive system directories
    let sensitive_prefixes: &[&str] = if cfg!(target_os = "windows") {
        // Match any drive letter, not just C:
        &["/windows/", "/program files/", "/program files (x86)/"]
    } else {
        &["/etc/", "/usr/", "/bin/", "/sbin/", "/boot/", "/proc/", "/sys/"]
    };

    for prefix in sensitive_prefixes {
        // On Windows, check if the path (after drive letter) targets a sensitive dir
        let check_path = if cfg!(target_os = "windows") {
            // Strip drive letter prefix (e.g., "c:" or "d:")
            if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
                &normalized[2..]
            } else {
                &normalized
            }
        } else {
            &normalized
        };
        if check_path.starts_with(prefix) {
            return Err(format!("Path targets sensitive system directory: {}", prefix));
        }
    }

    Ok(())
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_default(),
            auto_approve: false,
        }
    }
}

/// Trait that all tools must implement
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (used by the LLM to call this tool)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with given parameters
    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError>;

    /// Default permission level for this tool
    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Ask
    }

    /// Convert to a ToolDefinition for the LLM API
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.parameters_schema(),
        }
    }
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Create a registry with all default tools
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(BashTool::new()));
        registry.register(Arc::new(ReadTool::new()));
        registry.register(Arc::new(WriteTool::new()));
        registry.register(Arc::new(EditTool::new()));
        registry.register(Arc::new(GlobTool::new()));
        registry.register(Arc::new(GrepTool::new()));
        registry
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// Execute a tool by name
    pub async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let tool = self.get(name).ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        tool.execute(params, ctx).await
    }

    /// Get all tool definitions for the LLM API
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<_> = self.tools.values().map(|t| t.to_definition()).collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Get all tool names
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("output");
        assert_eq!(result.output, "output");
        assert!(!result.is_error);
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("failed");
        assert_eq!(result.output, "failed");
        assert!(result.is_error);
    }

    #[test]
    fn test_registry_with_defaults() {
        let registry = ToolRegistry::with_defaults();
        assert_eq!(registry.len(), 6);
        assert!(registry.get("bash").is_some());
        assert!(registry.get("read").is_some());
        assert!(registry.get("write").is_some());
        assert!(registry.get("edit").is_some());
        assert!(registry.get("glob").is_some());
        assert!(registry.get("grep").is_some());
    }

    #[test]
    fn test_registry_names() {
        let registry = ToolRegistry::with_defaults();
        let names = registry.names();
        assert!(names.contains(&"bash".to_string()));
        assert!(names.contains(&"read".to_string()));
    }

    #[test]
    fn test_registry_definitions() {
        let registry = ToolRegistry::with_defaults();
        let defs = registry.definitions();
        assert_eq!(defs.len(), 6);
        // Check sorted by name
        for i in 1..defs.len() {
            assert!(defs[i - 1].name <= defs[i].name);
        }
    }

    #[test]
    fn test_registry_not_found() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_empty() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_tool_context_default() {
        let ctx = ToolContext::default();
        assert!(!ctx.auto_approve);
    }

    #[test]
    fn test_validate_path_safety_normal() {
        let cwd = std::path::Path::new("/home/user/project");
        let path = std::path::Path::new("/home/user/project/src/main.rs");
        assert!(validate_path_safety(path, cwd).is_ok());
    }

    #[test]
    fn test_validate_path_safety_sensitive_unix() {
        let cwd = std::path::Path::new("/home/user");
        let path = std::path::Path::new("/etc/passwd");
        if !cfg!(target_os = "windows") {
            assert!(validate_path_safety(path, cwd).is_err());
        }
    }

    #[test]
    fn test_validate_path_safety_null_byte() {
        let cwd = std::path::Path::new("/home/user");
        let path = std::path::Path::new("/home/user/file\0.txt");
        assert!(validate_path_safety(path, cwd).is_err());
    }

    #[test]
    fn test_validate_path_safety_traversal() {
        let cwd = std::path::Path::new("/home/user/project");
        let path = std::path::Path::new("/home/user/project/../../etc/shadow");
        assert!(validate_path_safety(path, cwd).is_err());
    }

    #[test]
    fn test_permission_level_variants() {
        assert_eq!(PermissionLevel::Allow, PermissionLevel::Allow);
        assert_ne!(PermissionLevel::Allow, PermissionLevel::Deny);
    }

    #[test]
    fn test_validate_path_safety_windows_sensitive() {
        // Bug fix R4: sensitive directory check should work for any drive letter
        if cfg!(target_os = "windows") {
            let cwd = std::path::Path::new("C:\\Users\\user\\project");
            // C:\Windows should be blocked
            let path = std::path::Path::new("C:\\Windows\\System32\\cmd.exe");
            assert!(validate_path_safety(path, cwd).is_err());
            // D:\Windows should also be blocked (other drive letters)
            let path2 = std::path::Path::new("D:\\Windows\\System32\\cmd.exe");
            assert!(validate_path_safety(path2, cwd).is_err());
        }
    }
}
