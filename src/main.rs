//! claw-rs: A working AI agent runtime
//!
//! Inspired by claw-code. Reimplemented from scratch in Rust.
//! For educational purposes only, non-commercial use.

use clap::Parser;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// claw-rs — AI agent runtime with tool execution
#[derive(Parser, Debug)]
#[command(name = "claw-rs", version, about, long_about = None)]
struct Cli {
    /// Prompt to execute (if not provided, enters REPL mode)
    #[arg(short, long)]
    prompt: Option<String>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Model to use
    #[arg(short, long)]
    model: Option<String>,

    /// Auto-approve all tool calls (dangerous)
    #[arg(long, default_value_t = false)]
    auto_approve: bool,

    /// Maximum turns in the agent loop
    #[arg(long)]
    max_turns: Option<usize>,

    /// Session ID to resume
    #[arg(long)]
    resume: Option<String>,

    /// Print version information
    #[arg(long)]
    info: bool,
}

/// Handle slash commands in REPL mode
fn handle_slash_command(input: &str, agent: &mut claw_rs::agent::Agent) -> bool {
    let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
    let command = parts[0].to_lowercase();

    match command.as_str() {
        "/help" => {
            println!("Available commands:");
            println!("  /help     — Show this help message");
            println!("  /clear    — Clear conversation history");
            println!("  /compact  — Compact conversation (keep last N messages)");
            println!("  /cost     — Show token usage statistics");
            println!("  /tools    — List available tools");
            println!("  /status   — Show session info");
            println!("  /quit     — Exit the REPL");
            true
        }
        "/clear" => {
            agent.clear();
            println!("Conversation cleared.");
            true
        }
        "/compact" => {
            let keep = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
            agent.compact(keep);
            println!("Compacted to last {} messages.", keep);
            true
        }
        "/cost" => {
            let usage = agent.total_usage();
            println!("Token usage:");
            println!("  Input:  {} tokens", usage.input_tokens);
            println!("  Output: {} tokens", usage.output_tokens);
            println!("  Total:  {} tokens", usage.total());
            true
        }
        "/tools" => {
            let names = agent.tools().names();
            println!("Available tools ({}):", names.len());
            for name in &names {
                if let Some(tool) = agent.tools().get(name) {
                    println!("  {} — {}", name, tool.description());
                }
            }
            true
        }
        "/status" => {
            let session = agent.session();
            println!("Session: {}", session.id);
            println!("Messages: {}", session.message_count());
            println!("Turns: {}", session.metadata.turn_count);
            println!("Model: {}", session.metadata.model);
            println!("CWD: {}", session.metadata.cwd);
            true
        }
        "/quit" | "/exit" | "/q" => {
            println!("Goodbye!");
            std::process::exit(0);
        }
        _ => {
            println!("Unknown command: {}. Type /help for available commands.", command);
            true
        }
    }
}

/// Interactive REPL mode
async fn run_repl(agent: &mut claw_rs::agent::Agent) {
    println!("claw-rs v{} — AI Agent Runtime", env!("CARGO_PKG_VERSION"));
    println!("Type /help for commands, /quit to exit.\n");

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match reader.read_line(&mut input) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Handle slash commands
        if input.starts_with('/') {
            handle_slash_command(input, agent);
            continue;
        }

        // Process through agent
        println!();
        let result = agent.process_message(input).await;
        println!();

        // Show tool calls if any
        if !result.tool_calls.is_empty() {
            for call in &result.tool_calls {
                if call.result.is_error {
                    eprintln!("[Tool {} error: {}]", call.name, call.result.output);
                }
            }
        }

        match result.stop_reason {
            claw_rs::agent::TurnStopReason::MaxTurns => {
                println!("[Max turns reached]");
            }
            claw_rs::agent::TurnStopReason::BudgetExceeded => {
                println!("[Token budget exceeded]");
            }
            claw_rs::agent::TurnStopReason::Error(ref e) => {
                eprintln!("[Error: {}]", e);
            }
            _ => {}
        }
    }
}

/// Single prompt mode
async fn run_prompt(agent: &mut claw_rs::agent::Agent, prompt: &str) {
    let result = agent.process_message(prompt).await;

    if result.text.is_empty() && result.tool_calls.is_empty() {
        eprintln!("No response received.");
    }

    match result.stop_reason {
        claw_rs::agent::TurnStopReason::Error(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    if cli.info {
        println!("claw-rs v{}", env!("CARGO_PKG_VERSION"));
        println!("A working AI agent runtime");
        println!("Inspired by claw-code. Reimplemented from scratch in Rust.");
        println!("For educational purposes only, non-commercial use.");
        return;
    }

    // Load configuration
    let mut config = match claw_rs::config::AppConfig::load(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {}", e);
            std::process::exit(1);
        }
    };

    // Apply CLI overrides
    if let Some(model) = cli.model {
        config.llm.model = model;
    }
    if cli.auto_approve {
        config.permissions.auto_approve = true;
    }
    if let Some(max_turns) = cli.max_turns {
        config.agent.max_turns = max_turns;
    }

    // Validate config (warn but don't fail if API key missing — tools still work)
    if let Err(e) = config.validate() {
        if cli.prompt.is_some() {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        } else {
            eprintln!("Warning: {} (LLM calls will fail)", e);
        }
    }

    // Create agent
    let mut agent = claw_rs::agent::Agent::new(&config);

    // Set up permission callback for interactive mode
    if !cli.auto_approve {
        agent.set_permission_callback(|reason| {
            eprint!("Permission required: {} [y/N] ", reason);
            io::stderr().flush().unwrap();
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap_or(0);
            input.trim().to_lowercase() == "y"
        });
    }

    // Run in appropriate mode
    match cli.prompt {
        Some(prompt) => run_prompt(&mut agent, &prompt).await,
        None => run_repl(&mut agent).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_default() {
        let cli = Cli::parse_from(["claw-rs"]);
        assert!(cli.prompt.is_none());
        assert!(!cli.auto_approve);
        assert!(cli.max_turns.is_none());
    }

    #[test]
    fn test_cli_parse_with_prompt() {
        let cli = Cli::parse_from(["claw-rs", "--prompt", "hello"]);
        assert_eq!(cli.prompt, Some("hello".to_string()));
    }

    #[test]
    fn test_cli_parse_auto_approve() {
        let cli = Cli::parse_from(["claw-rs", "--auto-approve"]);
        assert!(cli.auto_approve);
    }

    #[test]
    fn test_cli_parse_max_turns() {
        let cli = Cli::parse_from(["claw-rs", "--max-turns", "50"]);
        assert_eq!(cli.max_turns, Some(50));
    }

    #[test]
    fn test_cli_parse_model() {
        let cli = Cli::parse_from(["claw-rs", "--model", "claude-3-opus"]);
        assert_eq!(cli.model, Some("claude-3-opus".to_string()));
    }

    #[test]
    fn test_handle_slash_help() {
        let config = claw_rs::config::AppConfig {
            llm: claw_rs::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = claw_rs::agent::Agent::new(&config);
        assert!(handle_slash_command("/help", &mut agent));
    }

    #[test]
    fn test_handle_slash_clear() {
        let config = claw_rs::config::AppConfig {
            llm: claw_rs::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = claw_rs::agent::Agent::new(&config);
        agent.session_mut().add_message(claw_rs::llm::Message::user("test"));
        handle_slash_command("/clear", &mut agent);
        assert_eq!(agent.session().message_count(), 0);
    }

    #[test]
    fn test_handle_slash_cost() {
        let config = claw_rs::config::AppConfig {
            llm: claw_rs::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = claw_rs::agent::Agent::new(&config);
        assert!(handle_slash_command("/cost", &mut agent));
    }

    #[test]
    fn test_handle_slash_tools() {
        let config = claw_rs::config::AppConfig {
            llm: claw_rs::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = claw_rs::agent::Agent::new(&config);
        assert!(handle_slash_command("/tools", &mut agent));
    }

    #[test]
    fn test_handle_unknown_command() {
        let config = claw_rs::config::AppConfig {
            llm: claw_rs::config::LlmConfig {
                api_key: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut agent = claw_rs::agent::Agent::new(&config);
        assert!(handle_slash_command("/unknown", &mut agent));
    }
}
