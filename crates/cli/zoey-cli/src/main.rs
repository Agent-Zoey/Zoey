//! zoey — privacy-first AI agent from the terminal.
//!
//! Usage:
//!   zoey                            # Interactive chat session
//!   zoey "message"                  # One-shot: send message, get response, exit
//!   zoey [SUBCOMMAND] [OPTIONS]     # Manage agents, plugins, knowledge, etc.

mod chat;
mod character;
mod config;
mod doctor;
mod knowledge;
mod plugins;
mod providers;
mod runtime_builder;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use colored::Colorize;

/// zoey — privacy-first AI agent from the terminal
#[derive(Parser, Debug)]
#[command(
    name = "zoey",
    version = "0.1.1",
    about = "Privacy-first AI agent — chat, manage knowledge, run workflows",
    long_about = "Zoey is a terminal-native AI agent framework. Chat with local or cloud \
                   models, ingest knowledge, run workflows, and manage your agent — all \
                   from one command. Privacy-first, local-first."
)]
struct Cli {
    /// Message to send (one-shot mode)
    #[arg(value_name = "MESSAGE")]
    message: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Character file to load
    #[arg(long, env = "CHARACTER_FILE", default_value = "characters/zoey-agent-sql.xml")]
    character: String,

    /// LLM provider to use (ollama, openai, anthropic)
    #[arg(long, env = "MODEL_PROVIDER")]
    provider: Option<String>,

    /// Model name override
    #[arg(long, env = "OLLAMA_MODEL")]
    model: Option<String>,

    /// Output as JSON (for scripting)
    #[arg(long)]
    json: bool,

    /// Log level
    #[arg(long, env = "ZOEY_LOG_LEVEL", default_value = "warn")]
    log_level: String,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start interactive chat session (default when no args)
    Chat,

    /// Run agent with adaptors (web, discord, telegram)
    Run {
        /// Adaptors to start (comma-separated: web,discord,telegram)
        #[arg(long, default_value = "web")]
        adaptors: String,
        /// Run headless (API server only, no terminal)
        #[arg(long)]
        headless: bool,
    },

    /// Manage agent characters
    Character {
        #[command(subcommand)]
        action: CharacterAction,
    },

    /// List and inspect loaded plugins
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Manage LLM providers
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },

    /// Knowledge base management (ingest, search, list)
    Knowledge {
        #[command(subcommand)]
        action: KnowledgeAction,
    },

    /// View or edit configuration
    Config {
        /// Key to get/set
        key: Option<String>,
        /// Value to set
        value: Option<String>,
    },

    /// Show database status and run migrations
    Db {
        #[command(subcommand)]
        action: DbAction,
    },

    /// Run a named workflow
    Workflow {
        /// Workflow name
        name: String,
    },

    /// Show agent logs (tail mode)
    Logs {
        /// Show only reasoning chains
        #[arg(long)]
        reasoning: bool,
    },

    /// Check system health and dependencies
    Doctor,

    /// Generate shell completions
    Completions {
        /// Shell type
        shell: Shell,
    },
}

#[derive(Subcommand, Debug)]
enum CharacterAction {
    /// List available character files
    List,
    /// Show character details
    Show {
        /// Character file or name
        name: String,
    },
    /// Validate a character file
    Validate {
        /// Path to character XML
        path: String,
    },
}

#[derive(Subcommand, Debug)]
enum PluginAction {
    /// List all loaded plugins
    List,
    /// Show details for a specific plugin
    Info {
        /// Plugin name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum ProviderAction {
    /// List available LLM providers
    List,
    /// Test a provider's connectivity
    Test {
        /// Provider name (ollama, openai, anthropic)
        name: String,
    },
    /// Switch active provider
    Switch {
        /// Provider name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum KnowledgeAction {
    /// Ingest documents into the knowledge base
    Ingest {
        /// Path to file or directory
        path: String,
    },
    /// Search the knowledge base
    Search {
        /// Search query
        query: String,
        /// Max results
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// List ingested collections
    List,
    /// Clear the knowledge base
    Clear {
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
enum DbAction {
    /// Show database connection status
    Status,
    /// Run pending migrations
    Migrate,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Logging
    let level = if cli.verbose { "debug" } else { &cli.log_level };
    tracing_subscriber::fmt()
        .with_env_filter(format!("zoey_cli={},zoey_core=warn,warn", level))
        .init();

    let _ = dotenvy::dotenv();

    match (cli.message.as_deref(), &cli.command) {
        // One-shot: zoey "hello"
        (Some(msg), None) => {
            let runtime = runtime_builder::build(&cli.character, cli.provider.as_deref(), cli.model.as_deref()).await?;
            let response = chat::one_shot(&runtime, msg).await?;
            if cli.json {
                println!("{}", serde_json::json!({"response": response}));
            } else {
                println!("{}", response);
            }
        }

        // Subcommand
        (_, Some(cmd)) => {
            handle_command(cmd, &cli).await?;
        }

        // Default: interactive chat
        (None, None) => {
            let runtime = runtime_builder::build(&cli.character, cli.provider.as_deref(), cli.model.as_deref()).await?;
            chat::interactive(&runtime, &cli.character).await?;
        }
    }

    Ok(())
}

async fn handle_command(cmd: &Commands, cli: &Cli) -> anyhow::Result<()> {
    match cmd {
        Commands::Chat => {
            let runtime = runtime_builder::build(&cli.character, cli.provider.as_deref(), cli.model.as_deref()).await?;
            chat::interactive(&runtime, &cli.character).await?;
        }

        Commands::Run { adaptors: _, headless } => {
            println!("{}", "Starting Zoey agent...".cyan());
            let runtime = runtime_builder::build(&cli.character, cli.provider.as_deref(), cli.model.as_deref()).await?;

            // Start agent API server
            let agent_host = std::env::var("AGENT_API_HOST").unwrap_or_else(|_| "127.0.0.1".into());
            let agent_port: u16 = std::env::var("AGENT_API_PORT").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(9090);

            use zoey_core::agent_api::{AgentApiConfig, AgentApiServer};
            let mut api = AgentApiServer::new(
                AgentApiConfig { host: agent_host.clone(), port: agent_port, ..Default::default() },
                runtime.clone(),
            );
            api.start().await?;
            println!("  {} Agent API on http://{}:{}/agent", "✓".green(), agent_host, agent_port);

            if !headless {
                println!("  {} Press Ctrl+C to stop", "→".cyan());
            }

            // Wait for shutdown
            tokio::signal::ctrl_c().await?;
            api.stop().await?;
        }

        Commands::Character { action } => {
            character::handle(action).await?;
        }

        Commands::Plugin { action } => {
            let runtime = runtime_builder::build(&cli.character, cli.provider.as_deref(), cli.model.as_deref()).await?;
            plugins::handle(action, &runtime)?;
        }

        Commands::Provider { action } => {
            providers::handle(action, cli.provider.as_deref()).await?;
        }

        Commands::Knowledge { action } => {
            knowledge::handle(action).await?;
        }

        Commands::Config { key, value } => {
            config::handle(key.as_deref(), value.as_deref(), &cli.character)?;
        }

        Commands::Db { action } => {
            match action {
                DbAction::Status => {
                    let db_url = std::env::var("DATABASE_URL")
                        .unwrap_or_else(|_| "sqlite::memory:".to_string());
                    println!("{}: {}", "Database URL".cyan(), db_url);
                }
                DbAction::Migrate => {
                    println!("Migrations are run automatically on agent startup.");
                }
            }
        }

        Commands::Workflow { name } => {
            println!("Running workflow '{}'...", name.bold());
            println!("{}", "Workflows are executed via the agent runtime. Use `zoey run` and trigger via the API.".dimmed());
        }

        Commands::Logs { reasoning } => {
            println!("Tailing agent logs{}...", if *reasoning { " (reasoning only)" } else { "" });
            println!("{}", "Start the agent first with `zoey run`, then view logs at http://127.0.0.1:9100".dimmed());
        }

        Commands::Doctor => {
            doctor::run().await?;
        }

        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            generate(*shell, &mut cmd, name, &mut std::io::stdout());
        }
    }

    Ok(())
}
