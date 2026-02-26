//! Configuration display and editing.

use colored::Colorize;

pub fn handle(key: Option<&str>, value: Option<&str>, character_path: &str) -> anyhow::Result<()> {
    match (key, value) {
        (None, None) => {
            // Display current effective configuration
            println!("{}", "Zoey Configuration:".cyan());
            println!();

            println!("  {}", "Character:".bold());
            println!("    file: {}", character_path);

            println!();
            println!("  {}", "LLM Provider:".bold());
            let provider = std::env::var("MODEL_PROVIDER").unwrap_or_else(|_| "local (Ollama)".to_string());
            println!("    provider:  {}", provider);
            println!("    ollama:    {}", std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string()));
            println!("    model:     {}", std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "(default)".to_string()));
            println!("    openai:    {}", if std::env::var("OPENAI_API_KEY").is_ok() { "configured".green().to_string() } else { "not set".dimmed().to_string() });
            println!("    anthropic: {}", if std::env::var("ANTHROPIC_API_KEY").is_ok() { "configured".green().to_string() } else { "not set".dimmed().to_string() });

            println!();
            println!("  {}", "Database:".bold());
            println!("    url: {}", std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string()));

            println!();
            println!("  {}", "API Server:".bold());
            println!("    host: {}", std::env::var("AGENT_API_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()));
            println!("    port: {}", std::env::var("AGENT_API_PORT").unwrap_or_else(|_| "9090".to_string()));

            println!();
            println!("  {}", "Paths:".bold());
            println!("    data:       ./data/");
            println!("    vectors:    ./data/vectors/");
            println!("    characters: ./characters/");

            println!();
            println!("{}", "Edit .env to change settings. Run `zoey doctor` to verify.".dimmed());
        }

        (Some(key), None) => {
            // Get a specific env var
            match std::env::var(key) {
                Ok(val) => println!("{} = {}", key, val),
                Err(_) => println!("{} is not set", key.dimmed()),
            }
        }

        (Some(key), Some(value)) => {
            // We can't persistently set env vars, but we can show what to add to .env
            println!("{}", "To set this permanently, add to your .env file:".dimmed());
            println!();
            println!("  {}={}", key, value);
            println!();

            // Also check if .env exists
            if std::path::Path::new(".env").exists() {
                println!("{}", "An .env file exists in the current directory.".dimmed());
            } else if std::path::Path::new(".env.example").exists() {
                println!("{}", "Tip: copy .env.example to .env and edit it.".dimmed());
            }
        }

        _ => unreachable!(),
    }

    Ok(())
}
