use async_trait::async_trait;
use zoey_core::{types::*, Result};
use zoey_plugin_knowledge::graph::KnowledgeGraph;
use zoey_plugin_knowledge::retrieval::{HybridRetriever, SearchResult};
use std::sync::Arc;

pub struct SummarizationEvaluator;
pub struct LongTermExtractionEvaluator;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[async_trait]
impl Evaluator for SummarizationEvaluator {
    fn name(&self) -> &str {
        "memory_summarization"
    }
    fn description(&self) -> &str {
        "Generates session summaries when thresholds are met"
    }

    async fn validate(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        let threshold = env_usize("MEMORY_SUMMARIZATION_THRESHOLD", 16);
        let interval = env_usize("MEMORY_SUMMARIZATION_INTERVAL", 10);
        let retain_recent = env_usize("MEMORY_RETAIN_RECENT", 10);

        let rt_ref = zoey_core::runtime_ref::downcast_runtime_ref(&runtime)
            .ok_or_else(|| zoey_core::ZoeyError::other("Runtime unavailable"))?;
        if let Some(rt) = rt_ref.try_upgrade() {
            // Capture adapter and drop lock before await
            let (adapter_opt, room_id) = {
                let r = rt.read().unwrap();
                (r.get_adapter(), _message.room_id)
            };
            if let Some(adapter) = adapter_opt {
                let q = MemoryQuery {
                    room_id: Some(room_id),
                    count: None,
                    table_name: "messages".to_string(),
                    ..Default::default()
                };
                if let Ok(total) = adapter.count_memories(q).await {
                    return Ok(
                        total >= threshold && (total.saturating_sub(retain_recent)) % interval == 0
                    );
                }
            }
        }
        Ok(false)
    }

    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _did_respond: bool,
        _responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        let retain_recent = env_usize("MEMORY_RETAIN_RECENT", 10);
        let rt_ref = zoey_core::runtime_ref::downcast_runtime_ref(&runtime)
            .ok_or_else(|| zoey_core::ZoeyError::other("Runtime unavailable"))?;

        if let Some(rt) = rt_ref.try_upgrade() {
            let (adapter_opt, agent_id, room_id) = {
                let r = rt.read().unwrap();
                (r.get_adapter(), r.agent_id, message.room_id)
            };
            if let Some(adapter) = adapter_opt {
                let q = MemoryQuery {
                    room_id: Some(room_id),
                    count: Some(retain_recent),
                    table_name: "messages".to_string(),
                    ..Default::default()
                };
                let recent = adapter.get_memories(q).await.unwrap_or_default();

                // Use hybrid retriever + reranker to find key sentences/topics
                let corpus: Vec<String> = recent.iter().map(|m| m.content.text.clone()).collect();
                let retriever =
                    HybridRetriever::new(KnowledgeGraph::new("session"), corpus.clone());
                let query = format!(
                    "Summarize main points and topics from: {}",
                    corpus.join(" ")
                );
                let results = retriever.search(&query, 8).await.unwrap_or_default();
                let key_points = results
                    .iter()
                    .map(|r| format!("- {}", r.text))
                    .collect::<Vec<_>>()
                    .join("\n");
                let summary_text = if key_points.is_empty() {
                    "No recent messages".to_string()
                } else {
                    format!("Session summary:\n{}", key_points)
                };

                // Simple topic extraction: top keywords by frequency
                let mut topics: Vec<String> = results.iter().map(|r| r.text.clone()).collect();
                topics.truncate(8);

                // Store summary
                let mem = Memory {
                    id: uuid::Uuid::new_v4(),
                    entity_id: agent_id,
                    agent_id,
                    room_id,
                    content: Content {
                        text: summary_text,
                        source: Some("session_summary".to_string()),
                        ..Default::default()
                    },
                    embedding: None,
                    metadata: Some(MemoryMetadata {
                        memory_type: Some("session_summary".to_string()),
                        entity_name: None,
                        data: {
                            let mut m = std::collections::HashMap::new();
                            m.insert("topics".to_string(), serde_json::json!(topics));
                            m
                        },
                    }),
                    created_at: chrono::Utc::now().timestamp(),
                    unique: Some(true),
                    similarity: None,
                };
                let _ = adapter.create_memory(&mem, "session_summaries").await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Evaluator for LongTermExtractionEvaluator {
    fn name(&self) -> &str {
        "long_term_extraction"
    }
    fn description(&self) -> &str {
        "Extracts and stores persistent facts into long-term memory"
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        let text = message.content.text.to_lowercase();
        let triggers = [
            "remember",
            "keep in mind",
            "don\'t forget",
            "note that",
            "save this",
        ];
        Ok(triggers.iter().any(|t| text.contains(t)))
    }

    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _did_respond: bool,
        _responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        let confidence_threshold = env_f64("MEMORY_CONFIDENCE_THRESHOLD", 0.85);
        let rt_ref = zoey_core::runtime_ref::downcast_runtime_ref(&runtime)
            .ok_or_else(|| zoey_core::ZoeyError::other("Runtime unavailable"))?;

        if let Some(rt) = rt_ref.try_upgrade() {
            let (adapter_opt, agent_id, room_id, entity_id) = {
                let r = rt.read().unwrap();
                (
                    r.get_adapter(),
                    r.agent_id,
                    message.room_id,
                    message.entity_id,
                )
            };
            if let Some(adapter) = adapter_opt {
                let text = &message.content.text;
                let cat = if text.to_lowercase().contains("prefer")
                    || text.to_lowercase().contains("love")
                {
                    crate::service::LongTermMemoryCategory::Semantic
                } else if text.to_lowercase().contains("working on")
                    || text.to_lowercase().contains("in q")
                    || text.to_lowercase().contains("last week")
                {
                    crate::service::LongTermMemoryCategory::Episodic
                } else if text.to_lowercase().contains("always")
                    || text.to_lowercase().contains("typically")
                    || text.to_lowercase().contains("my workflow")
                {
                    crate::service::LongTermMemoryCategory::Procedural
                } else {
                    crate::service::LongTermMemoryCategory::Semantic
                };

                let mem = Memory {
                    id: uuid::Uuid::new_v4(),
                    entity_id,
                    agent_id,
                    room_id,
                    content: Content {
                        text: text.clone(),
                        source: Some("long_term".to_string()),
                        ..Default::default()
                    },
                    embedding: None,
                    metadata: Some(MemoryMetadata {
                        memory_type: Some("long_term".to_string()),
                        entity_name: None,
                        data: {
                            let mut m = std::collections::HashMap::new();
                            m.insert("category".to_string(), serde_json::json!(cat.as_str()));
                            m.insert(
                                "confidence".to_string(),
                                serde_json::json!(confidence_threshold),
                            );
                            m.insert("source".to_string(), serde_json::json!("manual"));
                            m
                        },
                    }),
                    created_at: chrono::Utc::now().timestamp(),
                    unique: Some(true),
                    similarity: None,
                };
                let _ = adapter.create_memory(&mem, "long_term_memories").await;
            }
        }
        Ok(())
    }
}
