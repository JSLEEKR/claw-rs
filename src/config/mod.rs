//! Configuration management for claw-rs
//!
//! Loads configuration from YAML files and environment variables.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration error
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing API key: set CLAW_API_KEY or ANTHROPIC_API_KEY environment variable")]
    MissingApiKey,

    #[error("Config file error: {0}")]
    FileError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// LLM API configuration
    pub llm: LlmConfig,

    /// Session configuration
    pub session: SessionConfig,

    /// Permission configuration
    pub permissions: PermissionConfig,

    /// Agent loop configuration
    pub agent: AgentConfig,
}

/// LLM API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// API key (from env or config)
    pub api_key: String,

    /// API base URL
    pub base_url: String,

    /// Model name
    pub model: String,

    /// Maximum tokens for response
    pub max_tokens: u32,

    /// Temperature (0.0 - 1.0)
    pub temperature: f32,

    /// System prompt
    pub system_prompt: Option<String>,
}

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Directory for session storage
    pub storage_dir: PathBuf,

    /// Maximum messages before compaction
    pub compact_after: usize,

    /// Number of messages to keep after compaction
    pub compact_keep_last: usize,
}

/// Permission configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Tools that are always allowed
    pub allow_list: Vec<String>,

    /// Tools that always require confirmation
    pub ask_list: Vec<String>,

    /// Tools that are always denied
    pub deny_list: Vec<String>,

    /// Whether to auto-approve all tool calls (dangerous)
    pub auto_approve: bool,
}

/// Agent loop configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum turns in the agent loop
    pub max_turns: usize,

    /// Maximum total tokens budget
    pub max_budget_tokens: usize,

    /// Whether to stream responses
    pub streaming: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            system_prompt: None,
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        let storage_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claw-rs")
            .join("sessions");
        Self {
            storage_dir,
            compact_after: 20,
            compact_keep_last: 10,
        }
    }
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            allow_list: vec![
                "read".to_string(),
                "glob".to_string(),
                "grep".to_string(),
            ],
            ask_list: vec![
                "bash".to_string(),
                "write".to_string(),
                "edit".to_string(),
            ],
            deny_list: Vec::new(),
            auto_approve: false,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: 30,
            max_budget_tokens: 200_000,
            streaming: true,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            session: SessionConfig::default(),
            permissions: PermissionConfig::default(),
            agent: AgentConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration from file, environment, and defaults
    pub fn load(config_path: Option<&Path>) -> Result<Self, ConfigError> {
        let mut config = if let Some(path) = config_path {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| ConfigError::FileError(e.to_string()))?;
                serde_yaml::from_str(&content)
                    .map_err(|e| ConfigError::ParseError(e.to_string()))?
            } else {
                AppConfig::default()
            }
        } else {
            // Try default config location
            let default_path = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claw-rs")
                .join("config.yaml");
            if default_path.exists() {
                let content = std::fs::read_to_string(&default_path)
                    .map_err(|e| ConfigError::FileError(e.to_string()))?;
                serde_yaml::from_str(&content)
                    .map_err(|e| ConfigError::ParseError(e.to_string()))?
            } else {
                AppConfig::default()
            }
        };

        // Override with environment variables
        config.apply_env_overrides();

        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(key) = std::env::var("CLAW_API_KEY") {
            self.llm.api_key = key;
        } else if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            self.llm.api_key = key;
        }

        if let Ok(url) = std::env::var("CLAW_BASE_URL") {
            self.llm.base_url = url;
        }

        if let Ok(model) = std::env::var("CLAW_MODEL") {
            self.llm.model = model;
        }

        if let Ok(tokens) = std::env::var("CLAW_MAX_TOKENS") {
            if let Ok(n) = tokens.parse() {
                self.llm.max_tokens = n;
            }
        }

        if let Ok(turns) = std::env::var("CLAW_MAX_TURNS") {
            if let Ok(n) = turns.parse() {
                self.agent.max_turns = n;
            }
        }
    }

    /// Validate that required fields are set
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.llm.api_key.is_empty() {
            return Err(ConfigError::MissingApiKey);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.llm.max_tokens, 4096);
        assert_eq!(config.agent.max_turns, 30);
        assert_eq!(config.session.compact_after, 20);
        assert!(!config.permissions.auto_approve);
    }

    #[test]
    fn test_validate_missing_api_key() {
        let config = AppConfig::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_with_api_key() {
        let mut config = AppConfig::default();
        config.llm.api_key = "test-key".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let config = AppConfig::load(Some(Path::new("/nonexistent/config.yaml")));
        assert!(config.is_ok()); // Falls back to default
    }

    #[test]
    fn test_yaml_roundtrip() {
        let config = AppConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.llm.max_tokens, config.llm.max_tokens);
    }

    #[test]
    fn test_default_permission_lists() {
        let config = PermissionConfig::default();
        assert!(config.allow_list.contains(&"read".to_string()));
        assert!(config.ask_list.contains(&"bash".to_string()));
        assert!(config.deny_list.is_empty());
    }

    #[test]
    fn test_llm_config_defaults() {
        let config = LlmConfig::default();
        assert!(config.base_url.contains("anthropic"));
        assert!(config.model.contains("claude"));
        assert_eq!(config.temperature, 0.0);
    }

    #[test]
    fn test_session_config_default_dir() {
        let config = SessionConfig::default();
        let path_str = config.storage_dir.to_string_lossy();
        assert!(path_str.contains(".claw-rs") || path_str.contains("sessions"));
    }
}
