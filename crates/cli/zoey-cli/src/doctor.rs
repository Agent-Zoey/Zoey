//! System diagnostics — checks all dependencies and connectivity.

use colored::Colorize;

pub async fn run() -> anyhow::Result<()> {
    println!();
    println!("  {}", "Zoey Doctor — System Health Check".cyan().bold());
    println!("  {}", "═".repeat(40).dimmed());
    println!();

    let mut issues = 0;

    // ── Rust / Binary ──────────────────────────────────────────────
    println!("  {}", "Binary:".bold());
    println!("    {} zoey v{}", "✓".green(), env!("CARGO_PKG_VERSION"));

    // ── Environment ────────────────────────────────────────────────
    println!();
    println!("  {}", "Environment:".bold());

    if std::path::Path::new(".env").exists() {
        println!("    {} .env file found", "✓".green());
    } else if std::path::Path::new(".env.example").exists() {
        println!("    {} .env not found (copy .env.example to .env)", "⚠".yellow());
        issues += 1;
    } else {
        println!("    {} No .env or .env.example", "⚠".yellow());
        issues += 1;
    }

    // ── Characters ─────────────────────────────────────────────────
    println!();
    println!("  {}", "Characters:".bold());
    let chars_dir = std::path::Path::new("characters");
    if chars_dir.exists() {
        let count = std::fs::read_dir(chars_dir)
            .map(|entries| entries.filter(|e| {
                e.as_ref().ok().map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.ends_with(".xml") || n.ends_with(".json")
                }).unwrap_or(false)
            }).count())
            .unwrap_or(0);
        if count > 0 {
            println!("    {} {} character file(s) found", "✓".green(), count);
        } else {
            println!("    {} characters/ exists but no .xml/.json files", "⚠".yellow());
            issues += 1;
        }
    } else {
        println!("    {} characters/ directory not found", "✗".red());
        issues += 1;
    }

    // ── Ollama ─────────────────────────────────────────────────────
    println!();
    println!("  {}", "Ollama (Local LLM):".bold());
    let ollama_host = std::env::var("OLLAMA_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());

    let client = reqwest::Client::new();
    match client
        .get(format!("{}/api/tags", ollama_host))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            println!("    {} Running at {}", "✓".green(), ollama_host);
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(models) = json["models"].as_array() {
                    println!("    {} {} model(s) available", "✓".green(), models.len());
                    for m in models.iter().take(5) {
                        if let Some(name) = m["name"].as_str() {
                            println!("      • {}", name);
                        }
                    }
                    if models.len() > 5 {
                        println!("      ... and {} more", models.len() - 5);
                    }
                }
            }
        }
        _ => {
            println!("    {} Not reachable at {}", "✗".red(), ollama_host);
            println!("    {}", "Install: https://ollama.com".dimmed());
            issues += 1;
        }
    }

    // ── API Keys ───────────────────────────────────────────────────
    println!();
    println!("  {}", "API Keys:".bold());

    let keys = vec![
        ("OPENAI_API_KEY", "OpenAI", false),
        ("ANTHROPIC_API_KEY", "Anthropic", false),
    ];

    for (env_var, name, required) in &keys {
        match std::env::var(env_var) {
            Ok(val) => {
                let masked = format!("{}...{}", &val[..4.min(val.len())], &val[val.len().saturating_sub(4)..]);
                println!("    {} {} ({})", "✓".green(), name, masked.dimmed());
            }
            Err(_) => {
                if *required {
                    println!("    {} {} not set (required)", "✗".red(), name);
                    issues += 1;
                } else {
                    println!("    {} {} not set (optional)", "·".dimmed(), name);
                }
            }
        }
    }

    // ── Database ───────────────────────────────────────────────────
    println!();
    println!("  {}", "Database:".bold());
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());
    if db_url.starts_with("sqlite:") || db_url == "sqlite::memory:" {
        println!("    {} SQLite: {}", "✓".green(), db_url);
    } else if db_url.starts_with("postgres") {
        println!("    {} PostgreSQL: {}", "✓".green(), db_url.split('@').last().unwrap_or(&db_url));
    } else {
        println!("    {} Unknown DB: {}", "⚠".yellow(), db_url);
    }

    // ── Data directories ───────────────────────────────────────────
    println!();
    println!("  {}", "Storage:".bold());
    let dirs_to_check = vec![
        ("./data", "Data directory"),
        ("./data/vectors", "Vector store"),
    ];
    for (dir, desc) in &dirs_to_check {
        if std::path::Path::new(dir).exists() {
            println!("    {} {}: {}", "✓".green(), desc, dir);
        } else {
            println!("    {} {}: {} (will be created on first use)", "·".dimmed(), desc, dir);
        }
    }

    // ── Summary ────────────────────────────────────────────────────
    println!();
    println!("  {}", "═".repeat(40).dimmed());
    if issues == 0 {
        println!("  {} All checks passed!", "✓".green().bold());
    } else {
        println!("  {} {} issue(s) found", "⚠".yellow().bold(), issues);
    }
    println!();

    Ok(())
}
