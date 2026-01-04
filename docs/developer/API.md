# API Reference

## 🔌 Core API

### Creating an Agent Runtime

```rust
use zoey_core::*;

let runtime = AgentRuntime::new(RuntimeOpts {
    character: Some(character),
    adapter: Some(Arc::new(adapter)),
    plugins: vec![
        Arc::new(BootstrapPlugin::new()),
    ],
    ..Default::default()
}).await?;
```

### Processing Messages

```rust
let processor = MessageProcessor::new(runtime);
let message = create_test_memory("Hello!");
let room = create_test_room(ChannelType::Dm);

let responses = processor.process_message(message, room).await?;
```

### Using Multi-Agent Coordination

```rust
let coordinator = Arc::new(MultiAgentCoordinator::new());

// Register agent
coordinator.register_agent(agent_id, "MyAgent".to_string())?;

// Advertise capability
coordinator.register_capability(AgentCapability {
    agent_id,
    name: "coding".to_string(),
    proficiency: 0.95,
    availability: 1.0,
})?;

// Request help
let helper = coordinator.request_help(
    from_agent,
    "coding",
    serde_json::json!({"task": "write_function"})
).await?;
```

### Using Distributed Runtime

```rust
let runtime = DistributedRuntime::new(node_id);

// Register nodes
runtime.register_node(node_info)?;

// Send cross-node message
runtime.send_to_agent(from_agent, to_agent, payload, "type".to_string()).await?;

// Find best node
let best = runtime.find_best_node();
```

---

See source code for complete API documentation (rustdoc).

