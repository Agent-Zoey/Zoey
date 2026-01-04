//! Local Infrastructure Example
//!
//! Demonstrates the new local-first infrastructure:
//! 1. Local vector database (no PostgreSQL required)
//! 2. Automatic hardware detection and optimization
//! 3. Model routing based on task type and hardware
//!
//! This example shows how to build a fully offline, privacy-first AI agent
//! that automatically optimizes for your hardware.

use zoey_core::{
    runtime::{AgentRuntime, RuntimeOpts},
    types::*,
    Character, Result,
};
use zoey_plugin_bootstrap::BootstrapPlugin;
use zoey_plugin_hardware::HardwarePlugin;
use zoey_provider_local::LocalLLMPlugin;
use zoey_storage_vector::LocalVectorPlugin;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("=== Local Infrastructure Demo ===");
    info!("");

    // Step 1: Initialize hardware detection
    info!("Step 1: Detecting hardware...");
    let mut hardware_plugin = HardwarePlugin::new();
    hardware_plugin.initialize().await?;

    let hardware_info = hardware_plugin
        .get_hardware_info()
        .expect("Hardware should be detected during initialization");

    info!("");

    // Step 2: Get model recommendations
    info!("Step 2: Getting model recommendations based on hardware...");
    let recommendations = hardware_plugin.get_model_recommendations().await?;

    info!("Top 5 recommended models:");
    for (i, rec) in recommendations.iter().take(5).enumerate() {
        info!(
            "  {}. {} ({}) - {:.1}GB, {} context, backend: {}",
            i + 1,
            rec.model_name,
            rec.size_category,
            rec.estimated_memory_gb,
            rec.context_length,
            rec.recommended_backend
        );
    }

    info!("");

    // Step 3: Get optimization configuration
    info!("Step 3: Generating optimization configuration...");
    let opt_config = hardware_plugin.get_optimization_config().await?;

    info!("Optimization Configuration:");
    info!("  Backend: {}", opt_config.backend);
    info!("  Use GPU: {}", opt_config.use_gpu);
    if let Some(gpu_backend) = &opt_config.gpu_backend {
        info!("  GPU Backend: {}", gpu_backend);
    }
    info!("  Threads: {}", opt_config.num_threads);
    info!("  Max Context: {}", opt_config.max_context_length);
    info!("  Batch Size: {}", opt_config.batch_size);
    info!("  GPU Layers: {}", opt_config.gpu_layers);

    info!("");

    // Step 4: Initialize model router
    info!("Step 4: Initializing model router...");
    let hardware_constraints = HardwareConstraints {
        available_memory_gb: hardware_info.available_memory_gb,
        has_gpu: hardware_info.gpu.is_some(),
        max_context_length: opt_config.max_context_length,
    };

    let router = ModelRouter::new(hardware_constraints);

    // Demonstrate routing for different tasks
    info!("Routing examples:");

    let tasks = vec![
        (TaskType::Chat, RoutingPreference::Balanced),
        (TaskType::CodeGeneration, RoutingPreference::Quality),
        (TaskType::Summarization, RoutingPreference::Speed),
        (TaskType::Embedding, RoutingPreference::Speed),
    ];

    for (task, preference) in tasks {
        if let Ok(model) = router.route(task, preference) {
            info!(
                "  {:?} ({:?}) -> {} ({}, {})",
                task, preference, model.name, model.size_category, model.speed_rating as i32
            );
        }
    }

    info!("");

    // Step 5: Initialize local vector database
    info!("Step 5: Initializing local vector database...");
    let vector_plugin = LocalVectorPlugin::new("./data/vectors")?;

    // Add some test embeddings
    let test_embeddings = vec![
        (
            UUID::new_v4(),
            vec![0.1; 1536], // Dummy embedding
        ),
        (
            UUID::new_v4(),
            vec![0.2; 1536], // Dummy embedding
        ),
    ];

    for (id, embedding) in test_embeddings {
        vector_plugin.add_embedding(id, embedding).await?;
    }

    let stats = vector_plugin.stats().await;
    info!("Vector Database Stats:");
    info!("  Total vectors: {}", stats.total_vectors);
    info!("  Dimension: {}", stats.dimension);
    info!("  Index type: {}", stats.index_type);

    info!("");

    // Step 6: Create a character configuration
    info!("Step 6: Creating agent with local infrastructure...");

    let optimal_backend = hardware_plugin.get_optimal_backend().await?;
    let optimal_model = if let Ok(model) = router.route(TaskType::Chat, RoutingPreference::Balanced)
    {
        model.name.clone()
    } else {
        "phi3:mini".to_string()
    };

    let character = Character {
        name: "LocalAI".to_string(),
        bio: vec![
            "I am a privacy-first AI assistant running entirely on local hardware.".to_string(),
            "I use automatic hardware detection to optimize my performance.".to_string(),
            "I route tasks to the optimal local model for your hardware.".to_string(),
        ],
        lore: vec![],
        topics: vec![
            "privacy".to_string(),
            "local AI".to_string(),
            "hardware optimization".to_string(),
        ],
        adjectives: vec![
            "private".to_string(),
            "local".to_string(),
            "optimized".to_string(),
        ],
        style: Style {
            all: vec![
                "Be concise and helpful".to_string(),
                "Respect user privacy".to_string(),
            ],
            chat: vec!["Use friendly language".to_string()],
            post: vec![],
        },
        knowledge: vec![],
        message_examples: vec![],
        post_examples: vec![],
        settings: Settings {
            model: optimal_model.clone(),
            embeddings_model: Some("nomic-embed-text".to_string()),
            ..Default::default()
        },
        system_prompt: Some(format!(
            "You are LocalAI, running on {} backend with model {}. \
            You operate entirely offline for maximum privacy. \
            Available memory: {:.1}GB, GPU: {}",
            optimal_backend,
            optimal_model,
            hardware_info.available_memory_gb,
            if hardware_info.gpu.is_some() {
                "Yes"
            } else {
                "No"
            }
        )),
        ..Default::default()
    };

    info!("Agent Configuration:");
    info!("  Name: {}", character.name);
    info!("  Model: {}", character.settings.model);
    info!(
        "  Embeddings: {}",
        character
            .settings
            .embeddings_model
            .as_deref()
            .unwrap_or("none")
    );

    info!("");

    // Step 7: Initialize plugins
    info!("Step 7: Initializing agent runtime...");

    let plugins: Vec<Arc<dyn Plugin>> = vec![
        Arc::new(BootstrapPlugin::new()),
        Arc::new(LocalLLMPlugin::new()),
        Arc::new(hardware_plugin),
    ];

    // Create runtime options
    let opts = RuntimeOpts {
        agent_id: Some(UUID::new_v4()),
        character: Some(character),
        plugins: plugins.clone(),
        adapter: None, // Using local vector DB, no SQL needed
        settings: None,
        conversation_length: Some(opt_config.max_context_length),
        all_available_plugins: Some(plugins),
    };

    // Create agent runtime
    let runtime = AgentRuntime::new(opts).await?;

    info!("Agent runtime created successfully!");
    info!("");

    // Summary
    info!("=== Summary ===");
    info!("✓ Hardware detection complete");
    info!("✓ Model routing configured");
    info!("✓ Local vector database initialized");
    info!("✓ Agent runtime created");
    info!("");
    info!("Your agent is now running with:");
    info!("  - No PostgreSQL dependency (local vector DB)");
    info!("  - Automatic hardware optimization");
    info!("  - Smart model routing for each task type");
    info!("  - 100% offline operation for maximum privacy");
    info!("");
    info!("Ready for inference!");

    Ok(())
}
