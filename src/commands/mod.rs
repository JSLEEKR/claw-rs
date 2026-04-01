//! Command registry for slash commands
//!
//! Implements real commands like /help, /clear, /compact, /cost, /config,
//! /model, /status, /session, /resume, /history, /diff, /init, /version,
//! /permissions, /export.

use std::collections::HashMap;

/// Result of executing a command
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Output text
    pub output: String,
    /// Whether to continue the REPL loop (false = exit)
    pub continue_loop: bool,
}

impl CommandResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            continue_loop: true,
        }
    }

    pub fn exit(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            continue_loop: false,
        }
    }
}

/// Context passed to command execution
pub struct CommandContext {
    /// Current model name
    pub model: String,
    /// Current working directory
    pub cwd: String,
    /// Session ID
    pub session_id: String,
    /// Message count in current session
    pub message_count: usize,
    /// Turn count
    pub turn_count: usize,
    /// Input tokens used
    pub input_tokens: usize,
    /// Output tokens used
    pub output_tokens: usize,
    /// Auto-approve mode
    pub auto_approve: bool,
    /// Conversation history (for /history and /export)
    pub history: Vec<String>,
    /// Permission allow list
    pub allow_list: Vec<String>,
    /// Permission ask list
    pub ask_list: Vec<String>,
    /// Permission deny list
    pub deny_list: Vec<String>,
}

/// A slash command definition
pub struct Command {
    /// Primary command name (without slash)
    pub name: String,
    /// Aliases (without slash)
    pub aliases: Vec<String>,
    /// Short description
    pub description: String,
    /// Execution function
    handler: Box<dyn Fn(&[&str], &CommandContext) -> CommandResult + Send + Sync>,
}

impl Command {
    /// Create a new command
    pub fn new<F>(
        name: impl Into<String>,
        aliases: Vec<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Self
    where
        F: Fn(&[&str], &CommandContext) -> CommandResult + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            aliases,
            description: description.into(),
            handler: Box::new(handler),
        }
    }

    /// Execute the command
    pub fn execute(&self, args: &[&str], ctx: &CommandContext) -> CommandResult {
        (self.handler)(args, ctx)
    }
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("aliases", &self.aliases)
            .field("description", &self.description)
            .finish()
    }
}

/// Registry of available slash commands
pub struct CommandRegistry {
    commands: Vec<Command>,
    /// Lookup: name/alias -> index in commands vec
    lookup: HashMap<String, usize>,
}

impl CommandRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    /// Register a command
    pub fn register(&mut self, command: Command) {
        let idx = self.commands.len();
        self.lookup.insert(command.name.clone(), idx);
        for alias in &command.aliases {
            self.lookup.insert(alias.clone(), idx);
        }
        self.commands.push(command);
    }

    /// Look up a command by name or alias (without the leading slash)
    pub fn get(&self, name: &str) -> Option<&Command> {
        let name_lower = name.to_lowercase();
        self.lookup.get(&name_lower).map(|&idx| &self.commands[idx])
    }

    /// Execute a command by name with arguments
    pub fn execute(&self, name: &str, args: &[&str], ctx: &CommandContext) -> Option<CommandResult> {
        self.get(name).map(|cmd| cmd.execute(args, ctx))
    }

    /// Parse a slash command string into (command_name, args)
    pub fn parse(input: &str) -> Option<(&str, Vec<&str>)> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let without_slash = &trimmed[1..];
        let mut parts = without_slash.splitn(2, |c: char| c.is_whitespace());
        let name = parts.next()?;
        let args: Vec<&str> = parts
            .next()
            .map(|s| s.split_whitespace().collect())
            .unwrap_or_default();

        Some((name, args))
    }

    /// Get all command names
    pub fn names(&self) -> Vec<&str> {
        self.commands.iter().map(|c| c.name.as_str()).collect()
    }

    /// Number of registered commands
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Create a registry with all default commands
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // /help
        registry.register(Command::new(
            "help",
            vec!["h".into(), "?".into()],
            "Show available commands",
            |_args, _ctx| {
                let help_text = [
                    "Available commands:",
                    "  /help          — Show this help message",
                    "  /clear         — Reset conversation history",
                    "  /compact [n]   — Trim context to last n messages",
                    "  /cost          — Show token usage and cost",
                    "  /config        — Show current configuration",
                    "  /model [name]  — Show or change the model",
                    "  /status        — Show session info",
                    "  /session       — List saved sessions",
                    "  /resume <id>   — Resume a saved session",
                    "  /history       — Show conversation history",
                    "  /diff          — Show git diff",
                    "  /init          — Initialize project (.claude/, CLAUDE.md)",
                    "  /version       — Show version information",
                    "  /permissions   — Show permission settings",
                    "  /export [file] — Export conversation to file",
                    "  /quit          — Exit",
                ].join("\n");
                CommandResult::ok(help_text)
            },
        ));

        // /clear
        registry.register(Command::new(
            "clear",
            vec![],
            "Reset conversation history",
            |_args, _ctx| CommandResult::ok("CLEAR"),
        ));

        // /compact
        registry.register(Command::new(
            "compact",
            vec![],
            "Trim context to last n messages",
            |args, _ctx| {
                let n: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(10);
                CommandResult::ok(format!("COMPACT:{}", n))
            },
        ));

        // /cost
        registry.register(Command::new(
            "cost",
            vec!["usage".into()],
            "Show token usage and cost",
            |_args, ctx| {
                let total = ctx.input_tokens + ctx.output_tokens;
                let output = format!(
                    "Token usage:\n  Input:  {} tokens\n  Output: {} tokens\n  Total:  {} tokens",
                    ctx.input_tokens, ctx.output_tokens, total
                );
                CommandResult::ok(output)
            },
        ));

        // /config
        registry.register(Command::new(
            "config",
            vec!["cfg".into()],
            "Show current configuration",
            |_args, ctx| {
                let output = format!(
                    "Configuration:\n  Model: {}\n  CWD: {}\n  Auto-approve: {}",
                    ctx.model, ctx.cwd, ctx.auto_approve
                );
                CommandResult::ok(output)
            },
        ));

        // /model
        registry.register(Command::new(
            "model",
            vec![],
            "Show or change the model",
            |args, ctx| {
                if args.is_empty() {
                    CommandResult::ok(format!("Current model: {}", ctx.model))
                } else {
                    CommandResult::ok(format!("MODEL:{}", args[0]))
                }
            },
        ));

        // /status
        registry.register(Command::new(
            "status",
            vec!["info".into()],
            "Show session info",
            |_args, ctx| {
                let output = format!(
                    "Session: {}\nMessages: {}\nTurns: {}\nModel: {}\nCWD: {}",
                    ctx.session_id, ctx.message_count, ctx.turn_count, ctx.model, ctx.cwd
                );
                CommandResult::ok(output)
            },
        ));

        // /session
        registry.register(Command::new(
            "session",
            vec!["sessions".into()],
            "List saved sessions",
            |_args, _ctx| CommandResult::ok("SESSION:LIST"),
        ));

        // /resume
        registry.register(Command::new(
            "resume",
            vec![],
            "Resume a saved session",
            |args, _ctx| {
                if args.is_empty() {
                    CommandResult::ok("Usage: /resume <session-id>")
                } else {
                    CommandResult::ok(format!("RESUME:{}", args[0]))
                }
            },
        ));

        // /history
        registry.register(Command::new(
            "history",
            vec!["hist".into()],
            "Show conversation history",
            |_args, ctx| {
                if ctx.history.is_empty() {
                    CommandResult::ok("(no history)")
                } else {
                    CommandResult::ok(ctx.history.join("\n"))
                }
            },
        ));

        // /diff
        registry.register(Command::new(
            "diff",
            vec![],
            "Show git diff",
            |_args, _ctx| CommandResult::ok("DIFF"),
        ));

        // /init
        registry.register(Command::new(
            "init",
            vec![],
            "Initialize project (.claude/, CLAUDE.md)",
            |_args, _ctx| CommandResult::ok("INIT"),
        ));

        // /version
        registry.register(Command::new(
            "version",
            vec!["ver".into()],
            "Show version information",
            |_args, _ctx| {
                CommandResult::ok(format!(
                    "claw-rs v{}\nA working AI agent runtime\nInspired by claw-code, reimplemented in Rust",
                    env!("CARGO_PKG_VERSION")
                ))
            },
        ));

        // /permissions
        registry.register(Command::new(
            "permissions",
            vec!["perms".into()],
            "Show current permission settings",
            |_args, ctx| {
                let mut lines = vec!["Permission settings:".to_string()];
                lines.push(format!("  Auto-approve: {}", ctx.auto_approve));
                if !ctx.allow_list.is_empty() {
                    lines.push(format!("  Allow: {}", ctx.allow_list.join(", ")));
                }
                if !ctx.ask_list.is_empty() {
                    lines.push(format!("  Ask: {}", ctx.ask_list.join(", ")));
                }
                if !ctx.deny_list.is_empty() {
                    lines.push(format!("  Deny: {}", ctx.deny_list.join(", ")));
                }
                CommandResult::ok(lines.join("\n"))
            },
        ));

        // /export
        registry.register(Command::new(
            "export",
            vec![],
            "Export conversation to file",
            |args, ctx| {
                let filename = args.first().copied().unwrap_or("conversation.txt");
                if ctx.history.is_empty() {
                    CommandResult::ok("Nothing to export (empty conversation)")
                } else {
                    CommandResult::ok(format!("EXPORT:{}", filename))
                }
            },
        ));

        // /quit
        registry.register(Command::new(
            "quit",
            vec!["exit".into(), "q".into()],
            "Exit the REPL",
            |_args, _ctx| CommandResult::exit("Goodbye!"),
        ));

        registry
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> CommandContext {
        CommandContext {
            model: "claude-sonnet-4-20250514".to_string(),
            cwd: "/home/user/project".to_string(),
            session_id: "test-session-123".to_string(),
            message_count: 5,
            turn_count: 3,
            input_tokens: 1000,
            output_tokens: 500,
            auto_approve: false,
            history: vec!["User: hello".to_string(), "Assistant: hi".to_string()],
            allow_list: vec!["read".to_string(), "glob".to_string()],
            ask_list: vec!["bash".to_string(), "write".to_string()],
            deny_list: vec![],
        }
    }

    #[test]
    fn test_parse_simple_command() {
        let result = CommandRegistry::parse("/help");
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "help");
        assert!(args.is_empty());
    }

    #[test]
    fn test_parse_command_with_args() {
        let result = CommandRegistry::parse("/compact 5");
        let (name, args) = result.unwrap();
        assert_eq!(name, "compact");
        assert_eq!(args, vec!["5"]);
    }

    #[test]
    fn test_parse_command_with_multiple_args() {
        let result = CommandRegistry::parse("/model claude-3-opus fast");
        let (name, args) = result.unwrap();
        assert_eq!(name, "model");
        assert_eq!(args, vec!["claude-3-opus", "fast"]);
    }

    #[test]
    fn test_parse_not_a_command() {
        assert!(CommandRegistry::parse("hello").is_none());
        assert!(CommandRegistry::parse("").is_none());
    }

    #[test]
    fn test_registry_with_defaults() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.len() >= 15);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_lookup_by_name() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.get("help").is_some());
        assert!(registry.get("clear").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_lookup_by_alias() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.get("h").is_some()); // alias for help
        assert!(registry.get("?").is_some()); // alias for help
        assert!(registry.get("q").is_some()); // alias for quit
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.get("HELP").is_some());
        assert!(registry.get("Help").is_some());
    }

    #[test]
    fn test_help_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("help", &[], &ctx).unwrap();
        assert!(result.output.contains("/help"));
        assert!(result.output.contains("/clear"));
        assert!(result.output.contains("/quit"));
        assert!(result.continue_loop);
    }

    #[test]
    fn test_cost_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("cost", &[], &ctx).unwrap();
        assert!(result.output.contains("1000"));
        assert!(result.output.contains("500"));
        assert!(result.output.contains("1500"));
    }

    #[test]
    fn test_status_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("status", &[], &ctx).unwrap();
        assert!(result.output.contains("test-session-123"));
        assert!(result.output.contains("Messages: 5"));
        assert!(result.output.contains("Turns: 3"));
    }

    #[test]
    fn test_config_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("config", &[], &ctx).unwrap();
        assert!(result.output.contains("sonnet"));
        assert!(result.output.contains("/home/user/project"));
    }

    #[test]
    fn test_model_command_show() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("model", &[], &ctx).unwrap();
        assert!(result.output.contains("sonnet"));
    }

    #[test]
    fn test_model_command_change() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("model", &["opus"], &ctx).unwrap();
        assert!(result.output.contains("MODEL:opus"));
    }

    #[test]
    fn test_history_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("history", &[], &ctx).unwrap();
        assert!(result.output.contains("User: hello"));
        assert!(result.output.contains("Assistant: hi"));
    }

    #[test]
    fn test_history_empty() {
        let registry = CommandRegistry::with_defaults();
        let mut ctx = test_context();
        ctx.history = vec![];
        let result = registry.execute("history", &[], &ctx).unwrap();
        assert!(result.output.contains("no history"));
    }

    #[test]
    fn test_permissions_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("permissions", &[], &ctx).unwrap();
        assert!(result.output.contains("read"));
        assert!(result.output.contains("bash"));
        assert!(result.output.contains("Auto-approve: false"));
    }

    #[test]
    fn test_version_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("version", &[], &ctx).unwrap();
        assert!(result.output.contains("claw-rs"));
    }

    #[test]
    fn test_quit_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("quit", &[], &ctx).unwrap();
        assert!(!result.continue_loop);
    }

    #[test]
    fn test_compact_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("compact", &["5"], &ctx).unwrap();
        assert!(result.output.contains("COMPACT:5"));
    }

    #[test]
    fn test_compact_default() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("compact", &[], &ctx).unwrap();
        assert!(result.output.contains("COMPACT:10"));
    }

    #[test]
    fn test_resume_no_args() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("resume", &[], &ctx).unwrap();
        assert!(result.output.contains("Usage"));
    }

    #[test]
    fn test_resume_with_id() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("resume", &["abc123"], &ctx).unwrap();
        assert!(result.output.contains("RESUME:abc123"));
    }

    #[test]
    fn test_export_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        let result = registry.execute("export", &["out.txt"], &ctx).unwrap();
        assert!(result.output.contains("EXPORT:out.txt"));
    }

    #[test]
    fn test_export_empty_history() {
        let registry = CommandRegistry::with_defaults();
        let mut ctx = test_context();
        ctx.history = vec![];
        let result = registry.execute("export", &[], &ctx).unwrap();
        assert!(result.output.contains("Nothing to export"));
    }

    #[test]
    fn test_command_names() {
        let registry = CommandRegistry::with_defaults();
        let names = registry.names();
        assert!(names.contains(&"help"));
        assert!(names.contains(&"clear"));
        assert!(names.contains(&"quit"));
    }

    #[test]
    fn test_command_debug() {
        let cmd = Command::new("test", vec![], "A test command", |_, _| {
            CommandResult::ok("ok")
        });
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_alias_usage_command() {
        let registry = CommandRegistry::with_defaults();
        let ctx = test_context();
        // "usage" is an alias for "cost"
        let result = registry.execute("usage", &[], &ctx).unwrap();
        assert!(result.output.contains("Token usage"));
    }
}
