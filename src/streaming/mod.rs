//! Streaming event system for terminal output
//!
//! Defines event types for real-time rendering of LLM responses
//! and tool execution in the terminal.

use serde::{Deserialize, Serialize};

/// Events emitted during a streaming interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamingEvent {
    /// LLM response started
    MessageStart {
        /// Message ID
        id: String,
        /// Model being used
        model: String,
    },

    /// A slash command was matched from user input
    CommandMatch {
        /// Command name (without slash)
        command: String,
        /// Arguments provided
        args: Vec<String>,
    },

    /// A tool call was identified in the LLM response
    ToolMatch {
        /// Tool use ID
        id: String,
        /// Tool name
        name: String,
        /// Tool input parameters (serialized)
        input_summary: String,
    },

    /// A tool call was denied by the permission system
    PermissionDenial {
        /// Tool name
        tool_name: String,
        /// Reason for denial
        reason: String,
    },

    /// A chunk of text content from the LLM
    MessageDelta {
        /// Text content
        text: String,
    },

    /// LLM response completed
    MessageStop {
        /// Stop reason
        reason: String,
        /// Input tokens used
        input_tokens: usize,
        /// Output tokens used
        output_tokens: usize,
    },
}

impl StreamingEvent {
    /// Get a short label for the event type
    pub fn event_type(&self) -> &str {
        match self {
            Self::MessageStart { .. } => "message_start",
            Self::CommandMatch { .. } => "command_match",
            Self::ToolMatch { .. } => "tool_match",
            Self::PermissionDenial { .. } => "permission_denial",
            Self::MessageDelta { .. } => "message_delta",
            Self::MessageStop { .. } => "message_stop",
        }
    }
}

/// ANSI color codes for terminal output
pub mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";

    /// Check if the terminal likely supports color
    pub fn is_color_supported() -> bool {
        // Check NO_COLOR convention
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }
        // Check TERM
        if let Ok(term) = std::env::var("TERM") {
            if term == "dumb" {
                return false;
            }
        }
        // On Windows, check if ENABLE_VIRTUAL_TERMINAL_PROCESSING might be set
        // For simplicity, assume color is supported on modern terminals
        true
    }
}

/// Renderer that formats streaming events for terminal output
pub struct StreamRenderer {
    /// Whether to use colors
    use_colors: bool,
    /// Whether we are currently in the middle of a message
    in_message: bool,
}

impl StreamRenderer {
    /// Create a new renderer
    pub fn new(use_colors: bool) -> Self {
        Self {
            use_colors,
            in_message: false,
        }
    }

    /// Create a renderer that auto-detects color support
    pub fn auto() -> Self {
        Self::new(colors::is_color_supported())
    }

    /// Render an event to a formatted string
    pub fn render(&mut self, event: &StreamingEvent) -> String {
        match event {
            StreamingEvent::MessageStart { model, .. } => {
                self.in_message = true;
                if self.use_colors {
                    format!("{}{}[{}]{}\n", colors::DIM, colors::CYAN, model, colors::RESET)
                } else {
                    format!("[{}]\n", model)
                }
            }

            StreamingEvent::CommandMatch { command, args } => {
                let args_str = if args.is_empty() {
                    String::new()
                } else {
                    format!(" {}", args.join(" "))
                };
                if self.use_colors {
                    format!(
                        "{}{}> /{}{}{}\n",
                        colors::BOLD, colors::GREEN, command, args_str, colors::RESET
                    )
                } else {
                    format!("> /{}{}\n", command, args_str)
                }
            }

            StreamingEvent::ToolMatch { name, input_summary, .. } => {
                if self.use_colors {
                    format!(
                        "{}{}[Tool: {}]{} {}\n",
                        colors::BOLD, colors::BLUE, name, colors::RESET, input_summary
                    )
                } else {
                    format!("[Tool: {}] {}\n", name, input_summary)
                }
            }

            StreamingEvent::PermissionDenial { tool_name, reason } => {
                if self.use_colors {
                    format!(
                        "{}{}[Permission denied: {}]{} {}\n",
                        colors::BOLD, colors::RED, tool_name, colors::RESET, reason
                    )
                } else {
                    format!("[Permission denied: {}] {}\n", tool_name, reason)
                }
            }

            StreamingEvent::MessageDelta { text } => {
                // Raw text, no decoration
                text.clone()
            }

            StreamingEvent::MessageStop {
                reason,
                input_tokens,
                output_tokens,
            } => {
                self.in_message = false;
                if self.use_colors {
                    format!(
                        "\n{}{}[{} | in:{} out:{}]{}\n",
                        colors::DIM,
                        colors::CYAN,
                        reason,
                        input_tokens,
                        output_tokens,
                        colors::RESET
                    )
                } else {
                    format!(
                        "\n[{} | in:{} out:{}]\n",
                        reason, input_tokens, output_tokens
                    )
                }
            }
        }
    }

    /// Check if currently in a message
    pub fn is_in_message(&self) -> bool {
        self.in_message
    }

    /// Check if colors are enabled
    pub fn uses_colors(&self) -> bool {
        self.use_colors
    }
}

impl Default for StreamRenderer {
    fn default() -> Self {
        Self::auto()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_types() {
        let events = vec![
            StreamingEvent::MessageStart { id: "1".into(), model: "sonnet".into() },
            StreamingEvent::CommandMatch { command: "help".into(), args: vec![] },
            StreamingEvent::ToolMatch { id: "t1".into(), name: "bash".into(), input_summary: "ls".into() },
            StreamingEvent::PermissionDenial { tool_name: "bash".into(), reason: "denied".into() },
            StreamingEvent::MessageDelta { text: "hello".into() },
            StreamingEvent::MessageStop { reason: "end_turn".into(), input_tokens: 100, output_tokens: 50 },
        ];

        let types: Vec<&str> = events.iter().map(|e| e.event_type()).collect();
        assert_eq!(types, vec![
            "message_start", "command_match", "tool_match",
            "permission_denial", "message_delta", "message_stop"
        ]);
    }

    #[test]
    fn test_renderer_no_color() {
        let mut renderer = StreamRenderer::new(false);
        let output = renderer.render(&StreamingEvent::MessageStart {
            id: "1".into(),
            model: "sonnet".into(),
        });
        assert_eq!(output, "[sonnet]\n");
        assert!(renderer.is_in_message());
    }

    #[test]
    fn test_renderer_message_stop() {
        let mut renderer = StreamRenderer::new(false);
        renderer.render(&StreamingEvent::MessageStart {
            id: "1".into(),
            model: "sonnet".into(),
        });
        assert!(renderer.is_in_message());

        let output = renderer.render(&StreamingEvent::MessageStop {
            reason: "end_turn".into(),
            input_tokens: 100,
            output_tokens: 50,
        });
        assert!(!renderer.is_in_message());
        assert!(output.contains("end_turn"));
        assert!(output.contains("100"));
        assert!(output.contains("50"));
    }

    #[test]
    fn test_renderer_tool_match() {
        let mut renderer = StreamRenderer::new(false);
        let output = renderer.render(&StreamingEvent::ToolMatch {
            id: "t1".into(),
            name: "bash".into(),
            input_summary: "ls -la".into(),
        });
        assert_eq!(output, "[Tool: bash] ls -la\n");
    }

    #[test]
    fn test_renderer_permission_denial() {
        let mut renderer = StreamRenderer::new(false);
        let output = renderer.render(&StreamingEvent::PermissionDenial {
            tool_name: "bash".into(),
            reason: "destructive command".into(),
        });
        assert!(output.contains("Permission denied"));
        assert!(output.contains("bash"));
        assert!(output.contains("destructive command"));
    }

    #[test]
    fn test_renderer_command_match_no_args() {
        let mut renderer = StreamRenderer::new(false);
        let output = renderer.render(&StreamingEvent::CommandMatch {
            command: "help".into(),
            args: vec![],
        });
        assert_eq!(output, "> /help\n");
    }

    #[test]
    fn test_renderer_command_match_with_args() {
        let mut renderer = StreamRenderer::new(false);
        let output = renderer.render(&StreamingEvent::CommandMatch {
            command: "model".into(),
            args: vec!["opus".into()],
        });
        assert_eq!(output, "> /model opus\n");
    }

    #[test]
    fn test_renderer_message_delta() {
        let mut renderer = StreamRenderer::new(false);
        let output = renderer.render(&StreamingEvent::MessageDelta {
            text: "Hello, world!".into(),
        });
        assert_eq!(output, "Hello, world!");
    }

    #[test]
    fn test_renderer_with_color() {
        let mut renderer = StreamRenderer::new(true);
        let output = renderer.render(&StreamingEvent::ToolMatch {
            id: "t1".into(),
            name: "bash".into(),
            input_summary: "ls".into(),
        });
        assert!(output.contains(colors::BLUE));
        assert!(output.contains(colors::RESET));
        assert!(output.contains("bash"));
    }

    #[test]
    fn test_renderer_uses_colors() {
        let renderer = StreamRenderer::new(true);
        assert!(renderer.uses_colors());
        let renderer = StreamRenderer::new(false);
        assert!(!renderer.uses_colors());
    }

    #[test]
    fn test_streaming_event_serde() {
        let event = StreamingEvent::MessageDelta { text: "hello".into() };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: StreamingEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type(), "message_delta");
    }

    #[test]
    fn test_color_constants() {
        assert!(!colors::RESET.is_empty());
        assert!(!colors::BOLD.is_empty());
        assert!(!colors::RED.is_empty());
        assert!(!colors::GREEN.is_empty());
        assert!(!colors::BLUE.is_empty());
    }
}
