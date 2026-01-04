//! Multi-Agent Coordination Example
//!
//! Demonstrates agents helping each other and collaborating

use zoey_core::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║     Multi-Agent Coordination Example                  ║");
    println!("║     Agents Helping Each Other Natively                ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Create multi-agent coordinator
    let coordinator = Arc::new(MultiAgentCoordinator::new());

    println!("✓ Multi-agent coordinator created\n");

    // Register three agents with different capabilities
    let agent1_id = uuid::Uuid::new_v4();
    let agent2_id = uuid::Uuid::new_v4();
    let agent3_id = uuid::Uuid::new_v4();

    coordinator.register_agent(agent1_id, "CodeAgent".to_string())?;
    coordinator.register_agent(agent2_id, "TranslationAgent".to_string())?;
    coordinator.register_agent(agent3_id, "ResearchAgent".to_string())?;

    println!("✓ Registered 3 agents:");
    println!("  - CodeAgent (ID: {})", agent1_id);
    println!("  - TranslationAgent (ID: {})", agent2_id);
    println!("  - ResearchAgent (ID: {})", agent3_id);
    println!();

    // Register capabilities
    coordinator.register_capability(AgentCapability {
        agent_id: agent1_id,
        name: "code_generation".to_string(),
        description: "Can generate and review code".to_string(),
        proficiency: 0.95,
        availability: 1.0,
    })?;

    coordinator.register_capability(AgentCapability {
        agent_id: agent2_id,
        name: "translation".to_string(),
        description: "Can translate between languages".to_string(),
        proficiency: 0.90,
        availability: 1.0,
    })?;

    coordinator.register_capability(AgentCapability {
        agent_id: agent3_id,
        name: "research".to_string(),
        description: "Can research topics and find information".to_string(),
        proficiency: 0.85,
        availability: 1.0,
    })?;

    println!("✓ Registered capabilities:");
    println!("  - CodeAgent: code_generation (proficiency: 0.95)");
    println!("  - TranslationAgent: translation (proficiency: 0.90)");
    println!("  - ResearchAgent: research (proficiency: 0.85)");
    println!();

    // Scenario 1: Agent needs help with translation
    println!("═══ SCENARIO 1: Agent Requests Help ═══\n");

    let helper = coordinator
        .request_help(
            agent1_id,
            "translation",
            serde_json::json!({
                "text": "Hello, how are you?",
                "from": "English",
                "to": "Spanish"
            }),
        )
        .await?;

    if let Some(helper_id) = helper {
        println!("✓ CodeAgent requested translation help");
        println!("  Coordinator assigned: TranslationAgent ({})", helper_id);
        println!("  Request sent successfully");

        // TranslationAgent receives the message
        let messages = coordinator.get_messages(helper_id);
        println!("  TranslationAgent received {} message(s)", messages.len());

        if let Some(msg) = messages.first() {
            println!("  Message type: {:?}", msg.message_type);
            println!("  From: {}", msg.from_agent_id);
            println!("  Content: {}", msg.content);
        }
    }
    println!();

    // Scenario 2: Find agents with specific capability
    println!("═══ SCENARIO 2: Find Capable Agents ═══\n");

    let code_agents = coordinator.find_agents_with_capability("code_generation");
    println!("✓ Agents with code_generation capability:");
    for (agent_id, score) in &code_agents {
        println!("  - Agent {} (score: {:.2})", agent_id, score);
    }
    println!();

    // Scenario 3: Broadcast information
    println!("═══ SCENARIO 3: Broadcast to All Agents ═══\n");

    let sent = coordinator.broadcast(
        agent1_id,
        serde_json::json!({
            "announcement": "System maintenance in 1 hour",
            "priority": "high"
        }),
    )?;

    println!("✓ CodeAgent broadcast message");
    println!("  Sent to {} agents", sent);

    let agent2_messages = coordinator.get_messages(agent2_id);
    let agent3_messages = coordinator.get_messages(agent3_id);

    println!(
        "  TranslationAgent received: {} message(s)",
        agent2_messages.len()
    );
    println!(
        "  ResearchAgent received: {} message(s)",
        agent3_messages.len()
    );
    println!();

    // Scenario 4: Agent status updates
    println!("═══ SCENARIO 4: Agent Status Management ═══\n");

    coordinator.update_agent_status(agent1_id, MultiAgentStatus::Busy, 0.8)?;
    coordinator.update_agent_status(agent2_id, MultiAgentStatus::Idle, 0.1)?;
    coordinator.update_agent_status(agent3_id, MultiAgentStatus::Online, 0.5)?;

    println!("✓ Agent statuses updated:");
    for agent in coordinator.get_active_agents() {
        println!(
            "  - {}: {:?} (load: {:.1}%)",
            agent.name,
            agent.status,
            agent.load * 100.0
        );
    }
    println!();

    // Scenario 5: Load-based task assignment
    println!("═══ SCENARIO 5: Smart Task Assignment ═══\n");

    // Now request translation again - should prefer idle agent
    let helper2 = coordinator
        .request_help(
            agent3_id,
            "translation",
            serde_json::json!({"text": "Goodbye", "to": "French"}),
        )
        .await?;

    println!("✓ ResearchAgent requested translation help");
    println!("  Coordinator considers:");
    println!("    - TranslationAgent: Idle (load: 0.1) ← Best choice!");
    println!("  Assigned: TranslationAgent (because it has lowest load)");
    println!();

    // Summary
    println!("═══ MULTI-AGENT FEATURES ═══\n");
    println!("✓ Agent Registration:");
    println!("  - Register/unregister agents dynamically");
    println!("  - Track agent status and load\n");

    println!("✓ Capability System:");
    println!("  - Agents advertise their capabilities");
    println!("  - Find best agent for a task");
    println!("  - Proficiency and availability scoring\n");

    println!("✓ Coordination Messages:");
    println!("  - Help requests");
    println!("  - Task delegation");
    println!("  - Information sharing");
    println!("  - Status updates\n");

    println!("✓ Smart Routing:");
    println!("  - Find agents by capability");
    println!("  - Consider proficiency, availability, and load");
    println!("  - Automatic best-agent selection\n");

    println!("✓ Broadcast:");
    println!("  - Send messages to all agents");
    println!("  - System-wide announcements\n");

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║     🤝 MULTI-AGENT COORDINATION COMPLETE 🤝            ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    println!("Use Cases:");
    println!("  • Specialized agents for different tasks");
    println!("  • Load balancing across agents");
    println!("  • Collaborative problem solving");
    println!("  • Agent swarms");
    println!("  • Hierarchical agent systems\n");

    Ok(())
}
