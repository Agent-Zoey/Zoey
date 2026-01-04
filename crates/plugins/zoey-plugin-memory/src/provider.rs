use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

pub struct ContextMemoriesProvider;

impl Default for ContextMemoriesProvider {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for ContextMemoriesProvider {
    fn name(&self) -> &str {
        "context_memories"
    }
    fn description(&self) -> Option<String> {
        Some("Injects curated memories into State for prompt context".to_string())
    }
    fn dynamic(&self) -> bool {
        true
    }

    async fn get(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<ProviderResult> {
        let mut result = ProviderResult::default();

        let rt_ref = zoey_core::runtime_ref::downcast_runtime_ref(&runtime)
            .ok_or_else(|| zoey_core::ZoeyError::other("Runtime unavailable"))?;

        if let Some(rt) = rt_ref.try_upgrade() {
            let (adapter_opt, agent_id) = {
                let r = rt.read().unwrap();
                (r.get_adapter(), r.agent_id)
            };

            let mut texts: Vec<String> = Vec::new();

            if let Some(adapter) = adapter_opt {
                let recent_query = MemoryQuery {
                    room_id: Some(message.room_id),
                    count: Some(12),
                    table_name: "messages".to_string(),
                    ..Default::default()
                };
                if let Ok(recent) = adapter.get_memories(recent_query).await {
                    for m in recent {
                        texts.push(format!("- {}", m.content.text));
                    }
                }

                if let Some(embedding) = &message.embedding {
                    let search = SearchMemoriesParams {
                        table_name: "memories".to_string(),
                        agent_id: Some(agent_id),
                        room_id: Some(message.room_id),
                        world_id: None,
                        entity_id: Some(message.entity_id),
                        embedding: embedding.clone(),
                        count: 8,
                        unique: Some(true),
                        threshold: Some(0.30),
                    };
                    if let Ok(cands) = adapter.search_memories_by_embedding(search).await {
                        for m in cands {
                            texts.push(format!("* {}", m.content.text));
                        }
                    }
                }
            }

            let joined = if texts.is_empty() {
                "No prior memories".to_string()
            } else {
                texts.join("\n")
            };
            result.values = Some({
                let mut v = std::collections::HashMap::new();
                v.insert("RELEVANT_MEMORIES".to_string(), joined.clone());
                v
            });
            result.text = Some(format!("Relevant Memories:\n{}", joined));
        }

        Ok(result)
    }
    fn capabilities(&self) -> Option<Vec<String>> {
        Some(vec!["CHAT".to_string()])
    }
}
