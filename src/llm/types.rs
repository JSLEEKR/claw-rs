//! Types for LLM API communication
//!
//! Covers both Anthropic Messages API and OpenAI-compatible formats.

use serde::{Deserialize, Serialize};

/// Role in a conversation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A content block in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Plain text content
    #[serde(rename = "text")]
    Text { text: String },

    /// Tool use request from the assistant
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool result from executing a tool
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

impl ContentBlock {
    /// Create a text content block
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create a tool use content block
    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        Self::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    /// Create a tool result content block
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error,
        }
    }

    /// Check if this is a text block
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    /// Check if this is a tool use block
    pub fn is_tool_use(&self) -> bool {
        matches!(self, Self::ToolUse { .. })
    }

    /// Get text content if this is a text block
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }

    /// Estimate token count for this block (simple word-based approximation)
    pub fn estimate_tokens(&self) -> usize {
        match self {
            Self::Text { text } => estimate_token_count(text),
            Self::ToolUse { input, name, .. } => {
                estimate_token_count(name) + estimate_token_count(&input.to_string())
            }
            Self::ToolResult { content, .. } => estimate_token_count(content),
        }
    }
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message role
    pub role: Role,

    /// Content blocks
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Create a user message with text content
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::text(text)],
        }
    }

    /// Create an assistant message with text content
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::text(text)],
        }
    }

    /// Create an assistant message with tool use
    pub fn assistant_tool_use(blocks: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::Assistant,
            content: blocks,
        }
    }

    /// Create a user message with tool results
    pub fn tool_results(results: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::User,
            content: results,
        }
    }

    /// Check if this message contains any tool use blocks
    pub fn has_tool_use(&self) -> bool {
        self.content.iter().any(|c| c.is_tool_use())
    }

    /// Get all tool use blocks from this message
    pub fn tool_uses(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|c| c.is_tool_use()).collect()
    }

    /// Get all text content concatenated
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Estimate token count for this message
    pub fn estimate_tokens(&self) -> usize {
        // Role overhead ~4 tokens
        4 + self.content.iter().map(|c| c.estimate_tokens()).sum::<usize>()
    }
}

/// Tool definition sent to the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,

    /// Tool description
    pub description: String,

    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
}

/// Request to the LLM API
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    /// Model name
    pub model: String,

    /// Messages in the conversation
    pub messages: Vec<Message>,

    /// System prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,

    /// Maximum tokens in the response
    pub max_tokens: u32,

    /// Temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Tool definitions
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,

    /// Whether to stream the response
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}

/// Response from the LLM API
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    /// Response ID
    pub id: String,

    /// Model used
    pub model: String,

    /// Content blocks in the response
    pub content: Vec<ContentBlock>,

    /// Stop reason
    pub stop_reason: Option<StopReason>,

    /// Token usage
    pub usage: Usage,
}

/// Why the model stopped generating
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Natural end of response
    EndTurn,
    /// Max tokens reached
    MaxTokens,
    /// Model wants to use a tool
    ToolUse,
    /// Stopped by stop sequence
    StopSequence,
}

/// Token usage information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens consumed
    pub input_tokens: usize,
    /// Output tokens generated
    pub output_tokens: usize,
}

impl Usage {
    /// Total tokens
    pub fn total(&self) -> usize {
        self.input_tokens + self.output_tokens
    }

    /// Add another usage to this one
    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

/// Streaming event from SSE
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Start of message
    MessageStart {
        id: String,
        model: String,
    },
    /// Text content delta
    ContentBlockDelta {
        index: usize,
        text: String,
    },
    /// Start of a content block
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    /// End of a content block
    ContentBlockStop {
        index: usize,
    },
    /// End of message with usage
    MessageDelta {
        stop_reason: Option<StopReason>,
        usage: Usage,
    },
    /// Message complete
    MessageStop,
    /// Ping / keepalive
    Ping,
}

/// Estimate token count using a simple character-based heuristic.
/// Roughly 1 token per 4 characters for English text.
/// Uses `str::chars().count()` (Unicode scalar count) instead of byte length
/// so that CJK / emoji text is not over-counted.
pub fn estimate_token_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    // ~4 chars per token is a reasonable approximation
    let char_count = text.chars().count();
    (char_count + 3) / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_block_text() {
        let block = ContentBlock::text("hello");
        assert!(block.is_text());
        assert!(!block.is_tool_use());
        assert_eq!(block.as_text(), Some("hello"));
    }

    #[test]
    fn test_content_block_tool_use() {
        let block = ContentBlock::tool_use("id1", "bash", serde_json::json!({"command": "ls"}));
        assert!(block.is_tool_use());
        assert!(!block.is_text());
        assert_eq!(block.as_text(), None);
    }

    #[test]
    fn test_content_block_tool_result() {
        let block = ContentBlock::tool_result("id1", "output", false);
        assert!(!block.is_text());
        assert!(!block.is_tool_use());
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user("hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text_content(), "hello");
        assert!(!msg.has_tool_use());
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("response");
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.text_content(), "response");
    }

    #[test]
    fn test_message_with_tool_use() {
        let blocks = vec![
            ContentBlock::text("Let me check"),
            ContentBlock::tool_use("id1", "bash", serde_json::json!({"command": "ls"})),
        ];
        let msg = Message::assistant_tool_use(blocks);
        assert!(msg.has_tool_use());
        assert_eq!(msg.tool_uses().len(), 1);
    }

    #[test]
    fn test_message_tool_results() {
        let results = vec![
            ContentBlock::tool_result("id1", "file1.rs\nfile2.rs", false),
        ];
        let msg = Message::tool_results(results);
        assert_eq!(msg.role, Role::User);
    }

    #[test]
    fn test_usage() {
        let mut usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
        };
        assert_eq!(usage.total(), 150);

        let other = Usage {
            input_tokens: 200,
            output_tokens: 100,
        };
        usage.add(&other);
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 150);
    }

    #[test]
    fn test_estimate_token_count() {
        assert_eq!(estimate_token_count(""), 0);
        assert!(estimate_token_count("hello world") > 0);
        // ~11 chars -> ~3 tokens
        assert_eq!(estimate_token_count("hello world"), 3);
    }

    #[test]
    fn test_estimate_token_count_multibyte() {
        // Bug fix R5: estimate_token_count used byte length instead of char count.
        // "hello" (5 chars, 5 bytes) + 3 emoji (3 chars, 12 bytes) = 8 chars.
        // Old (byte-based): (17+3)/4 = 5 tokens — over-count.
        // New (char-based): (8+3)/4 = 2 tokens — correct.
        let text = "hello\u{1F600}\u{1F601}\u{1F602}";
        assert_eq!(text.chars().count(), 8);
        assert_eq!(text.len(), 17); // bytes
        assert_eq!(estimate_token_count(text), 2); // (8+3)/4 = 2
    }

    #[test]
    fn test_stop_reason_serde() {
        let json = serde_json::to_string(&StopReason::EndTurn).unwrap();
        assert_eq!(json, "\"end_turn\"");

        let parsed: StopReason = serde_json::from_str("\"tool_use\"").unwrap();
        assert_eq!(parsed, StopReason::ToolUse);
    }

    #[test]
    fn test_content_block_serde() {
        let block = ContentBlock::text("hello");
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"text\""));

        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_text());
    }

    #[test]
    fn test_tool_definition_serde() {
        let tool = ToolDefinition {
            name: "bash".to_string(),
            description: "Execute commands".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("bash"));
    }

    #[test]
    fn test_message_estimate_tokens() {
        let msg = Message::user("hello world");
        let tokens = msg.estimate_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_content_block_estimate_tokens() {
        let text_block = ContentBlock::text("hello world this is a test");
        assert!(text_block.estimate_tokens() > 0);

        let tool_block = ContentBlock::tool_use("id", "bash", serde_json::json!({"cmd": "ls"}));
        assert!(tool_block.estimate_tokens() > 0);
    }

    #[test]
    fn test_role_serde() {
        let json = serde_json::to_string(&Role::User).unwrap();
        assert_eq!(json, "\"user\"");

        let parsed: Role = serde_json::from_str("\"assistant\"").unwrap();
        assert_eq!(parsed, Role::Assistant);
    }
}
