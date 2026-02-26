//! Chat module — interactive and one-shot messaging via AgentRuntime.

use std::sync::{Arc, RwLock};

use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use zoey_core::AgentRuntime;
use zoey_core::MessageProcessor;
use zoey_core::types::{ChannelType, Content, Memory, Room};

/// Send a single message and return the response (one-shot mode).
pub async fn one_shot(
    runtime: &Arc<RwLock<AgentRuntime>>,
    message: &str,
) -> anyhow::Result<String> {
    let (entity_id, room) = make_room();
    let memory = make_memory(entity_id, room.id, message);

    let processor = MessageProcessor::new(runtime.clone());
    let responses = processor.process_message(memory, room).await?;

    let text = responses
        .iter()
        .map(|m| m.content.text.clone())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(text)
}

/// Run an interactive chat session.
pub async fn interactive(
    runtime: &Arc<RwLock<AgentRuntime>>,
    _character_path: &str,
) -> anyhow::Result<()> {
    let agent_name = {
        let rt = runtime.read().unwrap();
        rt.character.name.clone()
    };

    println!();
    println!("  {}", "╔══════════════════════════════════════════════╗".cyan());
    println!("  {}  {} — {}",
        "║".cyan(), agent_name.bold(), "interactive chat".dimmed());
    println!("  {}  {} {}",
        "║".cyan(), "Type".dimmed(), "/help for commands, Ctrl+D to exit".dimmed());
    println!("  {}", "╚══════════════════════════════════════════════╝".cyan());
    println!();

    let mut rl = DefaultEditor::new()?;
    let history_dir = dirs::home_dir().map(|h| h.join(".zoey")).unwrap_or_default();
    let _ = std::fs::create_dir_all(&history_dir);
    let history_path = history_dir.join("chat_history.txt");
    let _ = rl.load_history(&history_path);

    loop {
        let prompt = format!("{} ", "you>".green().bold());

        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim().to_string();
                if line.is_empty() { continue; }
                let _ = rl.add_history_entry(&line);

                if line.starts_with('/') {
                    match handle_slash(&line, runtime, &agent_name) {
                        SlashResult::Continue => continue,
                        SlashResult::Exit => break,
                    }
                }

                let (entity_id, room) = make_room();
                let memory = make_memory(entity_id, room.id, &line);

                let spinner = indicatif::ProgressBar::new_spinner();
                spinner.set_style(
                    indicatif::ProgressStyle::with_template("  {spinner:.cyan} {msg}")
                        .unwrap()
                        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏·"),
                );
                spinner.set_message("thinking...");
                spinner.enable_steady_tick(std::time::Duration::from_millis(80));

                let processor = MessageProcessor::new(runtime.clone());
                match processor.process_message(memory, room).await {
                    Ok(responses) => {
                        spinner.finish_and_clear();
                        for resp in &responses {
                            println!("  {}: {}", agent_name.cyan().bold(), resp.content.text);
                        }
                        println!();
                    }
                    Err(e) => {
                        spinner.finish_and_clear();
                        eprintln!("  {} {}", "Error:".red(), e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("\n{}", "Use /exit or Ctrl+D to quit.".dimmed());
            }
            Err(ReadlineError::Eof) => {
                println!("\nGoodbye!");
                break;
            }
            Err(e) => {
                eprintln!("Input error: {}", e);
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
    Ok(())
}

enum SlashResult { Continue, Exit }

fn handle_slash(line: &str, runtime: &Arc<RwLock<AgentRuntime>>, _agent_name: &str) -> SlashResult {
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    match parts[0] {
        "/exit" | "/quit" | "/q" => {
            println!("Goodbye!");
            SlashResult::Exit
        }
        "/help" | "/h" | "/?" => {
            println!();
            println!("  {}", "Commands:".cyan());
            println!("    {}      Show this help", "/help".bold());
            println!("    {}     Show current provider and model", "/model".bold());
            println!("    {}  Show loaded plugins", "/plugins".bold());
            println!("    {}     Clear screen", "/clear".bold());
            println!("    {}      Exit", "/exit".bold());
            println!();
            SlashResult::Continue
        }
        "/model" => {
            let rt = runtime.read().unwrap();
            let provider = rt.get_setting("model_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "local".to_string());
            let model = rt.get_setting("LOCAL_LLM_MODEL")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "default".to_string());
            println!("  {}: {}", "Provider".cyan(), provider);
            println!("  {}: {}", "Model".cyan(), model);
            SlashResult::Continue
        }
        "/plugins" => {
            let rt = runtime.read().unwrap();
            let providers = rt.get_providers();
            let actions = rt.get_actions();
            let evaluators = rt.get_evaluators();
            println!("  {} ({})", "Providers".cyan(), providers.len());
            for p in &providers { println!("    • {}", p.name()); }
            println!("  {} ({})", "Actions".cyan(), actions.len());
            for a in &actions { println!("    • {}", a.name()); }
            println!("  {} ({})", "Evaluators".cyan(), evaluators.len());
            for e in &evaluators { println!("    • {}", e.name()); }
            SlashResult::Continue
        }
        "/clear" => {
            print!("\x1b[2J\x1b[1;1H");
            SlashResult::Continue
        }
        _ => {
            println!("  Unknown: {}. Type {}", parts[0], "/help".bold());
            SlashResult::Continue
        }
    }
}

/// Create a Room for CLI chat.
fn make_room() -> (uuid::Uuid, Room) {
    let entity_id = uuid::Uuid::new_v4();
    let room = Room {
        id: uuid::Uuid::new_v4(),
        agent_id: Some(entity_id),
        name: "cli-chat".to_string(),
        source: "cli".to_string(),
        channel_type: ChannelType::Dm,
        channel_id: None,
        server_id: None,
        world_id: uuid::Uuid::new_v4(),
        metadata: Default::default(),
        created_at: Some(chrono::Utc::now().timestamp()),
    };
    (entity_id, room)
}

/// Create a Memory (message) from user text.
fn make_memory(entity_id: uuid::Uuid, room_id: uuid::Uuid, text: &str) -> Memory {
    Memory {
        id: uuid::Uuid::new_v4(),
        entity_id,
        agent_id: entity_id,
        room_id,
        content: Content {
            text: text.to_string(),
            source: Some("cli".to_string()),
            ..Default::default()
        },
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: None,
        similarity: None,
    }
}
