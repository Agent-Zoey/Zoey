//! Knowledge management — ingest, search, list, clear.

use colored::Colorize;
use crate::KnowledgeAction;

pub async fn handle(action: &KnowledgeAction) -> anyhow::Result<()> {
    match action {
        KnowledgeAction::Ingest { path } => {
            let path = std::path::Path::new(path);
            if !path.exists() {
                println!("{} File not found: {}", "✗".red(), path.display());
                return Ok(());
            }

            let is_dir = path.is_dir();
            let files: Vec<_> = if is_dir {
                std::fs::read_dir(path)?
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.ends_with(".md")
                            || name.ends_with(".txt")
                            || name.ends_with(".pdf")
                            || name.ends_with(".json")
                            || name.ends_with(".csv")
                    })
                    .map(|e| e.path())
                    .collect()
            } else {
                vec![path.to_path_buf()]
            };

            if files.is_empty() {
                println!("{}", "No supported files found. Supported: .md, .txt, .pdf, .json, .csv".dimmed());
                return Ok(());
            }

            println!("{} {} file(s)...", "Ingesting".cyan(), files.len());

            for file in &files {
                let name = file.file_name().unwrap_or_default().to_string_lossy();
                let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);

                // Read the file content
                match std::fs::read_to_string(file) {
                    Ok(content) => {
                        let lines = content.lines().count();
                        let chars = content.len();

                        // Store in the vector data directory
                        let data_dir = std::path::Path::new("./data/vectors");
                        std::fs::create_dir_all(data_dir)?;

                        let slug = name
                            .replace(' ', "-")
                            .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '.', "")
                            .to_lowercase();
                        let dest = data_dir.join(&slug);
                        std::fs::write(&dest, &content)?;

                        println!("  {} {} ({} lines, {} chars, {} bytes)",
                            "✓".green(), name.bold(), lines, chars, size);
                    }
                    Err(e) => {
                        println!("  {} {} — {}", "✗".red(), name, e);
                    }
                }
            }

            println!();
            println!("{}", "Documents saved to ./data/vectors/".dimmed());
            println!("{}", "They will be indexed when the agent starts with the knowledge plugin.".dimmed());
        }

        KnowledgeAction::Search { query, limit } => {
            println!("{} \"{}\" (top {})...", "Searching".cyan(), query, limit);

            // Check if vector store has data
            let data_dir = std::path::Path::new("./data/vectors");
            if !data_dir.exists() || std::fs::read_dir(data_dir)?.count() == 0 {
                println!("{}", "No documents ingested. Run: zoey knowledge ingest <path>".dimmed());
                return Ok(());
            }

            // Simple text search over stored documents
            let mut results = Vec::new();
            for entry in std::fs::read_dir(data_dir)? {
                let entry = entry?;
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    let query_lower = query.to_lowercase();
                    let content_lower = content.to_lowercase();

                    // Simple relevance: count query word occurrences
                    let score: usize = query_lower
                        .split_whitespace()
                        .map(|word| content_lower.matches(word).count())
                        .sum();

                    if score > 0 {
                        let name = entry.file_name().to_string_lossy().to_string();
                        // Extract a snippet around the first match
                        let snippet = if let Some(pos) = content_lower.find(&query_lower.split_whitespace().next().unwrap_or("")) {
                            let start = pos.saturating_sub(50);
                            let end = (pos + 150).min(content.len());
                            format!("...{}...", &content[start..end].replace('\n', " "))
                        } else {
                            content.chars().take(100).collect::<String>()
                        };

                        results.push((score, name, snippet));
                    }
                }
            }

            results.sort_by(|a, b| b.0.cmp(&a.0));

            if results.is_empty() {
                println!("  {}", "No matching documents found.".dimmed());
            } else {
                println!();
                for (i, (score, name, snippet)) in results.iter().take(*limit).enumerate() {
                    println!("  {}. {} {}", i + 1, name.bold(), format!("(score: {})", score).dimmed());
                    println!("     {}", snippet.dimmed());
                    println!();
                }
            }
        }

        KnowledgeAction::List => {
            let data_dir = std::path::Path::new("./data/vectors");
            if !data_dir.exists() {
                println!("{}", "No knowledge base found. Run: zoey knowledge ingest <path>".dimmed());
                return Ok(());
            }

            let entries: Vec<_> = std::fs::read_dir(data_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .collect();

            if entries.is_empty() {
                println!("{}", "Knowledge base is empty.".dimmed());
            } else {
                println!("{} ({} documents):", "Knowledge base".cyan(), entries.len());
                for entry in &entries {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    println!("  {} {} {}", "•".cyan(), name.bold(), format!("({} bytes)", size).dimmed());
                }
            }
        }

        KnowledgeAction::Clear { yes } => {
            if !yes {
                println!("{} This will delete all ingested documents. Run with --yes to confirm.", "⚠".yellow());
                return Ok(());
            }

            let data_dir = std::path::Path::new("./data/vectors");
            if data_dir.exists() {
                let count = std::fs::read_dir(data_dir)?.count();
                std::fs::remove_dir_all(data_dir)?;
                std::fs::create_dir_all(data_dir)?;
                println!("{} Cleared {} documents from knowledge base.", "✓".green(), count);
            } else {
                println!("{}", "Knowledge base is already empty.".dimmed());
            }
        }
    }

    Ok(())
}
