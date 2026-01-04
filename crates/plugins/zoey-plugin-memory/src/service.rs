use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTier {
    Ephemeral,
    Session,
    LongTerm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalMode {
    Hybrid,
    SemanticOnly,
    KeywordOnly,
}

#[derive(Debug, Clone)]
pub struct MemoryPolicy {
    pub max_session_messages: usize,
    pub context_budget_tokens: usize,
    pub importance_threshold: f32,
    pub forgetting_halflife_days: f64,
    pub retrieval_mode: RetrievalMode,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            max_session_messages: 32,
            context_budget_tokens: 2000,
            importance_threshold: 0.4,
            forgetting_halflife_days: 30.0,
            retrieval_mode: RetrievalMode::Hybrid,
        }
    }
}

pub struct TieredMemoryService {
    runtime: Option<Arc<dyn std::any::Any + Send + Sync>>,
    policy: MemoryPolicy,
    running: bool,
}

impl TieredMemoryService {
    pub fn new(policy: MemoryPolicy) -> Self {
        Self {
            runtime: None,
            policy,
            running: false,
        }
    }

    fn score_recency(days: f64) -> f32 {
        (-(days / 14.0)).exp() as f32
    }
    fn score_importance(user_rating: Option<f64>, access_count: Option<usize>) -> f32 {
        let r = (user_rating.unwrap_or(0.5) * 0.7) as f32;
        let a = ((access_count.unwrap_or(0) as f32).min(10.0) / 10.0) * 0.3;
        r + a
    }

    pub async fn pick_memories_for_context(&self, message: &Memory) -> Result<Vec<Memory>> {
        let mut selected: Vec<Memory> = Vec::new();
        if let Some(rt_any) = &self.runtime {
            if let Some(rt) = zoey_core::runtime_ref::downcast_runtime_ref(rt_any) {
                if let Some(runtime) = rt.try_upgrade() {
                    let r = runtime.read().unwrap();

                    if let Some(adapter) = r.get_adapter() {
                        let recent_query = MemoryQuery {
                            room_id: Some(message.room_id),
                            count: Some(self.policy.max_session_messages),
                            table_name: "messages".to_string(),
                            ..Default::default()
                        };
                        if let Ok(mut recent) = adapter.get_memories(recent_query).await {
                            selected.append(&mut recent);
                        }

                        if let Some(embedding) = &message.embedding {
                            let search = SearchMemoriesParams {
                                table_name: "memories".to_string(),
                                agent_id: Some(r.agent_id),
                                room_id: Some(message.room_id),
                                world_id: None,
                                entity_id: Some(message.entity_id),
                                embedding: embedding.clone(),
                                count: 16,
                                unique: Some(true),
                                threshold: Some(0.25),
                            };
                            if let Ok(mut sem) = adapter.search_memories_by_embedding(search).await
                            {
                                selected.append(&mut sem);
                            }
                        }
                    }

                    // Reranking with recency + importance heuristic
                    selected.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                    let recency_weight = 0.6f32;
                    let importance_weight = 0.4f32;
                    selected.sort_by(|a, b| {
                        let age_days_a =
                            (chrono::Utc::now().timestamp() - a.created_at) as f64 / 86400.0;
                        let age_days_b =
                            (chrono::Utc::now().timestamp() - b.created_at) as f64 / 86400.0;
                        let rec_a = Self::score_recency(age_days_a);
                        let rec_b = Self::score_recency(age_days_b);
                        let imp_a = Self::score_importance(
                            a.metadata
                                .as_ref()
                                .and_then(|m| m.data.get("user_rating"))
                                .and_then(|v| v.as_f64()),
                            a.metadata
                                .as_ref()
                                .and_then(|m| m.data.get("access_count"))
                                .and_then(|v| v.as_u64())
                                .map(|u| u as usize),
                        );
                        let imp_b = Self::score_importance(
                            b.metadata
                                .as_ref()
                                .and_then(|m| m.data.get("user_rating"))
                                .and_then(|v| v.as_f64()),
                            b.metadata
                                .as_ref()
                                .and_then(|m| m.data.get("access_count"))
                                .and_then(|v| v.as_u64())
                                .map(|u| u as usize),
                        );
                        let s_a = recency_weight * rec_a + importance_weight * imp_a;
                        let s_b = recency_weight * rec_b + importance_weight * imp_b;
                        s_b.partial_cmp(&s_a).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let mut budget = zoey_core::TokenBudget::new(
                        self.policy.context_budget_tokens,
                        self.policy.context_budget_tokens / 4,
                        zoey_core::TokenCounter::estimate_conversation_tokens(&selected),
                        0,
                    );
                    while budget.is_exceeded && !selected.is_empty() {
                        selected.pop();
                        budget = zoey_core::TokenBudget::new(
                            self.policy.context_budget_tokens,
                            self.policy.context_budget_tokens / 4,
                            zoey_core::TokenCounter::estimate_conversation_tokens(&selected),
                            0,
                        );
                    }
                }
            }
        }
        Ok(selected)
    }

    pub async fn store_long_term_memory(
        &self,
        agent_id: UUID,
        entity_id: UUID,
        category: LongTermMemoryCategory,
        content: String,
        confidence: f64,
        source: &str,
    ) -> Result<UUID> {
        if let Some(rt_any) = &self.runtime {
            if let Some(rt) = zoey_core::runtime_ref::downcast_runtime_ref(rt_any) {
                if let Some(runtime) = rt.try_upgrade() {
                    let r = runtime.read().unwrap();
                    if let Some(adapter) = r.get_adapter() {
                        let mem = Memory {
                            id: uuid::Uuid::new_v4(),
                            entity_id,
                            agent_id,
                            room_id: uuid::Uuid::new_v4(),
                            content: Content {
                                text: content,
                                source: Some(source.to_string()),
                                ..Default::default()
                            },
                            embedding: None,
                            metadata: Some(MemoryMetadata {
                                memory_type: Some("long_term".to_string()),
                                entity_name: None,
                                data: {
                                    let mut m = std::collections::HashMap::new();
                                    m.insert(
                                        "category".to_string(),
                                        serde_json::json!(category.as_str()),
                                    );
                                    m.insert(
                                        "confidence".to_string(),
                                        serde_json::json!(confidence),
                                    );
                                    m
                                },
                            }),
                            created_at: chrono::Utc::now().timestamp_millis(),
                            unique: Some(true),
                            similarity: None,
                        };
                        return adapter.create_memory(&mem, "long_term_memories").await;
                    }
                }
            }
        }
        Err(zoey_core::ZoeyError::other("Runtime/adapter unavailable"))
    }

    pub async fn get_long_term_memories(&self, entity_id: UUID) -> Result<Vec<Memory>> {
        if let Some(rt_any) = &self.runtime {
            if let Some(rt) = zoey_core::runtime_ref::downcast_runtime_ref(rt_any) {
                if let Some(runtime) = rt.try_upgrade() {
                    let r = runtime.read().unwrap();
                    if let Some(adapter) = r.get_adapter() {
                        let q = MemoryQuery {
                            entity_id: Some(entity_id),
                            count: Some(50),
                            table_name: "long_term_memories".to_string(),
                            ..Default::default()
                        };
                        return adapter.get_memories(q).await;
                    }
                }
            }
        }
        Ok(vec![])
    }

    pub async fn store_session_summary(
        &self,
        room_id: UUID,
        summary: String,
        topics: Vec<String>,
    ) -> Result<UUID> {
        if let Some(rt_any) = &self.runtime {
            if let Some(rt) = zoey_core::runtime_ref::downcast_runtime_ref(rt_any) {
                if let Some(runtime) = rt.try_upgrade() {
                    let r = runtime.read().unwrap();
                    if let Some(adapter) = r.get_adapter() {
                        let mem = Memory {
                            id: uuid::Uuid::new_v4(),
                            entity_id: r.agent_id,
                            agent_id: r.agent_id,
                            room_id,
                            content: Content {
                                text: summary,
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
                        return adapter.create_memory(&mem, "session_summaries").await;
                    }
                }
            }
        }
        Err(zoey_core::ZoeyError::other("Runtime/adapter unavailable"))
    }

    pub async fn get_session_summaries(&self, room_id: UUID) -> Result<Vec<Memory>> {
        if let Some(rt_any) = &self.runtime {
            if let Some(rt) = zoey_core::runtime_ref::downcast_runtime_ref(rt_any) {
                if let Some(runtime) = rt.try_upgrade() {
                    let r = runtime.read().unwrap();
                    if let Some(adapter) = r.get_adapter() {
                        let q = MemoryQuery {
                            room_id: Some(room_id),
                            count: Some(10),
                            table_name: "session_summaries".to_string(),
                            ..Default::default()
                        };
                        return adapter.get_memories(q).await;
                    }
                }
            }
        }
        Ok(vec![])
    }

    pub async fn log_access(
        &self,
        agent_id: UUID,
        entity_id: Option<UUID>,
        room_id: UUID,
        operation: &str,
        metadata: serde_json::Value,
    ) -> Result<UUID> {
        if let Some(rt_any) = &self.runtime {
            if let Some(rt) = zoey_core::runtime_ref::downcast_runtime_ref(rt_any) {
                if let Some(runtime) = rt.try_upgrade() {
                    let r = runtime.read().unwrap();
                    if let Some(adapter) = r.get_adapter() {
                        let log = Memory {
                            id: uuid::Uuid::new_v4(),
                            entity_id: entity_id.unwrap_or(agent_id),
                            agent_id,
                            room_id,
                            content: Content {
                                text: operation.to_string(),
                                source: Some("memory_access".to_string()),
                                ..Default::default()
                            },
                            embedding: None,
                            metadata: Some(MemoryMetadata {
                                memory_type: Some("memory_access".to_string()),
                                entity_name: None,
                                data: metadata
                                    .as_object()
                                    .map(|m| m.clone().into_iter().collect())
                                    .unwrap_or_default(),
                            }),
                            created_at: chrono::Utc::now().timestamp(),
                            unique: Some(false),
                            similarity: None,
                        };
                        return adapter.create_memory(&log, "memory_access_logs").await;
                    }
                }
            }
        }
        Err(zoey_core::ZoeyError::other("Runtime/adapter unavailable"))
    }
}

#[async_trait]
impl Service for TieredMemoryService {
    fn service_type(&self) -> &str {
        "memory_manager"
    }

    async fn initialize(&mut self, runtime: Arc<dyn std::any::Any + Send + Sync>) -> Result<()> {
        self.runtime = Some(runtime);
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        self.running = true;
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.running
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongTermMemoryCategory {
    Episodic,
    Semantic,
    Procedural,
}

impl LongTermMemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Episodic => "episodic",
            Self::Semantic => "semantic",
            Self::Procedural => "procedural",
        }
    }
}
