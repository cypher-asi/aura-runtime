//! # aura-cli
//!
//! Interactive CLI for the Aura Swarm.
//!
//! Provides a REPL interface for interacting with Aura agents.

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

mod approval;
mod session;

use anyhow::Result;
use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::Editor;
use session::{Session, SessionConfig};
use tracing::info;
use tracing_subscriber::EnvFilter;

// ============================================================================
// Commands
// ============================================================================

/// Parsed CLI command.
#[derive(Debug)]
enum Command {
    /// User prompt to the agent
    Prompt(String),
    /// Show agent status
    Status,
    /// Show history
    History(usize),
    /// Approve pending tool request
    Approve,
    /// Deny pending tool request
    Deny,
    /// Show pending changes
    Diff,
    /// Show help
    Help,
    /// Exit CLI
    Quit,
    /// Unknown command
    Unknown(String),
}

impl Command {
    fn parse(input: &str) -> Self {
        let input = input.trim();

        if input.is_empty() {
            return Self::Prompt(String::new());
        }

        if !input.starts_with('/') {
            return Self::Prompt(input.to_string());
        }

        let parts: Vec<&str> = input[1..].splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).unwrap_or(&"").trim();

        match cmd.as_str() {
            "status" | "s" => Self::Status,
            "history" | "h" => {
                let n = arg.parse().unwrap_or(10);
                Self::History(n)
            }
            "approve" | "yes" | "y" => Self::Approve,
            "deny" | "no" | "n" => Self::Deny,
            "diff" | "d" => Self::Diff,
            "help" | "?" => Self::Help,
            "quit" | "exit" | "q" => Self::Quit,
            _ => Self::Unknown(cmd),
        }
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    println!("{}", banner());

    // Create session
    let config = SessionConfig::from_env()?;
    let mut session = Session::new(config).await?;

    info!(agent_id = %session.agent_id(), "Session started");

    // Create readline editor
    let mut rl: Editor<(), DefaultHistory> = Editor::new()?;
    let history_path = dirs::data_local_dir().map(|p| p.join("aura").join("history.txt"));

    // Load history if available
    if let Some(ref path) = history_path {
        let _ = rl.load_history(path);
    }

    // REPL loop
    loop {
        let prompt = format!("{} ", "aura>".cyan().bold());
        match rl.readline(&prompt) {
            Ok(line) => {
                let _ = rl.add_history_entry(&line);

                match Command::parse(&line) {
                    Command::Prompt(text) => {
                        if text.is_empty() {
                            continue;
                        }
                        handle_prompt(&mut session, &text).await;
                    }
                    Command::Status => handle_status(&session),
                    Command::History(n) => handle_history(&session, n),
                    Command::Approve => handle_approve(&mut session).await,
                    Command::Deny => handle_deny(&mut session).await,
                    Command::Diff => handle_diff(&session),
                    Command::Help => print_help(),
                    Command::Quit => {
                        println!("{}", "Goodbye!".yellow());
                        break;
                    }
                    Command::Unknown(cmd) => {
                        println!(
                            "{} Unknown command: {}. Type /help for available commands.",
                            "Error:".red().bold(),
                            cmd
                        );
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "Use /quit or Ctrl-D to exit.".yellow());
            }
            Err(ReadlineError::Eof) => {
                println!("{}", "Goodbye!".yellow());
                break;
            }
            Err(err) => {
                eprintln!("{} {:?}", "Error:".red().bold(), err);
                break;
            }
        }
    }

    // Save history
    if let Some(ref path) = history_path {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = rl.save_history(path);
    }

    Ok(())
}

// ============================================================================
// Command Handlers
// ============================================================================

async fn handle_prompt(session: &mut Session, text: &str) {
    println!("{} Processing...\n", "▶".blue().bold());

    match session.submit_prompt(text).await {
        Ok(result) => {
            // Print the assistant's response
            if let Some(message) = &result.final_message {
                let text = message.text_content();
                if !text.is_empty() {
                    println!("{}", text);
                }
            }

            // Print stats
            println!(
                "\n{} Steps: {}, Input tokens: {}, Output tokens: {}",
                "✓".green().bold(),
                result.steps,
                result.total_input_tokens,
                result.total_output_tokens
            );

            if result.had_failures {
                println!("{} Some tool calls failed", "⚠".yellow().bold());
            }
        }
        Err(e) => {
            eprintln!("{} {}", "Error:".red().bold(), e);
        }
    }
    println!();
}

fn handle_status(session: &Session) {
    println!("{}", "Session Status".cyan().bold());
    println!("  Agent ID: {}", session.agent_id());
    println!("  Sequence: {}", session.current_seq());
    println!("  Provider: {}", session.provider_name());
    println!();
}

fn handle_history(_session: &Session, _n: usize) {
    println!("{} History display not yet implemented", "ℹ".blue().bold());
    println!();
}

async fn handle_approve(session: &mut Session) {
    if let Err(e) = session.approve_pending().await {
        eprintln!("{} {}", "Error:".red().bold(), e);
    } else {
        println!("{} Approved", "✓".green().bold());
    }
    println!();
}

async fn handle_deny(session: &mut Session) {
    if let Err(e) = session.deny_pending().await {
        eprintln!("{} {}", "Error:".red().bold(), e);
    } else {
        println!("{} Denied", "✗".red().bold());
    }
    println!();
}

fn handle_diff(_session: &Session) {
    println!("{} Diff display not yet implemented", "ℹ".blue().bold());
    println!();
}

fn print_help() {
    println!("{}", "Available Commands".cyan().bold());
    println!();
    println!("  {}    Submit a prompt to the agent", "<text>".green());
    println!("  {}   Show agent status", "/status".green());
    println!("  {} Show last N history entries", "/history [n]".green());
    println!("  {}  Approve pending tool request", "/approve".green());
    println!("  {}     Deny pending tool request", "/deny".green());
    println!("  {}     Show pending file changes", "/diff".green());
    println!("  {}     Show this help message", "/help".green());
    println!("  {}     Exit the CLI", "/quit".green());
    println!();
    println!("  Shortcuts: /s, /h, /y, /n, /d, /?, /q");
    println!();
}

fn banner() -> String {
    format!(
        r#"
{}
Version: {}
Type /help for available commands.
"#,
        r#"
    _   _   _ ____      _    
   / \ | | | |  _ \    / \   
  / _ \| | | | |_) |  / _ \  
 / ___ \ |_| |  _ <  / ___ \ 
/_/   \_\___/|_| \_\/_/   \_\
"#
        .cyan()
        .bold(),
        env!("CARGO_PKG_VERSION")
    )
}
