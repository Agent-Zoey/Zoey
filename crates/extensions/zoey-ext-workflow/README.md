<p align="center">
  <img src="../../assets/zoey-forest.png" alt="Zoey" width="400" />
</p>

# ⚙️ zoey-ext-workflow

> **Your secrets are safe with Zoey**

Workflow orchestration engine for ZoeyOS—define, execute, and monitor complex multi-step workflows with scheduling, resource management, and conditional branching.

## Status: ✅ Production

---

## Features

### 🔄 Workflow Engine
- Multi-step workflow definitions
- Task dependencies and DAG execution
- Parallel and sequential execution
- Error handling and retries

### 📅 Scheduler
- Cron-based scheduling
- Recurring workflow execution
- Job monitoring and management

### 🖥️ Resource Management
- CPU/GPU quotas
- Memory limits
- Rate limiting and throttling
- Concurrent task limits

### 🔀 Conditional Branching
- If/else conditions
- Switch statements
- Loop constructs
- Dynamic workflow paths

### 🌐 Distributed Execution
- Multi-worker task distribution
- Load balancing
- Worker health monitoring

---

## Quick Start

```rust
use zoey_ext_workflow::{WorkflowPlugin, WorkflowBuilder, Task};

let plugin = WorkflowPlugin::new();

// Build a workflow
let workflow = WorkflowBuilder::new("data_processing")
    .description("Process and analyze data")
    .add_task(Task::with_handler("fetch", |ctx| async {
        Ok(json!({"data": [1, 2, 3]}))
    }))
    .add_task(Task::with_handler("process", |ctx| async {
        let input = ctx.get_input("fetch")?;
        Ok(json!({"processed": true}))
    }).depends_on("fetch"))
    .build()?;

// Execute
let result = plugin.execute(workflow).await?;
println!("Status: {:?}", result.status);
```

---

## Usage Examples

### Basic Workflow

```rust
use zoey_ext_workflow::{WorkflowBuilder, Task};

let workflow = WorkflowBuilder::new("example")
    .description("Example workflow")
    .timeout(3600)
    .parallel(true)
    .add_task(Task::with_handler("step1", |ctx| async {
        Ok(json!({"step": 1}))
    }))
    .add_task(Task::with_handler("step2", |ctx| async {
        let prev = ctx.get_input("step1")?;
        Ok(json!({"step": 2, "prev": prev}))
    }).depends_on("step1"))
    .add_task(Task::with_handler("step3", |ctx| async {
        Ok(json!({"step": 3}))
    }).depends_on("step2"))
    .build()?;

let result = plugin.execute(workflow).await?;
```

### ML Training Pipeline

```rust
let ml_pipeline = WorkflowBuilder::new("ml_training_pipeline")
    .description("Complete ML model training workflow")
    .timeout(7200)
    .add_task(Task::with_handler("fetch_data", |ctx| async {
        let dataset = download_dataset("mnist").await?;
        Ok(json!({
            "dataset_path": "./data/mnist",
            "num_samples": 60000
        }))
    }))
    .add_task(Task::with_handler("train_model", |ctx| async {
        let data_info = ctx.get_input("fetch_data")?;
        train_pytorch_model(&data_info).await?;
        Ok(json!({
            "model_path": "./models/mnist.pt",
            "accuracy": 0.98
        }))
    }).depends_on("fetch_data"))
    .add_task(Task::with_handler("evaluate", |ctx| async {
        let model_info = ctx.get_input("train_model")?;
        let metrics = evaluate_model(&model_info).await?;
        Ok(json!({
            "accuracy": 0.98,
            "precision": 0.97,
            "recall": 0.98
        }))
    }).depends_on("train_model"))
    .add_task(Task::with_handler("deploy", |ctx| async {
        let eval_info = ctx.get_input("evaluate")?;
        deploy_model(&eval_info).await?;
        Ok(json!({"status": "deployed", "endpoint": "/predict"}))
    }).depends_on("evaluate"))
    .build()?;
```

### Scheduled Workflows

```rust
use zoey_ext_workflow::Scheduler;

let scheduler = plugin.scheduler();

// Schedule hourly execution
scheduler.write().await.schedule_cron(
    "hourly_sync",
    workflow_id,
    "0 * * * *",
).await?;

// Schedule daily at 9 AM weekdays
scheduler.write().await.schedule_cron(
    "daily_report",
    workflow_id,
    "0 9 * * 1-5",
).await?;
```

### Distributed Execution

```rust
use zoey_ext_workflow::{DistributedCoordinator, DistributedConfig, DistributedWorker};

let config = DistributedConfig {
    coordinator_host: "localhost".to_string(),
    coordinator_port: 8080,
    num_workers: 4,
    heartbeat_interval_secs: 10,
    enable_load_balancing: true,
    ..Default::default()
};

let coordinator = DistributedCoordinator::new(config).await?;

// Register workers
for i in 0..4 {
    let worker = DistributedWorker::new(
        format!("worker-{}", i),
        "localhost:8080".to_string(),
    );
    coordinator.register_worker(worker).await?;
}

// Execute workflow across workers
let result = coordinator.execute(parallel_workflow).await?;
```

### Resource Management

```rust
use zoey_ext_workflow::{ResourcePool, ResourceLimits, ResourceRequirements};

let pool = ResourcePool::new(ResourceLimits {
    max_cpu_cores: 16.0,
    max_memory_mb: 65536,
    max_gpu_devices: 2,
    max_concurrent_tasks: 10,
});

let workflow = WorkflowBuilder::new("gpu_workflow")
    .add_task(Task::with_handler("gpu_task", |ctx| async {
        run_gpu_inference().await
    }).with_resources(ResourceRequirements {
        cpu_cores: 4.0,
        memory_mb: 16384,
        gpu_devices: 1,
        gpu_memory_mb: 8192,
    }))
    .build()?;

let result = pool.execute(workflow).await?;
```

### Conditional Branching

```rust
use zoey_ext_workflow::{IfBranch, SwitchBranch, LoopBranch, Condition, CompareOp};

// If/else branching
let workflow = WorkflowBuilder::new("conditional")
    .add_task(Task::with_handler("check_quality", |ctx| async {
        Ok(json!({"quality": 0.85}))
    }))
    .add_branch(IfBranch::new(
        "quality_check",
        Condition::compare("quality", CompareOp::GreaterThan, 0.8),
        vec![
            Task::with_handler("high_quality_path", |ctx| async {
                Ok(json!({"path": "high"}))
            })
        ],
        vec![
            Task::with_handler("low_quality_path", |ctx| async {
                Ok(json!({"path": "low"}))
            })
        ]
    ).depends_on("check_quality"))
    .build()?;

// Loop construct
let loop_workflow = WorkflowBuilder::new("training_loop")
    .add_branch(LoopBranch::new(
        "training_loop",
        LoopConfig {
            max_iterations: 10,
            break_condition: Some(Condition::compare("accuracy", CompareOp::GreaterThan, 0.95)),
            ..Default::default()
        },
        vec![
            Task::with_handler("train_epoch", |ctx| async {
                let epoch = ctx.get_value("iteration")?;
                Ok(json!({"accuracy": 0.90 + (epoch as f64 * 0.01)}))
            })
        ]
    ))
    .build()?;
```

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              Workflow Engine                        │
├─────────────────────────────────────────────────────┤
│                                                     │
│  ┌──────────────┐  ┌──────────────┐                │
│  │   Workflows  │  │   Scheduler  │                │
│  │              │  │              │                │
│  │ • Definition │  │ • Cron jobs  │                │
│  │ • Execution  │  │ • Recurring  │                │
│  │ • Monitoring │  │ • Management │                │
│  └──────────────┘  └──────────────┘                │
│          │                │                         │
│          └────────┬───────┘                         │
│                   │                                 │
│  ┌──────────────────────────────────────────┐      │
│  │           Task Execution                  │      │
│  │                                           │      │
│  │ • Dependencies • Retries • Timeouts      │      │
│  │ • Resources   • Conditions • Loops       │      │
│  └──────────────────────────────────────────┘      │
│                   │                                 │
│  ┌──────────────────────────────────────────┐      │
│  │           Distributed Layer               │      │
│  │                                           │      │
│  │ • Workers • Load Balancing • Heartbeats  │      │
│  └──────────────────────────────────────────┘      │
│                                                     │
└─────────────────────────────────────────────────────┘
```

---

## Cron Syntax

```
* * * * *
│ │ │ │ │
│ │ │ │ └── Day of week (0-6, Sun=0)
│ │ │ └──── Month (1-12)
│ │ └────── Day of month (1-31)
│ └──────── Hour (0-23)
└────────── Minute (0-59)
```

Examples:
- `0 * * * *` - Every hour
- `*/15 * * * *` - Every 15 minutes
- `0 9 * * 1-5` - 9 AM weekdays
- `0 0 1 * *` - Midnight on 1st of month

---

## Configuration

```rust
use zoey_ext_workflow::WorkflowPluginConfig;

let config = WorkflowPluginConfig {
    max_concurrent_workflows: 10,
    max_concurrent_tasks: 5,
    default_timeout_secs: 300,
    enable_retry: true,
    max_retries: 3,
    retry_delay_secs: 5,
    enable_persistence: true,
    enable_events: true,
    ..Default::default()
};

let plugin = WorkflowPlugin::with_config(config);
```

---

## Dependencies

- `zoey-core` - Core runtime and types

---

## Testing

```bash
cargo test -p zoey-ext-workflow
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
