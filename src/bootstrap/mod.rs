//! Bootstrap pipeline for initializing the agent runtime
//!
//! Phases: platform_detect -> project_scan -> context_build -> prompt_generate

use crate::context::{self, WorkspaceContext};
use std::path::{Path, PathBuf};

/// Detected platform information
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    /// Operating system name
    pub os: String,
    /// Architecture (x86_64, aarch64, etc.)
    pub arch: String,
    /// Default shell
    pub shell: String,
    /// Home directory
    pub home_dir: Option<PathBuf>,
    /// Whether running in CI
    pub is_ci: bool,
}

/// Detect current platform information
pub fn detect_platform() -> PlatformInfo {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    let shell = if cfg!(target_os = "windows") {
        std::env::var("SHELL")
            .or_else(|_| std::env::var("COMSPEC"))
            .unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    };

    let home_dir = dirs::home_dir();

    let is_ci = std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("GITLAB_CI").is_ok()
        || std::env::var("JENKINS_URL").is_ok();

    PlatformInfo {
        os,
        arch,
        shell,
        home_dir,
        is_ci,
    }
}

/// Bootstrap result containing all discovered information
#[derive(Debug, Clone)]
pub struct BootstrapResult {
    /// Platform information
    pub platform: PlatformInfo,
    /// Workspace context (if a project was found)
    pub workspace: Option<WorkspaceContext>,
    /// Generated system prompt
    pub system_prompt: String,
}

/// Phase of the bootstrap pipeline
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapPhase {
    PlatformDetect,
    ProjectScan,
    ContextBuild,
    PromptGenerate,
    Complete,
}

impl BootstrapPhase {
    pub fn name(&self) -> &str {
        match self {
            Self::PlatformDetect => "platform_detect",
            Self::ProjectScan => "project_scan",
            Self::ContextBuild => "context_build",
            Self::PromptGenerate => "prompt_generate",
            Self::Complete => "complete",
        }
    }

    pub fn next(&self) -> Option<BootstrapPhase> {
        match self {
            Self::PlatformDetect => Some(Self::ProjectScan),
            Self::ProjectScan => Some(Self::ContextBuild),
            Self::ContextBuild => Some(Self::PromptGenerate),
            Self::PromptGenerate => Some(Self::Complete),
            Self::Complete => None,
        }
    }
}

/// Scan for a project starting from the given directory,
/// walking up to find a project manifest
pub fn scan_project(start_dir: &Path) -> Option<PathBuf> {
    let manifest_files = [
        "Cargo.toml",
        "go.mod",
        "package.json",
        "pyproject.toml",
        "setup.py",
        "pom.xml",
        "build.gradle",
    ];

    let mut current = start_dir.to_path_buf();
    loop {
        for manifest in &manifest_files {
            if current.join(manifest).exists() {
                return Some(current);
            }
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Generate a system prompt from workspace context and platform info
pub fn generate_system_prompt(
    platform: &PlatformInfo,
    workspace: Option<&WorkspaceContext>,
) -> String {
    let mut parts = Vec::new();

    parts.push("You are an AI assistant with access to tools for file operations, code search, and shell commands.".to_string());

    // Platform context
    parts.push(format!(
        "\nEnvironment: {} ({}) | Shell: {}",
        platform.os, platform.arch, platform.shell
    ));

    if platform.is_ci {
        parts.push("Running in CI environment.".to_string());
    }

    // Workspace context
    if let Some(ctx) = workspace {
        parts.push(format!(
            "\nProject: {} ({})",
            ctx.root.display(),
            ctx.project_type.name()
        ));

        if let Some(ref src) = ctx.source_root {
            parts.push(format!("Source: {}", src.display()));
        }

        let total = ctx.total_source_files();
        if total > 0 {
            parts.push(format!("Files: {} source files", total));
        }

        if let Some(ref git) = ctx.git_info {
            if let Some(ref branch) = git.branch {
                parts.push(format!("Branch: {}", branch));
            }
        }

        if ctx.claude_md.is_some() {
            parts.push("CLAUDE.md found — follow project-specific instructions.".to_string());
        }
    }

    parts.push("\nGuidelines:".to_string());
    parts.push("- Use tools when you need to interact with the filesystem or run commands".to_string());
    parts.push("- Prefer read-only operations before making changes".to_string());
    parts.push("- Explain what you're doing before and after tool use".to_string());
    parts.push("- Handle errors gracefully and report them clearly".to_string());

    parts.join("\n")
}

/// Run the full bootstrap pipeline
pub fn bootstrap(start_dir: &Path) -> BootstrapResult {
    // Phase 1: Platform detection
    let platform = detect_platform();

    // Phase 2: Project scan
    let project_root = scan_project(start_dir);

    // Phase 3: Context build
    let workspace = project_root.map(|root| context::discover(&root));

    // Phase 4: Prompt generation
    let system_prompt = generate_system_prompt(&platform, workspace.as_ref());

    BootstrapResult {
        platform,
        workspace,
        system_prompt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_platform() {
        let info = detect_platform();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(!info.shell.is_empty());
    }

    #[test]
    fn test_platform_os_known() {
        let info = detect_platform();
        let known = ["windows", "linux", "macos"];
        assert!(known.iter().any(|os| info.os == *os));
    }

    #[test]
    fn test_scan_project_finds_cargo() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let found = scan_project(dir.path());
        assert!(found.is_some());
    }

    #[test]
    fn test_scan_project_walks_up() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let sub = dir.path().join("src");
        std::fs::create_dir_all(&sub).unwrap();
        let found = scan_project(&sub);
        assert!(found.is_some());
        // Should find the parent with Cargo.toml
        assert_eq!(found.unwrap(), dir.path());
    }

    #[test]
    fn test_scan_project_not_found() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("empty");
        std::fs::create_dir_all(&sub).unwrap();
        // Will walk up from empty dir; since TempDir is in system temp,
        // it might find something or not — just verify no panic
        let _ = scan_project(&sub);
    }

    #[test]
    fn test_generate_system_prompt_no_workspace() {
        let platform = PlatformInfo {
            os: "linux".into(),
            arch: "x86_64".into(),
            shell: "/bin/bash".into(),
            home_dir: None,
            is_ci: false,
        };
        let prompt = generate_system_prompt(&platform, None);
        assert!(prompt.contains("linux"));
        assert!(prompt.contains("AI assistant"));
    }

    #[test]
    fn test_generate_system_prompt_with_workspace() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let ctx = context::discover(dir.path());
        let platform = detect_platform();
        let prompt = generate_system_prompt(&platform, Some(&ctx));
        assert!(prompt.contains("Rust"));
    }

    #[test]
    fn test_generate_system_prompt_ci() {
        let platform = PlatformInfo {
            os: "linux".into(),
            arch: "x86_64".into(),
            shell: "/bin/bash".into(),
            home_dir: None,
            is_ci: true,
        };
        let prompt = generate_system_prompt(&platform, None);
        assert!(prompt.contains("CI"));
    }

    #[test]
    fn test_bootstrap_phases() {
        assert_eq!(BootstrapPhase::PlatformDetect.name(), "platform_detect");
        assert_eq!(
            BootstrapPhase::PlatformDetect.next(),
            Some(BootstrapPhase::ProjectScan)
        );
        assert_eq!(
            BootstrapPhase::ProjectScan.next(),
            Some(BootstrapPhase::ContextBuild)
        );
        assert_eq!(
            BootstrapPhase::ContextBuild.next(),
            Some(BootstrapPhase::PromptGenerate)
        );
        assert_eq!(
            BootstrapPhase::PromptGenerate.next(),
            Some(BootstrapPhase::Complete)
        );
        assert_eq!(BootstrapPhase::Complete.next(), None);
    }

    #[test]
    fn test_bootstrap_full() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let result = bootstrap(dir.path());
        assert!(!result.platform.os.is_empty());
        assert!(result.workspace.is_some());
        assert!(!result.system_prompt.is_empty());
    }
}
