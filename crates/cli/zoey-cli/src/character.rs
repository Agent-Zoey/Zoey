//! Character management — list, show, validate character files.

use colored::Colorize;
use crate::CharacterAction;

pub async fn handle(action: &CharacterAction) -> anyhow::Result<()> {
    match action {
        CharacterAction::List => {
            let chars_dir = std::path::Path::new("characters");
            if !chars_dir.exists() {
                println!("{}", "No characters/ directory found. Create one with character XML files.".dimmed());
                return Ok(());
            }

            println!("{}", "Available characters:".cyan());
            let mut count = 0;
            for entry in std::fs::read_dir(chars_dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".xml") || name.ends_with(".json") {
                    // Try to parse and get the character name
                    let path = entry.path();
                    let char_name = zoey_core::load_character_from_xml(path.to_str().unwrap_or(""))
                        .map(|c| c.name)
                        .unwrap_or_else(|_| "?".to_string());

                    println!("  {} {} ({})", "•".cyan(), name.bold(), char_name.dimmed());
                    count += 1;
                }
            }
            if count == 0 {
                println!("  {}", "No character files found.".dimmed());
            }
        }

        CharacterAction::Show { name } => {
            // Try as filename first, then search in characters/
            let path = if std::path::Path::new(name).exists() {
                name.clone()
            } else {
                format!("characters/{}", name)
            };

            match zoey_core::load_character_from_xml(&path) {
                Ok(character) => {
                    println!("{}: {}", "Name".cyan(), character.name.bold());
                    println!("{}: {:?}", "Clients".cyan(), character.clients);
                    if let Some(ref provider) = character.model_provider {
                        println!("{}: {}", "Provider".cyan(), provider);
                    }
                    println!("{}", "Bio:".cyan());
                    for line in &character.bio {
                        println!("  {}", line);
                    }
                    if !character.knowledge.is_empty() {
                        println!("{}", "Knowledge:".cyan());
                        for k in &character.knowledge {
                            println!("  • {}", k);
                        }
                    }
                    if !character.plugins.is_empty() {
                        println!("{}: {}", "Plugins".cyan(), character.plugins.join(", "));
                    }
                    println!("{}: {} entries", "Settings".cyan(), character.settings.len());
                }
                Err(e) => {
                    println!("{} Could not load character '{}': {}", "✗".red(), name, e);
                }
            }
        }

        CharacterAction::Validate { path } => {
            print!("Validating {}... ", path);
            match zoey_core::load_character_from_xml(path) {
                Ok(character) => {
                    println!("{}", "✓ Valid".green());
                    println!("  Name: {}", character.name);
                    println!("  Plugins: {}", character.plugins.len());
                    println!("  Bio lines: {}", character.bio.len());
                    println!("  Knowledge entries: {}", character.knowledge.len());

                    // Warnings
                    if character.name.is_empty() {
                        println!("  {} Name is empty", "⚠".yellow());
                    }
                    if character.bio.is_empty() {
                        println!("  {} Bio is empty — agent won't have personality", "⚠".yellow());
                    }
                    if character.plugins.is_empty() {
                        println!("  {} No plugins configured — agent will have no capabilities", "⚠".yellow());
                    }
                }
                Err(e) => {
                    println!("{}", "✗ Invalid".red());
                    println!("  Error: {}", e);
                }
            }
        }
    }

    Ok(())
}
