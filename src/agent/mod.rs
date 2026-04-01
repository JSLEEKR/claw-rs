//! Agent loop — the core of the runtime
//!
//! Orchestrates the LLM conversation with tool execution.

use crate::config::AppConfig;
use crate::llm::{self, ContentBlock, Message, StopReason, Usage};
use crate::permissions::{PermissionDecision, PermissionManager};
use crate::session::Session;
use crate::tools::{ToolContext, ToolRegistry, ToolResult};

/// Agent loop state
pub struct Agent {
    /// LLM client
    llm_client: llm::LlmClient,
    /// Tool registry
    tools: ToolRegistry,
    /// Permission manager
    permissions: PermissionManager,
    /// Current session
    session: Session,
    /// System prompt
    system_prompt: Option<String>,
    /// Maximum turns
    max_turns: usize,
    /// Maximum token budget
    max_budget_tokens: usize,
    /// Tool execution context
    tool_ctx: ToolContext,
    /// Cumulative usage
    total_usage: Usage,
    /// Callback for permission prompts
    permission_callback: Box<dyn Fn(&str) -> bool + Send + Sync>,
    /// Callback for printing text output
    output_callback: Box<dyn Fn(&str) + Send + Sync>,
}

/// Result of a single agent turn
#[derive(Debug, Clone)]
pub struct TurnResult {
    /// Text output from the assistant
    pub text: String,
    /// Tools that were called this turn
    pub tool_calls: Vec<ToolCall>,
    /// Stop reason
    pub stop_reason: TurnStopReason,
    /// Usage for this turn
    pub usage: Usage,
}

/// A record of a tool call during a turn
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Tool use ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Input parameters
    pub input: serde_json::Value,
    /// Result
    pub result: ToolResult,
}

/// Why the turn loop stopped
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnStopReason {
    /// Natural end of conversation
    EndTurn,
    /// Max turns reached
    MaxTurns,
    /// Token budget exceeded
    BudgetExceeded,
    /// Error occurred
    Error(String),
}

impl Agent {
    /// Create a new agent
    pub fn new(config: &AppConfig) -> Self {
        let llm_config = config.llm.clone();
        let llm_client = llm::LlmClient::new(llm_config);
        let tools = ToolRegistry::with_defaults();
        let permissions = PermissionManager::new(&config.permissions);
        let cwd = std::env::current_dir().unwrap_or_default();
        let session = Session::new(&config.llm.model, &cwd.to_string_lossy());

        Self {
            llm_client,
            tools,
            permissions,
            session,
            system_prompt: config.llm.system_prompt.clone(),
            max_turns: config.agent.max_turns,
            max_budget_tokens: config.agent.max_budget_tokens,
            tool_ctx: ToolContext {
                cwd,
                auto_approve: config.permissions.auto_approve,
            },
            total_usage: Usage::default(),
            permission_callback: Box::new(|_| true),
            output_callback: Box::new(|text| print!("{}", text)),
        }
    }

    /// Create an agent with custom components (for testing)
    pub fn with_components(
        llm_client: llm::LlmClient,
        tools: ToolRegistry,
        permissions: PermissionManager,
        session: Session,
        max_turns: usize,
        max_budget_tokens: usize,
    ) -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        Self {
            llm_client,
            tools,
            permissions,
            session,
            system_prompt: None,
            max_turns,
            max_budget_tokens,
            tool_ctx: ToolContext {
                cwd,
                auto_approve: true,
            },
            total_usage: Usage::default(),
            permission_callback: Box::new(|_| true),
            output_callback: Box::new(|_| {}),
        }
    }

    /// Set the permission callback
    pub fn set_permission_callback<F>(&mut self, callback: F)
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        self.permission_callback = Box::new(callback);
    }

    /// Set the output callback
    pub fn set_output_callback<F>(&mut self, callback: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.output_callback = Box::new(callback);
    }

    /// Process a user message through the agent loop
    ///
    /// This is the core algorithm:
    /// 1. Add user message to session
    /// 2. Send messages to LLM with tool definitions
    /// 3. Parse response:
    ///    - Text only -> return
    ///    - Tool use -> check permissions -> execute -> add result -> loop
    /// 4. Repeat until no tool calls or max turns reached
    pub async fn process_message(&mut self, user_input: &str) -> TurnResult {
        // Add user message
        self.session.add_message(Message::user(user_input));

        let mut all_text = String::new();
        let mut all_tool_calls = Vec::new();
        let mut turn_count = 0;

        loop {
            turn_count += 1;

            // Check turn limit
            if turn_count > self.max_turns {
                return TurnResult {
                    text: all_text,
                    tool_calls: all_tool_calls,
                    stop_reason: TurnStopReason::MaxTurns,
                    usage: self.total_usage.clone(),
                };
            }

            // Check budget
            if self.total_usage.total() > self.max_budget_tokens {
                return TurnResult {
                    text: all_text,
                    tool_calls: all_tool_calls,
                    stop_reason: TurnStopReason::BudgetExceeded,
                    usage: self.total_usage.clone(),
                };
            }

            // Get tool definitions
            let tool_defs = self.tools.definitions();

            // Call LLM
            let response = match self
                .llm_client
                .chat(
                    &self.session.messages,
                    &tool_defs,
                    self.system_prompt.as_deref(),
                )
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    return TurnResult {
                        text: all_text,
                        tool_calls: all_tool_calls,
                        stop_reason: TurnStopReason::Error(e.to_string()),
                        usage: self.total_usage.clone(),
                    };
                }
            };

            // Update usage
            self.total_usage.add(&response.usage);
            self.session.update_usage(&response.usage);

            // Process content blocks
            let mut has_tool_use = false;
            let mut assistant_blocks = Vec::new();
            let mut tool_results = Vec::new();

            for block in &response.content {
                match block {
                    ContentBlock::Text { text } => {
                        all_text.push_str(text);
                        (self.output_callback)(text);
                        assistant_blocks.push(block.clone());
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        has_tool_use = true;
                        assistant_blocks.push(block.clone());

                        // Check permissions
                        let tool = self.tools.get(name);
                        let permission_level = tool
                            .map(|t| t.permission_level())
                            .unwrap_or(crate::tools::PermissionLevel::Deny);

                        let decision = if name == "bash" {
                            let cmd = input["command"].as_str().unwrap_or("");
                            self.permissions.check_bash(cmd)
                        } else {
                            self.permissions.check_tool(name, permission_level)
                        };

                        let result = match decision {
                            PermissionDecision::Allow => {
                                match self.tools.execute(name, input.clone(), &self.tool_ctx).await {
                                    Ok(r) => r,
                                    Err(e) => ToolResult::error(format!("Tool error: {}", e)),
                                }
                            }
                            PermissionDecision::Ask(reason) => {
                                if (self.permission_callback)(&reason) {
                                    match self
                                        .tools
                                        .execute(name, input.clone(), &self.tool_ctx)
                                        .await
                                    {
                                        Ok(r) => r,
                                        Err(e) => ToolResult::error(format!("Tool error: {}", e)),
                                    }
                                } else {
                                    ToolResult::error(format!("Permission denied: {}", reason))
                                }
                            }
                            PermissionDecision::Deny(reason) => {
                                ToolResult::error(format!("Permission denied: {}", reason))
                            }
                        };

                        all_tool_calls.push(ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                            result: result.clone(),
                        });

                        tool_results.push(ContentBlock::tool_result(
                            id.clone(),
                            result.output,
                            result.is_error,
                        ));
                    }
                    _ => {}
                }
            }

            // Add assistant message to session
            self.session
                .add_message(Message::assistant_tool_use(assistant_blocks));

            // If there were tool uses, add results and continue
            if has_tool_use && !tool_results.is_empty() {
                self.session.add_message(Message::tool_results(tool_results));
                continue;
            }

            // No tool use — we're done
            let stop_reason = match response.stop_reason {
                Some(StopReason::EndTurn) | None => TurnStopReason::EndTurn,
                Some(StopReason::MaxTokens) => TurnStopReason::EndTurn,
                Some(StopReason::ToolUse) => {
                    // Shouldn't happen if has_tool_use is false, but handle gracefully
                    TurnStopReason::EndTurn
                }
                Some(StopReason::StopSequence) => TurnStopReason::EndTurn,
            };

            return TurnResult {
                text: all_text,
                tool_calls: all_tool_calls,
                stop_reason,
                usage: self.total_usage.clone(),
            };
        }
    }

    /// Get the current session
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Get mutable session reference
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Get total usage
    pub fn total_usage(&self) -> &Usage {
        &self.total_usage
    }

    /// Compact the session messages
    pub fn compact(&mut self, keep_last: usize) {
        self.session.compact(keep_last);
    }

    /// Clear the session
    pub fn clear(&mut self) {
        self.session.clear();
        self.total_usage = Usage::default();
    }

    /// Get tool registry
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Get permission manager
    pub fn permissions(&self) -> &PermissionManager {
        &self.permissions
    }

    /// Get mutable permission manager
    pub fn permissions_mut(&mut self) -> &mut PermissionManager {
        &mut self.permissions
    }
}

/// Build the default system prompt for the agent
pub fn default_system_prompt() -> String {
    r#"You are an AI assistant with access to tools for file operations, code search, and shell commands.

Available capabilities:
- Read, write, and edit files
- Search files with glob patterns
- Search file contents with regex (grep)
- Execute shell commands

Guidelines:
- Use tools when you need to interact with the filesystem or run commands
- Prefer read-only operations before making changes
- Explain what you're doing before and after tool use
- Handle errors gracefully and report them clearly
- Be concise and direct in your responses"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_result() {
        let result = TurnResult {
            text: "hello".to_string(),
            tool_calls: vec![],
            stop_reason: TurnStopReason::EndTurn,
            usage: Usage::default(),
        };
        assert_eq!(result.text, "hello");
        assert!(result.tool_calls.is_empty());
        assert_eq!(result.stop_reason, TurnStopReason::EndTurn);
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall {
            id: "tc_1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
            result: ToolResult::success("file1\nfile2"),
        };
        assert_eq!(call.name, "bash");
        assert!(!call.result.is_error);
    }

    #[test]
    fn test_turn_stop_reasons() {
        assert_eq!(TurnStopReason::EndTurn, TurnStopReason::EndTurn);
        assert_ne!(TurnStopReason::EndTurn, TurnStopReason::MaxTurns);
        assert_ne!(TurnStopReason::MaxTurns, TurnStopReason::BudgetExceeded);
    }

    #[test]
    fn test_default_system_prompt() {
        let prompt = default_system_prompt();
        assert!(prompt.contains("AI assistant"));
        assert!(prompt.contains("tools"));
        assert!(!prompt.is_empty());
    }

    #[test]
    fn test_agent_creation() {
        let config = AppConfig {
            llm: crate::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let agent = Agent::new(&config);
        assert_eq!(agent.session().message_count(), 0);
        assert_eq!(agent.total_usage().total(), 0);
        assert_eq!(agent.tools().len(), 10);
    }

    #[test]
    fn test_agent_compact() {
        let config = AppConfig {
            llm: crate::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = Agent::new(&config);
        for i in 0..10 {
            agent.session_mut().add_message(Message::user(format!("msg {}", i)));
        }
        assert_eq!(agent.session().message_count(), 10);
        agent.compact(3);
        assert_eq!(agent.session().message_count(), 3);
    }

    #[test]
    fn test_agent_clear() {
        let config = AppConfig {
            llm: crate::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = Agent::new(&config);
        agent.session_mut().add_message(Message::user("hello"));
        agent.clear();
        assert_eq!(agent.session().message_count(), 0);
    }

    #[test]
    fn test_set_callbacks() {
        let config = AppConfig {
            llm: crate::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = Agent::new(&config);

        // Should not panic
        agent.set_permission_callback(|_| false);
        agent.set_output_callback(|_| {});
    }
}
