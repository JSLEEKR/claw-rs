//! Permission system for tool execution
//!
//! Three-level permission model: Allow, Ask, Deny

use crate::config::PermissionConfig;
use crate::tools::PermissionLevel;
use std::collections::HashSet;

/// Permission manager that controls tool access
pub struct PermissionManager {
    /// Tools always allowed
    allow_set: HashSet<String>,
    /// Tools requiring confirmation
    ask_set: HashSet<String>,
    /// Tools always denied
    deny_set: HashSet<String>,
    /// Whether to auto-approve all requests
    auto_approve: bool,
    /// Patterns for destructive bash commands
    destructive_patterns: Vec<String>,
}

/// Result of a permission check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Allowed to execute
    Allow,
    /// Requires user confirmation with reason
    Ask(String),
    /// Denied with reason
    Deny(String),
}

impl PermissionManager {
    /// Create a new permission manager from config
    pub fn new(config: &PermissionConfig) -> Self {
        Self {
            allow_set: config.allow_list.iter().map(|s| s.to_lowercase()).collect(),
            ask_set: config.ask_list.iter().map(|s| s.to_lowercase()).collect(),
            deny_set: config.deny_list.iter().map(|s| s.to_lowercase()).collect(),
            auto_approve: config.auto_approve,
            destructive_patterns: vec![
                "rm -rf".to_string(),
                "rm -r /".to_string(),
                "git push --force".to_string(),
                "git push -f".to_string(),
                "git reset --hard".to_string(),
                "git clean -f".to_string(),
                "DROP TABLE".to_string(),
                "DROP DATABASE".to_string(),
                "TRUNCATE ".to_string(),
                "shutdown".to_string(),
                "reboot".to_string(),
                "format ".to_string(),
                "mkfs.".to_string(),
            ],
        }
    }

    /// Create a permissive manager that allows everything (for testing)
    pub fn allow_all() -> Self {
        Self {
            allow_set: HashSet::new(),
            ask_set: HashSet::new(),
            deny_set: HashSet::new(),
            auto_approve: true,
            destructive_patterns: Vec::new(),
        }
    }

    /// Check permission for a tool
    pub fn check_tool(&self, tool_name: &str, default_level: PermissionLevel) -> PermissionDecision {
        let name_lower = tool_name.to_lowercase();

        // Explicit deny list takes priority
        if self.deny_set.contains(&name_lower) {
            return PermissionDecision::Deny(format!("Tool '{}' is in the deny list", tool_name));
        }

        // Explicit allow list
        if self.allow_set.contains(&name_lower) {
            return PermissionDecision::Allow;
        }

        // Explicit ask list
        if self.ask_set.contains(&name_lower) {
            if self.auto_approve {
                return PermissionDecision::Allow;
            }
            return PermissionDecision::Ask(format!(
                "Tool '{}' requires confirmation",
                tool_name
            ));
        }

        // Fall back to tool's default permission level
        match default_level {
            PermissionLevel::Allow => PermissionDecision::Allow,
            PermissionLevel::Ask => {
                if self.auto_approve {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Ask(format!(
                        "Tool '{}' requires confirmation",
                        tool_name
                    ))
                }
            }
            PermissionLevel::Deny => {
                if self.auto_approve && self.deny_set.is_empty() && self.destructive_patterns.is_empty() {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Deny(format!("Tool '{}' is denied by default", tool_name))
                }
            }
        }
    }

    /// Check if a bash command is destructive
    pub fn check_bash_command(&self, command: &str) -> PermissionDecision {
        let lower = command.to_lowercase();

        for pattern in &self.destructive_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                if self.auto_approve {
                    return PermissionDecision::Allow;
                }
                return PermissionDecision::Ask(format!(
                    "Potentially destructive command detected: contains '{}'",
                    pattern
                ));
            }
        }

        PermissionDecision::Allow
    }

    /// Check combined tool + command permissions for bash
    pub fn check_bash(&self, command: &str) -> PermissionDecision {
        // First check if bash tool itself is allowed
        let tool_decision = self.check_tool("bash", PermissionLevel::Ask);
        if let PermissionDecision::Deny(reason) = tool_decision {
            return PermissionDecision::Deny(reason);
        }

        // Then check the specific command
        let cmd_decision = self.check_bash_command(command);
        match (&tool_decision, &cmd_decision) {
            (_, PermissionDecision::Deny(reason)) => PermissionDecision::Deny(reason.clone()),
            (PermissionDecision::Ask(reason), _) => {
                if matches!(cmd_decision, PermissionDecision::Ask(_)) {
                    // Both ask — combine reasons
                    let cmd_reason = match cmd_decision {
                        PermissionDecision::Ask(r) => r,
                        _ => String::new(),
                    };
                    PermissionDecision::Ask(format!("{} | {}", reason, cmd_reason))
                } else {
                    PermissionDecision::Ask(reason.clone())
                }
            }
            (_, PermissionDecision::Ask(reason)) => PermissionDecision::Ask(reason.clone()),
            _ => PermissionDecision::Allow,
        }
    }

    /// Set auto-approve mode
    pub fn set_auto_approve(&mut self, auto: bool) {
        self.auto_approve = auto;
    }

    /// Check if auto-approve is enabled
    pub fn is_auto_approve(&self) -> bool {
        self.auto_approve
    }

    /// Add a tool to the allow list
    pub fn allow_tool(&mut self, name: &str) {
        self.allow_set.insert(name.to_lowercase());
        self.ask_set.remove(&name.to_lowercase());
        self.deny_set.remove(&name.to_lowercase());
    }

    /// Add a tool to the deny list
    pub fn deny_tool(&mut self, name: &str) {
        self.deny_set.insert(name.to_lowercase());
        self.allow_set.remove(&name.to_lowercase());
        self.ask_set.remove(&name.to_lowercase());
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new(&PermissionConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_permissions() {
        let mgr = PermissionManager::default();
        assert_eq!(
            mgr.check_tool("read", PermissionLevel::Allow),
            PermissionDecision::Allow
        );
        assert!(matches!(
            mgr.check_tool("bash", PermissionLevel::Ask),
            PermissionDecision::Ask(_)
        ));
    }

    #[test]
    fn test_allow_list() {
        let config = PermissionConfig {
            allow_list: vec!["bash".to_string()],
            ask_list: vec![],
            deny_list: vec![],
            auto_approve: false,
        };
        let mgr = PermissionManager::new(&config);
        assert_eq!(
            mgr.check_tool("bash", PermissionLevel::Ask),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_deny_list() {
        let config = PermissionConfig {
            allow_list: vec![],
            ask_list: vec![],
            deny_list: vec!["bash".to_string()],
            auto_approve: false,
        };
        let mgr = PermissionManager::new(&config);
        assert!(matches!(
            mgr.check_tool("bash", PermissionLevel::Allow),
            PermissionDecision::Deny(_)
        ));
    }

    #[test]
    fn test_deny_overrides_allow() {
        let config = PermissionConfig {
            allow_list: vec!["bash".to_string()],
            ask_list: vec![],
            deny_list: vec!["bash".to_string()],
            auto_approve: false,
        };
        let mgr = PermissionManager::new(&config);
        assert!(matches!(
            mgr.check_tool("bash", PermissionLevel::Allow),
            PermissionDecision::Deny(_)
        ));
    }

    #[test]
    fn test_auto_approve() {
        let config = PermissionConfig {
            allow_list: vec![],
            ask_list: vec!["bash".to_string()],
            deny_list: vec![],
            auto_approve: true,
        };
        let mgr = PermissionManager::new(&config);
        assert_eq!(
            mgr.check_tool("bash", PermissionLevel::Ask),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_destructive_command_detection() {
        let mgr = PermissionManager::default();
        assert!(matches!(
            mgr.check_bash_command("rm -rf /"),
            PermissionDecision::Ask(_)
        ));
        assert!(matches!(
            mgr.check_bash_command("git push --force"),
            PermissionDecision::Ask(_)
        ));
        assert_eq!(
            mgr.check_bash_command("ls -la"),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_check_bash_combined() {
        let mgr = PermissionManager::default();
        // bash is in ask_list, so always needs confirmation
        assert!(matches!(
            mgr.check_bash("ls -la"),
            PermissionDecision::Ask(_)
        ));
    }

    #[test]
    fn test_allow_all() {
        let mgr = PermissionManager::allow_all();
        assert_eq!(
            mgr.check_tool("anything", PermissionLevel::Deny),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_set_auto_approve() {
        let mut mgr = PermissionManager::default();
        assert!(!mgr.is_auto_approve());
        mgr.set_auto_approve(true);
        assert!(mgr.is_auto_approve());
    }

    #[test]
    fn test_allow_tool_dynamic() {
        let mut mgr = PermissionManager::default();
        // bash starts in ask list
        assert!(matches!(
            mgr.check_tool("bash", PermissionLevel::Ask),
            PermissionDecision::Ask(_)
        ));
        mgr.allow_tool("bash");
        assert_eq!(
            mgr.check_tool("bash", PermissionLevel::Ask),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_deny_tool_dynamic() {
        let mut mgr = PermissionManager::default();
        mgr.deny_tool("read");
        assert!(matches!(
            mgr.check_tool("read", PermissionLevel::Allow),
            PermissionDecision::Deny(_)
        ));
    }

    #[test]
    fn test_case_insensitive_tool_names() {
        let config = PermissionConfig {
            allow_list: vec!["Read".to_string()],
            ask_list: vec![],
            deny_list: vec![],
            auto_approve: false,
        };
        let mgr = PermissionManager::new(&config);
        // "read" should match "Read" in allow list
        assert_eq!(
            mgr.check_tool("read", PermissionLevel::Ask),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_destructive_auto_approve() {
        let mgr = PermissionManager::allow_all();
        assert_eq!(
            mgr.check_bash_command("rm -rf /"),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_safe_command() {
        let mgr = PermissionManager::default();
        assert_eq!(
            mgr.check_bash_command("cargo test"),
            PermissionDecision::Allow
        );
        assert_eq!(
            mgr.check_bash_command("git status"),
            PermissionDecision::Allow
        );
    }
}
