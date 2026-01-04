use crate::runtime::legacy::LockRecovery;
use crate::types::{EmbeddingPriority, Task, TaskWorker};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct EmbeddingWorker;

impl EmbeddingWorker {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskWorker for EmbeddingWorker {
    fn task_type(&self) -> &str {
        "embedding_generation"
    }

    async fn execute(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        task: &Task,
    ) -> Result<()> {
        if let Some(rt) = runtime.downcast_ref::<crate::runtime::legacy::AgentRuntime>() {
            if let Some(memory_id) = task.data.get("memory_id").and_then(|v| v.as_str()) {
                if let Some(adapter) = crate::runtime::RuntimeState::get_adapter(rt) {
                    let memories = adapter
                        .get_memories(crate::types::MemoryQuery {
                            table_name: "messages".to_string(),
                            ..Default::default()
                        })
                        .await?;
                    if let Some(mem) = memories.into_iter().find(|m| m.id.to_string() == memory_id)
                    {
                        let _ = rt
                            .queue_embedding_generation(&mem, EmbeddingPriority::Normal)
                            .await;
                    }
                }
            }
        }
        Ok(())
    }
}
