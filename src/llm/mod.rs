//! LLM client module for communicating with AI APIs
//!
//! Supports OpenAI-compatible and Anthropic APIs with streaming.

mod client;
mod streaming;
mod types;

pub use client::LlmClient;
pub use streaming::SseParser;
pub use types::*;

/// LLM-specific errors
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Timeout")]
    Timeout,

    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}
