//! claw-rs: A working AI agent runtime
//!
//! Inspired by claw-code. Reimplemented from scratch in Rust.
//! For educational purposes only, non-commercial use.

pub mod agent;
pub mod config;
pub mod llm;
pub mod permissions;
pub mod session;
pub mod tools;

/// Top-level error type for claw-rs
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("LLM error: {0}")]
    Llm(#[from] llm::LlmError),

    #[error("Tool error: {0}")]
    Tool(#[from] tools::ToolError),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Session error: {0}")]
    Session(#[from] session::SessionError),

    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
