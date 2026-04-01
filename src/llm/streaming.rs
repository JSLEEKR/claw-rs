//! SSE (Server-Sent Events) stream parser for LLM API responses

use super::types::*;
use super::LlmError;

/// Parser for Server-Sent Events streams from the Anthropic API
pub struct SseParser {
    buffer: String,
    current_event_type: Option<String>,
}

impl SseParser {
    /// Create a new SSE parser
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event_type: None,
        }
    }

    /// Feed a chunk of data and return any complete events
    pub fn feed(&mut self, chunk: &str) -> Vec<Result<StreamEvent, LlmError>> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();

        while let Some(pos) = self.buffer.find("\n\n") {
            let block = self.buffer[..pos].to_string();
            self.buffer = self.buffer[pos + 2..].to_string();

            let mut event_type: Option<String> = None;
            let mut data_parts: Vec<String> = Vec::new();

            for line in block.lines() {
                if line.starts_with("event: ") {
                    event_type = Some(line[7..].trim().to_string());
                } else if line.starts_with("data: ") {
                    data_parts.push(line[6..].to_string());
                }
            }

            // SSE spec: multiple data lines are joined with newlines
            if let Some(ref et) = event_type {
                let data = data_parts.join("\n");
                let trimmed = data.trim();
                if !trimmed.is_empty() {
                    if let Some(result) = self.parse_event(et, trimmed) {
                        events.push(result);
                    }
                }
            }
            // Carry forward the event type for the next block if no data was found
            // (per SSE spec, event type resets after dispatch; we reset here)
            self.current_event_type = None;
        }

        events
    }

    /// Parse a single SSE event
    fn parse_event(&self, event_type: &str, data: &str) -> Option<Result<StreamEvent, LlmError>> {
        match event_type {
            "message_start" => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
                match parsed {
                    Ok(v) => {
                        let msg = &v["message"];
                        Some(Ok(StreamEvent::MessageStart {
                            id: msg["id"].as_str().unwrap_or("").to_string(),
                            model: msg["model"].as_str().unwrap_or("").to_string(),
                        }))
                    }
                    Err(e) => Some(Err(LlmError::Parse(e.to_string()))),
                }
            }
            "content_block_start" => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
                match parsed {
                    Ok(v) => {
                        let index = v["index"].as_u64().unwrap_or(0) as usize;
                        let cb = &v["content_block"];
                        let block_type = cb["type"].as_str().unwrap_or("text");

                        let content_block = match block_type {
                            "tool_use" => ContentBlock::ToolUse {
                                id: cb["id"].as_str().unwrap_or("").to_string(),
                                name: cb["name"].as_str().unwrap_or("").to_string(),
                                input: serde_json::Value::Object(serde_json::Map::new()),
                            },
                            _ => ContentBlock::Text {
                                text: String::new(),
                            },
                        };
                        Some(Ok(StreamEvent::ContentBlockStart {
                            index,
                            content_block,
                        }))
                    }
                    Err(e) => Some(Err(LlmError::Parse(e.to_string()))),
                }
            }
            "content_block_delta" => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
                match parsed {
                    Ok(v) => {
                        let index = v["index"].as_u64().unwrap_or(0) as usize;
                        let delta = &v["delta"];
                        let delta_type = delta["type"].as_str().unwrap_or("");

                        let text = match delta_type {
                            "text_delta" => delta["text"].as_str().unwrap_or("").to_string(),
                            "input_json_delta" => {
                                delta["partial_json"].as_str().unwrap_or("").to_string()
                            }
                            _ => String::new(),
                        };

                        Some(Ok(StreamEvent::ContentBlockDelta { index, text }))
                    }
                    Err(e) => Some(Err(LlmError::Parse(e.to_string()))),
                }
            }
            "content_block_stop" => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
                match parsed {
                    Ok(v) => {
                        let index = v["index"].as_u64().unwrap_or(0) as usize;
                        Some(Ok(StreamEvent::ContentBlockStop { index }))
                    }
                    Err(e) => Some(Err(LlmError::Parse(e.to_string()))),
                }
            }
            "message_delta" => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
                match parsed {
                    Ok(v) => {
                        let stop_reason = v["delta"]["stop_reason"]
                            .as_str()
                            .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok());

                        let usage = Usage {
                            input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize,
                            output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0)
                                as usize,
                        };

                        Some(Ok(StreamEvent::MessageDelta { stop_reason, usage }))
                    }
                    Err(e) => Some(Err(LlmError::Parse(e.to_string()))),
                }
            }
            "message_stop" => Some(Ok(StreamEvent::MessageStop)),
            "ping" => Some(Ok(StreamEvent::Ping)),
            _ => None,
        }
    }
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_delta() {
        let mut parser = SseParser::new();
        let chunk = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 1);
        match events[0].as_ref().unwrap() {
            StreamEvent::ContentBlockDelta { index, text } => {
                assert_eq!(*index, 0);
                assert_eq!(text, "Hello");
            }
            _ => panic!("Expected ContentBlockDelta"),
        }
    }

    #[test]
    fn test_parse_message_start() {
        let mut parser = SseParser::new();
        let chunk = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 1);
        match events[0].as_ref().unwrap() {
            StreamEvent::MessageStart { id, model } => {
                assert_eq!(id, "msg_123");
                assert_eq!(model, "claude-3");
            }
            _ => panic!("Expected MessageStart"),
        }
    }

    #[test]
    fn test_parse_message_stop() {
        let mut parser = SseParser::new();
        let chunk = "event: message_stop\ndata: {}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].as_ref().unwrap(), StreamEvent::MessageStop));
    }

    #[test]
    fn test_parse_ping() {
        let mut parser = SseParser::new();
        let chunk = "event: ping\ndata: {}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].as_ref().unwrap(), StreamEvent::Ping));
    }

    #[test]
    fn test_partial_chunks() {
        let mut parser = SseParser::new();
        // Feed partial data
        let events1 = parser.feed("event: ping\n");
        assert!(events1.is_empty());
        // Complete the event
        let events2 = parser.feed("data: {}\n\n");
        assert_eq!(events2.len(), 1);
    }

    #[test]
    fn test_multiple_events() {
        let mut parser = SseParser::new();
        let chunk = "event: ping\ndata: {}\n\nevent: message_stop\ndata: {}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_content_block_start_tool_use() {
        let mut parser = SseParser::new();
        let chunk = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"bash\",\"input\":{}}}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 1);
        match events[0].as_ref().unwrap() {
            StreamEvent::ContentBlockStart { index, content_block } => {
                assert_eq!(*index, 1);
                assert!(content_block.is_tool_use());
            }
            _ => panic!("Expected ContentBlockStart"),
        }
    }

    #[test]
    fn test_message_delta_with_stop_reason() {
        let mut parser = SseParser::new();
        let chunk = "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":100,\"output_tokens\":50}}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 1);
        match events[0].as_ref().unwrap() {
            StreamEvent::MessageDelta { stop_reason, usage } => {
                assert_eq!(*stop_reason, Some(StopReason::EndTurn));
                assert_eq!(usage.output_tokens, 50);
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_empty_feed() {
        let mut parser = SseParser::new();
        let events = parser.feed("");
        assert!(events.is_empty());
    }

    #[test]
    fn test_input_json_delta() {
        let mut parser = SseParser::new();
        let chunk = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\"\"}}\n\n";
        let events = parser.feed(chunk);
        assert_eq!(events.len(), 1);
        match events[0].as_ref().unwrap() {
            StreamEvent::ContentBlockDelta { text, .. } => {
                assert!(text.contains("command"));
            }
            _ => panic!("Expected ContentBlockDelta"),
        }
    }
}
