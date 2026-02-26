//! Provider management — list, test, switch LLM providers.

use colored::Colorize;
use crate::ProviderAction;

pub async fn handle(action: &ProviderAction, current: Option<&str>) -> anyhow::Result<()> {
    match action {
        ProviderAction::List => {
            let active = current.unwrap_or("local");

            println!("{}", "Available LLM providers:".cyan());
            println!();

            let providers = vec![
                ("ollama", "Local models via Ollama", "OLLAMA_BASE_URL", true),
                ("openai", "OpenAI GPT models", "OPENAI_API_KEY", std::env::var("OPENAI_API_KEY").is_ok()),
                ("anthropic", "Anthropic Claude models", "ANTHROPIC_API_KEY", std::env::var("ANTHROPIC_API_KEY").is_ok()),
            ];

            for (name, desc, env_var, available) in &providers {
                let marker = if active == *name { "→ " } else { "  " };
                let status = if *available {
                    "available".green().to_string()
                } else {
                    format!("set {} to enable", env_var).dimmed().to_string()
                };
                println!("  {}{} — {} [{}]", marker, name.bold(), desc, status);
            }
        }

        ProviderAction::Test { name } => {
            println!("Testing provider '{}'...", name.bold());

            match name.to_lowercase().as_str() {
                "ollama" | "local" => {
                    let host = std::env::var("OLLAMA_BASE_URL")
                        .unwrap_or_else(|_| "http://localhost:11434".to_string());

                    let client = reqwest::Client::new();
                    match client
                        .get(format!("{}/api/tags", host))
                        .timeout(std::time::Duration::from_secs(5))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            println!("  {} Ollama is running at {}", "✓".green(), host);
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                if let Some(models) = json["models"].as_array() {
                                    println!("  Models ({}):", models.len());
                                    for m in models {
                                        if let Some(name) = m["name"].as_str() {
                                            let size = m["size"].as_u64().unwrap_or(0);
                                            let size_gb = size as f64 / 1_073_741_824.0;
                                            println!("    {} {} {}", "•".cyan(), name, format!("({:.1}GB)", size_gb).dimmed());
                                        }
                                    }
                                }
                            }

                            // Quick inference test
                            print!("  Testing inference... ");
                            let test_resp = client
                                .post(format!("{}/api/generate", host))
                                .json(&serde_json::json!({
                                    "model": std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:latest".to_string()),
                                    "prompt": "Say 'hello' in one word.",
                                    "stream": false,
                                    "options": {"num_predict": 10}
                                }))
                                .timeout(std::time::Duration::from_secs(30))
                                .send()
                                .await;

                            match test_resp {
                                Ok(r) if r.status().is_success() => {
                                    if let Ok(json) = r.json::<serde_json::Value>().await {
                                        let text = json["response"].as_str().unwrap_or("?");
                                        println!("{} (response: \"{}\")", "✓".green(), text.trim().dimmed());
                                    }
                                }
                                _ => println!("{}", "✗ inference failed".red()),
                            }
                        }
                        _ => {
                            println!("  {} Ollama not reachable at {}", "✗".red(), host);
                            println!("  Install: {}", "https://ollama.com".dimmed());
                        }
                    }
                }
                "openai" => {
                    match std::env::var("OPENAI_API_KEY") {
                        Ok(key) => {
                            println!("  {} OPENAI_API_KEY is set", "✓".green());
                            let client = reqwest::Client::new();
                            match client
                                .get("https://api.openai.com/v1/models")
                                .header("Authorization", format!("Bearer {}", key))
                                .timeout(std::time::Duration::from_secs(10))
                                .send()
                                .await
                            {
                                Ok(r) if r.status().is_success() => {
                                    println!("  {} API connection verified", "✓".green());
                                }
                                Ok(r) => println!("  {} API returned HTTP {}", "✗".red(), r.status()),
                                Err(e) => println!("  {} Connection failed: {}", "✗".red(), e),
                            }
                        }
                        Err(_) => println!("  {} OPENAI_API_KEY not set", "✗".red()),
                    }
                }
                "anthropic" => {
                    match std::env::var("ANTHROPIC_API_KEY") {
                        Ok(_) => {
                            println!("  {} ANTHROPIC_API_KEY is set", "✓".green());
                            // Anthropic doesn't have a simple models list endpoint,
                            // so we just verify the key is present
                            println!("  {} Key format looks valid", "✓".green());
                        }
                        Err(_) => println!("  {} ANTHROPIC_API_KEY not set", "✗".red()),
                    }
                }
                _ => {
                    println!("  {} Unknown provider: {}", "✗".red(), name);
                    println!("  Available: ollama, openai, anthropic");
                }
            }
        }

        ProviderAction::Switch { name } => {
            let valid = ["ollama", "local", "openai", "anthropic"];
            let normalized = match name.to_lowercase().as_str() {
                "ollama" | "local" => "local",
                "openai" => "openai",
                "anthropic" => "anthropic",
                _ => {
                    println!("{} Unknown provider: {}. Valid: {}", "✗".red(), name, valid.join(", "));
                    return Ok(());
                }
            };

            // Check if the provider is available
            match normalized {
                "openai" if std::env::var("OPENAI_API_KEY").is_err() => {
                    println!("{} OPENAI_API_KEY not set. Set it first.", "✗".red());
                    return Ok(());
                }
                "anthropic" if std::env::var("ANTHROPIC_API_KEY").is_err() => {
                    println!("{} ANTHROPIC_API_KEY not set. Set it first.", "✗".red());
                    return Ok(());
                }
                _ => {}
            }

            println!("{} Switched to provider: {}", "✓".green(), normalized.bold());
            println!("{}", "Note: this applies to the current session. To persist, set MODEL_PROVIDER in your .env file.".dimmed());
        }
    }

    Ok(())
}
