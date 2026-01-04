use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

#[derive(Default)]
pub struct ConversationReviewEvaluator;

#[async_trait]
impl Evaluator for ConversationReviewEvaluator {
    fn name(&self) -> &str {
        "conversation_review"
    }
    fn description(&self) -> &str {
        "Reviews agent interactions with a small LLM and records feedback for always-on learning"
    }
    fn always_run(&self) -> bool {
        false
    }

    async fn validate(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = match downcast_runtime_ref(&runtime) {
            Some(r) => r,
            None => return Ok(false),
        };
        if let Some(rt) = rt_ref.try_upgrade() {
            let fast = rt
                .read()
                .unwrap()
                .get_setting("ui:fast_mode")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if fast {
                return Ok(false);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        did_respond: bool,
        responses: Option<Vec<Memory>>,
    ) -> Result<()> {
        if !did_respond {
            return Ok(());
        }
        let Some(resp) = responses.as_ref().and_then(|v| v.first()) else {
            return Ok(());
        };

        use zoey_core::runtime_ref::downcast_runtime_ref;
        let rt_ref = match downcast_runtime_ref(&runtime) {
            Some(r) => r,
            None => return Ok(()),
        };
        let Some(rt) = rt_ref.try_upgrade() else {
            return Ok(());
        };

        // Build compact review prompt
        let prompt = format!(
            "Evaluate the assistant reply. Return JSON: {{\"score\":0..1,\"note\":\"string\"}}\nUser: {}\nAssistant: {}",
            message.content.text,
            resp.content.text,
        );

        // Call smallest available text model
        let handler = {
            let r = rt.read().unwrap();
            let models = r.get_models();
            models.get("TEXT_SMALL").and_then(|v| v.first().cloned())
        };
        if let Some(h) = handler {
            let params = GenerateTextParams {
                prompt,
                model: None,
                temperature: Some(0.2),
                max_tokens: Some(128),
                top_p: None,
                stop: None,
                frequency_penalty: None,
                presence_penalty: None,
            };
            let mh_params = ModelHandlerParams {
                runtime: runtime.clone(),
                params,
            };
            if let Ok(raw) = (h.handler)(mh_params).await {
                // Parse JSON
                let mut score: Option<f32> = None;
                let mut note: Option<String> = None;
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    score = v.get("score").and_then(|s| s.as_f64()).map(|f| f as f32);
                    note = v
                        .get("note")
                        .and_then(|n| n.as_str().map(|s| s.to_string()));
                }

                // Persist review signal into runtime settings keyed by response ID
                if let Some(s) = score {
                    let key_score = format!("training:review:{}:score", resp.id);
                    let key_note = format!("training:review:{}:note", resp.id);
                    let mut guard = rt.write().unwrap();
                    guard.set_setting(&key_score, serde_json::json!(s), false);
                    if let Some(n) = note {
                        guard.set_setting(&key_note, serde_json::json!(n), false);
                    }
                }
            }
        }
        // Trigger immediate training collector review application if available
        {
            use zoey_core::training::TrainingCollector;
            // Attempt to find the training sample id attached to response metadata
            if let Some(meta) = &resp.metadata {
                if let Some(v) = meta.data.get("training_sample_id").and_then(|v| v.as_str()) {
                    if let Ok(sample_uuid) = uuid::Uuid::parse_str(v) {
                        // Read back review score stored in settings
                        let key_score = format!("training:review:{}:score", resp.id);
                        let key_note = format!("training:review:{}:note", resp.id);
                        let (score_opt, note_opt) = {
                            let r = rt.read().unwrap();
                            let s = r
                                .get_setting(&key_score)
                                .and_then(|v| v.as_f64())
                                .map(|f| f as f32);
                            let n = r
                                .get_setting(&key_note)
                                .and_then(|v| v.as_str().map(|s| s.to_string()));
                            (s, n)
                        };
                        if let Some(score) = score_opt {
                            let runtime_ref = zoey_core::runtime_ref::RuntimeRef::new(&rt);
                            let collector = {
                                // No direct access to collector from here; rely on message pipeline to apply
                                None as Option<Arc<TrainingCollector>>
                            };
                            let _ = (sample_uuid, score, note_opt, runtime_ref);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_conversation_review_evaluator_runs() {
        let evaluator = ConversationReviewEvaluator::default();
        assert_eq!(evaluator.name(), "conversation_review");
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Hello".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };
        let response = Memory {
            content: Content {
                text: "Hi there".to_string(),
                ..Default::default()
            },
            ..message.clone()
        };
        let state = State::new();
        let result = evaluator
            .handler(Arc::new(()), &message, &state, true, Some(vec![response]))
            .await;
        assert!(result.is_ok());
    }
}
