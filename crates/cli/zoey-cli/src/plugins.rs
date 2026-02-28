//! Plugin introspection — list and inspect loaded plugins.

use std::sync::{Arc, RwLock};

use colored::Colorize;

use zoey_core::AgentRuntime;
use crate::PluginAction;

pub fn handle(action: &PluginAction, runtime: &Arc<RwLock<AgentRuntime>>) -> anyhow::Result<()> {
    let rt = runtime.read().unwrap();
    let providers = rt.get_providers();
    let actions = rt.get_actions();
    let evaluators = rt.get_evaluators();
    let services_count = rt.get_services_count();

    match action {
        PluginAction::List => {
            println!("{}", "Loaded plugins:".cyan());
            println!();

            println!("  {} ({}):", "Providers".bold(), providers.len());
            for p in &providers {
                println!("    {} {}", "•".cyan(), p.name());
            }

            println!();
            println!("  {} ({}):", "Actions".bold(), actions.len());
            for a in &actions {
                println!("    {} {} — {}", "•".cyan(), a.name().bold(), a.description().dimmed());
            }

            println!();
            println!("  {} ({}):", "Evaluators".bold(), evaluators.len());
            for e in &evaluators {
                println!("    {} {}", "•".cyan(), e.name());
            }

            println!();
            println!("  {} {}", "Services:".bold(), services_count);
        }

        PluginAction::Info { name } => {
            let name_lower = name.to_lowercase();

            // Search in providers
            if let Some(p) = providers.iter().find(|p| p.name().to_lowercase().contains(&name_lower)) {
                println!("{}: {}", "Provider".cyan(), p.name().bold());
                println!("{}: provider", "Type".cyan());
                return Ok(());
            }

            // Search in actions
            if let Some(a) = actions.iter().find(|a| a.name().to_lowercase().contains(&name_lower)) {
                println!("{}: {}", "Action".cyan(), a.name().bold());
                println!("{}: {}", "Description".cyan(), a.description());
                println!("{}: action", "Type".cyan());
                return Ok(());
            }

            // Search in evaluators
            if let Some(e) = evaluators.iter().find(|e| e.name().to_lowercase().contains(&name_lower)) {
                println!("{}: {}", "Evaluator".cyan(), e.name().bold());
                println!("{}: evaluator", "Type".cyan());
                return Ok(());
            }

            println!("{} Plugin '{}' not found. Use `zoey plugin list` to see all.", "✗".red(), name);
        }
    }

    Ok(())
}
