//! Workspace context discovery
//!
//! Detects project type, source layout, git info, and configuration files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Detected project type based on manifest files
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    Rust,
    Go,
    Python,
    TypeScript,
    JavaScript,
    Java,
    Unknown,
}

impl ProjectType {
    /// Display name for the project type
    pub fn name(&self) -> &str {
        match self {
            Self::Rust => "Rust",
            Self::Go => "Go",
            Self::Python => "Python",
            Self::TypeScript => "TypeScript",
            Self::JavaScript => "JavaScript",
            Self::Java => "Java",
            Self::Unknown => "Unknown",
        }
    }

    /// Common source file extension
    pub fn extension(&self) -> &str {
        match self {
            Self::Rust => "rs",
            Self::Go => "go",
            Self::Python => "py",
            Self::TypeScript => "ts",
            Self::JavaScript => "js",
            Self::Java => "java",
            Self::Unknown => "",
        }
    }
}

/// Git repository information
#[derive(Debug, Clone, Default)]
pub struct GitInfo {
    /// Current branch name
    pub branch: Option<String>,
    /// Remote URL (origin)
    pub remote: Option<String>,
    /// Whether the working tree has uncommitted changes
    pub is_dirty: bool,
    /// Number of untracked files
    pub untracked_count: usize,
}

/// Full workspace context
#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    /// Root directory of the workspace
    pub root: PathBuf,
    /// Detected project type
    pub project_type: ProjectType,
    /// Source root directory (e.g., src/ or lib/)
    pub source_root: Option<PathBuf>,
    /// Test root directory (e.g., tests/ or test/)
    pub test_root: Option<PathBuf>,
    /// File counts by extension
    pub file_counts: HashMap<String, usize>,
    /// Git information (if in a git repo)
    pub git_info: Option<GitInfo>,
    /// Path to CLAUDE.md if found
    pub claude_md: Option<PathBuf>,
    /// Path to .claude/ directory if found
    pub claude_dir: Option<PathBuf>,
}

impl WorkspaceContext {
    /// Total number of source files detected
    pub fn total_source_files(&self) -> usize {
        self.file_counts.values().sum()
    }

    /// Generate a summary string of the workspace
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("Project: {} ({})", self.root.display(), self.project_type.name()));

        if let Some(ref src) = self.source_root {
            parts.push(format!("Source root: {}", src.display()));
        }
        if let Some(ref test) = self.test_root {
            parts.push(format!("Test root: {}", test.display()));
        }

        let total = self.total_source_files();
        if total > 0 {
            parts.push(format!("Source files: {}", total));
            let mut counts: Vec<_> = self.file_counts.iter().collect();
            counts.sort_by(|a, b| b.1.cmp(a.1));
            for (ext, count) in counts.iter().take(5) {
                parts.push(format!("  .{}: {}", ext, count));
            }
        }

        if let Some(ref git) = self.git_info {
            if let Some(ref branch) = git.branch {
                parts.push(format!("Git branch: {}", branch));
            }
            if git.is_dirty {
                parts.push("Git status: dirty".to_string());
            }
        }

        if self.claude_md.is_some() {
            parts.push("CLAUDE.md: found".to_string());
        }

        parts.join("\n")
    }
}

/// Detect the project type from files in a directory
pub fn detect_project_type(root: &Path) -> ProjectType {
    // Order matters: more specific first
    if root.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if root.join("go.mod").exists() {
        ProjectType::Go
    } else if root.join("tsconfig.json").exists() {
        ProjectType::TypeScript
    } else if root.join("pyproject.toml").exists() || root.join("setup.py").exists() || root.join("requirements.txt").exists() {
        ProjectType::Python
    } else if root.join("package.json").exists() {
        ProjectType::JavaScript
    } else if root.join("pom.xml").exists() || root.join("build.gradle").exists() {
        ProjectType::Java
    } else {
        ProjectType::Unknown
    }
}

/// Find the source root directory for a given project type
pub fn find_source_root(root: &Path, project_type: &ProjectType) -> Option<PathBuf> {
    let candidates = match project_type {
        ProjectType::Rust => vec!["src"],
        ProjectType::Go => vec!["cmd", "pkg", "internal", "."],
        ProjectType::Python => vec!["src", "lib", "."],
        ProjectType::TypeScript | ProjectType::JavaScript => vec!["src", "lib"],
        ProjectType::Java => vec!["src/main/java", "src"],
        ProjectType::Unknown => vec!["src", "lib"],
    };

    for candidate in candidates {
        let path = root.join(candidate);
        if path.exists() && path.is_dir() {
            return Some(path);
        }
    }
    None
}

/// Find the test root directory for a given project type
pub fn find_test_root(root: &Path, project_type: &ProjectType) -> Option<PathBuf> {
    let candidates = match project_type {
        ProjectType::Rust => vec!["tests"],
        ProjectType::Go => vec!["tests", "."],
        ProjectType::Python => vec!["tests", "test"],
        ProjectType::TypeScript | ProjectType::JavaScript => vec!["tests", "test", "__tests__"],
        ProjectType::Java => vec!["src/test/java", "tests", "test"],
        ProjectType::Unknown => vec!["tests", "test"],
    };

    for candidate in candidates {
        let path = root.join(candidate);
        if path.exists() && path.is_dir() {
            return Some(path);
        }
    }
    None
}

/// Count source files by extension under a directory (non-recursive for speed)
/// Uses a simple recursive walk limited to common source extensions.
pub fn count_source_files(root: &Path) -> HashMap<String, usize> {
    let source_extensions = [
        "rs", "go", "py", "ts", "tsx", "js", "jsx", "java", "kt",
        "c", "cpp", "h", "hpp", "cs", "rb", "swift", "zig",
    ];

    let mut counts: HashMap<String, usize> = HashMap::new();
    count_files_recursive(root, &source_extensions, &mut counts, 0, 10);
    counts
}

fn count_files_recursive(
    dir: &Path,
    extensions: &[&str],
    counts: &mut HashMap<String, usize>,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden directories and common non-source dirs
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "vendor" || name == "dist" || name == "build" {
                continue;
            }
        }

        if path.is_dir() {
            count_files_recursive(&path, extensions, counts, depth + 1, max_depth);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext) {
                *counts.entry(ext.to_string()).or_insert(0) += 1;
            }
        }
    }
}

/// Detect git information from a directory
pub fn detect_git_info(root: &Path) -> Option<GitInfo> {
    let git_entry = root.join(".git");
    if !git_entry.exists() {
        return None;
    }

    // Handle both regular .git directory and .git file (worktrees/submodules)
    // A .git file contains "gitdir: /path/to/actual/git/dir"
    let git_dir = if git_entry.is_file() {
        if let Ok(content) = std::fs::read_to_string(&git_entry) {
            if let Some(dir_path) = content.trim().strip_prefix("gitdir: ") {
                let p = PathBuf::from(dir_path.trim());
                if p.is_absolute() {
                    p
                } else {
                    root.join(p)
                }
            } else {
                return None;
            }
        } else {
            return None;
        }
    } else {
        git_entry
    };

    let mut info = GitInfo::default();

    // Read current branch from .git/HEAD
    let head_path = git_dir.join("HEAD");
    if let Ok(content) = std::fs::read_to_string(&head_path) {
        let trimmed = content.trim();
        if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
            info.branch = Some(branch.to_string());
        } else {
            // Detached HEAD — show short hash
            info.branch = Some(trimmed.chars().take(8).collect());
        }
    }

    // Read remote URL from .git/config
    let config_path = git_dir.join("config");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        // Simple parse: find url = ... under [remote "origin"]
        let mut in_origin = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[remote \"origin\"]" {
                in_origin = true;
            } else if trimmed.starts_with('[') {
                in_origin = false;
            } else if in_origin {
                if let Some(url) = trimmed.strip_prefix("url = ") {
                    info.remote = Some(url.trim().to_string());
                    break;
                }
            }
        }
    }

    Some(info)
}

/// Find CLAUDE.md in the workspace
pub fn find_claude_md(root: &Path) -> Option<PathBuf> {
    let path = root.join("CLAUDE.md");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Find .claude/ directory in the workspace
pub fn find_claude_dir(root: &Path) -> Option<PathBuf> {
    let path = root.join(".claude");
    if path.exists() && path.is_dir() {
        Some(path)
    } else {
        None
    }
}

/// Discover full workspace context from a root directory
pub fn discover(root: &Path) -> WorkspaceContext {
    let project_type = detect_project_type(root);
    let source_root = find_source_root(root, &project_type);
    let test_root = find_test_root(root, &project_type);
    let file_counts = count_source_files(root);
    let git_info = detect_git_info(root);
    let claude_md = find_claude_md(root);
    let claude_dir = find_claude_dir(root);

    WorkspaceContext {
        root: root.to_path_buf(),
        project_type,
        source_root,
        test_root,
        file_counts,
        git_info,
        claude_md,
        claude_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_rust_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "pub fn hello() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join("tests/test.rs"), "#[test] fn t() {}").unwrap();
        dir
    }

    fn setup_go_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module test").unwrap();
        std::fs::create_dir_all(dir.path().join("cmd")).unwrap();
        std::fs::write(dir.path().join("cmd/main.go"), "package main").unwrap();
        dir
    }

    fn setup_python_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[project]\nname = \"test\"").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/app.py"), "def main(): pass").unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join("tests/test_app.py"), "def test_app(): pass").unwrap();
        dir
    }

    fn setup_ts_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export {};").unwrap();
        dir
    }

    #[test]
    fn test_detect_rust_project() {
        let dir = setup_rust_project();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Rust);
    }

    #[test]
    fn test_detect_go_project() {
        let dir = setup_go_project();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Go);
    }

    #[test]
    fn test_detect_python_project() {
        let dir = setup_python_project();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Python);
    }

    #[test]
    fn test_detect_typescript_project() {
        let dir = setup_ts_project();
        assert_eq!(detect_project_type(dir.path()), ProjectType::TypeScript);
    }

    #[test]
    fn test_detect_java_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("pom.xml"), "<project/>").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Java);
    }

    #[test]
    fn test_detect_unknown_project() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Unknown);
    }

    #[test]
    fn test_find_source_root_rust() {
        let dir = setup_rust_project();
        let src = find_source_root(dir.path(), &ProjectType::Rust);
        assert!(src.is_some());
        assert!(src.unwrap().ends_with("src"));
    }

    #[test]
    fn test_find_test_root_rust() {
        let dir = setup_rust_project();
        let tests = find_test_root(dir.path(), &ProjectType::Rust);
        assert!(tests.is_some());
        assert!(tests.unwrap().ends_with("tests"));
    }

    #[test]
    fn test_count_source_files() {
        let dir = setup_rust_project();
        let counts = count_source_files(dir.path());
        assert!(counts.get("rs").unwrap_or(&0) >= &2);
    }

    #[test]
    fn test_count_source_files_skips_hidden() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        std::fs::write(dir.path().join(".hidden/secret.rs"), "").unwrap();
        std::fs::write(dir.path().join("visible.rs"), "").unwrap();
        let counts = count_source_files(dir.path());
        assert_eq!(*counts.get("rs").unwrap_or(&0), 1);
    }

    #[test]
    fn test_git_info_no_repo() {
        let dir = TempDir::new().unwrap();
        assert!(detect_git_info(dir.path()).is_none());
    }

    #[test]
    fn test_git_info_with_repo() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let info = detect_git_info(dir.path());
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.branch, Some("main".to_string()));
    }

    #[test]
    fn test_git_info_detached_head() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/HEAD"), "abc123def456\n").unwrap();
        let info = detect_git_info(dir.path()).unwrap();
        assert_eq!(info.branch, Some("abc123de".to_string()));
    }

    #[test]
    fn test_git_info_remote_url() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            dir.path().join(".git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/user/repo.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
        ).unwrap();
        let info = detect_git_info(dir.path()).unwrap();
        assert_eq!(info.remote, Some("https://github.com/user/repo.git".to_string()));
    }

    #[test]
    fn test_find_claude_md() {
        let dir = TempDir::new().unwrap();
        assert!(find_claude_md(dir.path()).is_none());
        std::fs::write(dir.path().join("CLAUDE.md"), "# Instructions").unwrap();
        assert!(find_claude_md(dir.path()).is_some());
    }

    #[test]
    fn test_find_claude_dir() {
        let dir = TempDir::new().unwrap();
        assert!(find_claude_dir(dir.path()).is_none());
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        assert!(find_claude_dir(dir.path()).is_some());
    }

    #[test]
    fn test_discover_full_context() {
        let dir = setup_rust_project();
        let ctx = discover(dir.path());
        assert_eq!(ctx.project_type, ProjectType::Rust);
        assert!(ctx.source_root.is_some());
        assert!(ctx.test_root.is_some());
        assert!(ctx.total_source_files() > 0);
    }

    #[test]
    fn test_workspace_summary() {
        let dir = setup_rust_project();
        let ctx = discover(dir.path());
        let summary = ctx.summary();
        assert!(summary.contains("Rust"));
        assert!(summary.contains("Source root"));
    }

    #[test]
    fn test_project_type_name() {
        assert_eq!(ProjectType::Rust.name(), "Rust");
        assert_eq!(ProjectType::Go.name(), "Go");
        assert_eq!(ProjectType::Python.name(), "Python");
    }

    #[test]
    fn test_project_type_extension() {
        assert_eq!(ProjectType::Rust.extension(), "rs");
        assert_eq!(ProjectType::Go.extension(), "go");
        assert_eq!(ProjectType::TypeScript.extension(), "ts");
    }

    #[test]
    fn test_detect_js_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::JavaScript);
    }

    #[test]
    fn test_detect_python_requirements() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "flask\n").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Python);
    }

    #[test]
    fn test_git_info_worktree_file() {
        // When .git is a file (worktree/submodule), it should follow the gitdir pointer
        let dir = TempDir::new().unwrap();
        let actual_git_dir = dir.path().join("actual_git");
        std::fs::create_dir_all(&actual_git_dir).unwrap();
        std::fs::write(actual_git_dir.join("HEAD"), "ref: refs/heads/feature\n").unwrap();

        // Create .git file pointing to actual git dir
        let worktree = dir.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}", actual_git_dir.display()),
        ).unwrap();

        let info = detect_git_info(&worktree);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.branch, Some("feature".to_string()));
    }
}
