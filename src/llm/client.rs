//! LLM HTTP client for making API calls

use super::streaming::SseParser;
use super::types::*;
use super::LlmError;
use crate::config::LlmConfig;
use futures::Stream;
use std::pin::Pin;

/// LLM client for communicating with the Anthropic API
pub struct LlmClient {
    http: reqwest::Client,
    config: LlmConfig,
}

impl LlmClient {
    /// Create a new LLM client
    pub fn new(config: LlmConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client");

        Self { http, config }
    }

    /// Send a chat request and get a complete response
    pub async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: Option<&str>,
    ) -> Result<ChatResponse, LlmError> {
        let request = self.build_request(messages, tools, system, false);
        let url = format!("{}/messages", self.config.base_url);

        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = response.status().as_u16();
        if status == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(60);
            return Err(LlmError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status,
                message: body,
            });
        }

        let body = response.text().await.map_err(|e| LlmError::Http(e.to_string()))?;
        serde_json::from_str(&body).map_err(|e| LlmError::Parse(e.to_string()))
    }

    /// Send a chat request and get a streaming response
    pub async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: Option<&str>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError> {
        let request = self.build_request(messages, tools, system, true);
        let url = format!("{}/messages", self.config.base_url);

        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = response.status().as_u16();
        if status == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(60);
            return Err(LlmError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status,
                message: body,
            });
        }

        let byte_stream = response.bytes_stream();
        let stream = SseStream::new(byte_stream);
        Ok(Box::pin(stream))
    }

    /// Build a chat request
    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: Option<&str>,
        stream: bool,
    ) -> ChatRequest {
        let system_prompt = system
            .map(|s| s.to_string())
            .or_else(|| self.config.system_prompt.clone());

        ChatRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            system: system_prompt,
            max_tokens: self.config.max_tokens,
            temperature: Some(self.config.temperature),
            tools: tools.to_vec(),
            stream,
        }
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Get the max tokens setting
    pub fn max_tokens(&self) -> u32 {
        self.config.max_tokens
    }
}

/// Streaming wrapper that converts a byte stream to StreamEvents
struct SseStream {
    inner: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    parser: SseParser,
    pending: Vec<Result<StreamEvent, LlmError>>,
}

impl SseStream {
    fn new(stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static) -> Self {
        Self {
            inner: Box::pin(stream),
            parser: SseParser::new(),
            pending: Vec::new(),
        }
    }
}

impl Stream for SseStream {
    type Item = Result<StreamEvent, LlmError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // Return pending events first
        if !self.pending.is_empty() {
            return std::task::Poll::Ready(Some(self.pending.remove(0)));
        }

        // Poll for more data
        match Pin::new(&mut self.inner).poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(bytes))) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut events = self.parser.feed(&text);

                if events.is_empty() {
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                } else {
                    let first = events.remove(0);
                    self.pending = events;
                    std::task::Poll::Ready(Some(first))
                }
            }
            std::task::Poll::Ready(Some(Err(e))) => {
                std::task::Poll::Ready(Some(Err(LlmError::Stream(e.to_string()))))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = LlmConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.example.com".to_string(),
            model: "test-model".to_string(),
            max_tokens: 1024,
            temperature: 0.5,
            system_prompt: None,
        };
        let client = LlmClient::new(config);
        assert_eq!(client.model(), "test-model");
        assert_eq!(client.max_tokens(), 1024);
    }

    #[test]
    fn test_build_request_no_stream() {
        let config = LlmConfig {
            api_key: "key".to_string(),
            base_url: "https://api.example.com".to_string(),
            model: "claude-3".to_string(),
            max_tokens: 2048,
            temperature: 0.0,
            system_prompt: Some("You are helpful".to_string()),
        };
        let client = LlmClient::new(config);
        let messages = vec![Message::user("hello")];
        let request = client.build_request(&messages, &[], None, false);

        assert_eq!(request.model, "claude-3");
        assert!(!request.stream);
        assert_eq!(request.system, Some("You are helpful".to_string()));
        assert_eq!(request.messages.len(), 1);
    }

    #[test]
    fn test_build_request_with_tools() {
        let config = LlmConfig::default();
        let client = LlmClient::new(LlmConfig {
            api_key: "key".to_string(),
            ..config
        });
        let tools = vec![ToolDefinition {
            name: "bash".to_string(),
            description: "Run commands".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }];
        let request = client.build_request(&[], &tools, Some("system"), true);

        assert!(request.stream);
        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.system, Some("system".to_string()));
    }

    #[test]
    fn test_build_request_system_override() {
        let config = LlmConfig {
            api_key: "key".to_string(),
            system_prompt: Some("default system".to_string()),
            ..LlmConfig::default()
        };
        let client = LlmClient::new(config);
        // Explicit system prompt should override config
        let request = client.build_request(&[], &[], Some("custom system"), false);
        assert_eq!(request.system, Some("custom system".to_string()));
    }

    #[test]
    fn test_chat_request_serialization() {
        let request = ChatRequest {
            model: "claude-3".to_string(),
            messages: vec![Message::user("test")],
            system: Some("system".to_string()),
            max_tokens: 1024,
            temperature: Some(0.5),
            tools: vec![],
            stream: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-3"));
        assert!(json.contains("test"));
        // tools should not appear when empty (skip_serializing_if)
        assert!(!json.contains("tools"));
    }
}
