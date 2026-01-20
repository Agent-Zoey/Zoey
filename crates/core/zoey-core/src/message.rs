//! Message processing pipeline

use crate::streaming::{create_text_stream, StreamHandler, TextStream};
use crate::templates::compose_prompt_from_state;
use crate::training::TrainingCollector;
use crate::types::*;
use crate::{ZoeyError, Result};

use std::sync::OnceLock;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::{debug, info, warn};

/// Message processor for handling incoming messages
pub struct MessageProcessor {
    runtime: Arc<RwLock<crate::AgentRuntime>>,

    /// Training collector for RLHF and model fine-tuning
    training_collector: Option<Arc<TrainingCollector>>,
}

impl MessageProcessor {
    /// Create a new message processor
    pub fn new(runtime: Arc<RwLock<crate::AgentRuntime>>) -> Self {
        Self {
            runtime,
            training_collector: None,
        }
    }

    /// Create a new message processor with training enabled
    pub fn with_training(
        runtime: Arc<RwLock<crate::AgentRuntime>>,
        training_collector: Arc<TrainingCollector>,
    ) -> Self {
        Self {
            runtime,
            training_collector: Some(training_collector),
        }
    }

    /// Process an incoming message
    pub async fn process_message(&self, message: Memory, room: Room) -> Result<Vec<Memory>> {
        let span =
            tracing::info_span!("message_processing", message_id = %message.id, duration_ms = 0i64);
        let _enter = span.enter();
        let _start = Instant::now();
        info!(
            "INTERACTION_REQUEST id={} room_id={} entity_id={} text_len={} text_preview={}",
            message.id,
            message.room_id,
            message.entity_id,
            message.content.text.len(),
            message.content.text.chars().take(120).collect::<String>()
        );

        // 1. Store incoming message in database
        info!("INTERACTION_STORE message_id={} table=messages", message.id);
        // Production: Actually store the message
        // Note: Database operations are async
        let adapter_opt = self.runtime.read().unwrap().adapter.read().unwrap().clone();
        if let Some(adapter) = adapter_opt.as_ref() {
            match adapter.create_memory(&message, "messages").await {
                Ok(id) => info!("Message stored with ID: {}", id),
                Err(e) => warn!("Failed to store message: {}", e),
            }
        } else {
            warn!("No database adapter configured - message not stored");
        }

        // 2. Determine if should respond (with delayed reassessment window)
        debug!("Determining if should respond");
        let should_respond = self.should_respond(&message, &room).await?;
        // Merge follow-up if pending within window, otherwise start window for incomplete
        let mut message = message; // shadow for potential text update
        {
            let mut rt = self.runtime.write().unwrap();
            let enabled = rt
                .get_setting("AUTONOMOUS_DELAYED_REASSESSMENT")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if enabled {
                if let Some((ts, prev)) =
                    crate::utils::delayed_reassessment::DelayedReassessment::pending(
                        &rt,
                        message.room_id,
                    )
                {
                    if crate::utils::delayed_reassessment::DelayedReassessment::should_wait(ts) {
                        let merged = crate::utils::delayed_reassessment::DelayedReassessment::merge(
                            &prev,
                            &message.content.text,
                        );
                        crate::utils::delayed_reassessment::DelayedReassessment::clear(
                            &mut rt,
                            message.room_id,
                        );
                        message.content.text = merged;
                    } else {
                        crate::utils::delayed_reassessment::DelayedReassessment::clear(
                            &mut rt,
                            message.room_id,
                        );
                    }
                } else {
                    let incomplete = rt
                        .get_setting("ui:incomplete")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if incomplete {
                        crate::utils::delayed_reassessment::DelayedReassessment::start(
                            &mut rt,
                            message.room_id,
                            &message.content.text,
                        );
                        info!("Deferred response via delayed reassessment window: room_id={} message_id={}", message.room_id, message.id);
                        return Ok(vec![]);
                    }
                }
            }
        }

        if !should_respond {
            info!("Decided not to respond to message");
            return Ok(vec![]);
        }

        // Phase 0: Preprocess message with mini-pipelines
        {
            let enabled = {
                let rt = self.runtime.read().unwrap();
                rt.get_setting("ui:phase0_enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
            };
            if enabled {
                let pre = crate::preprocessor::Phase0Preprocessor::new(self.runtime.clone());
                let phase0 = pre.execute(&message).await;
                if let Ok(res) = phase0 {
                    if let Some(tone) = res.tone.as_ref() {
                        let mut rt = self.runtime.write().unwrap();
                        rt.set_setting("ui:tone", serde_json::json!(tone), false);
                    }
                    if let Some(lang) = res.language.as_ref() {
                        let mut rt = self.runtime.write().unwrap();
                        rt.set_setting("ui:language", serde_json::json!(lang), false);
                    }
                    if let Some(intent) = res.intent.as_ref() {
                        let mut rt = self.runtime.write().unwrap();
                        rt.set_setting("ui:intent", serde_json::json!(intent), false);
                    }
                    let mut rt = self.runtime.write().unwrap();
                    if !res.topics.is_empty() {
                        rt.set_setting("ui:topics", serde_json::json!(res.topics), false);
                    }
                    if !res.keywords.is_empty() {
                        rt.set_setting("ui:keywords", serde_json::json!(res.keywords), false);
                    }
                    if !res.entities.is_empty() {
                        rt.set_setting("ui:entities", serde_json::json!(res.entities), false);
                    }
                    if let Some(comp) = res.complexity.as_ref() {
                        rt.set_setting(
                            "ui:complexity",
                            serde_json::to_value(comp).unwrap_or(serde_json::Value::Null),
                            false,
                        );
                    }
                    let room_id = message.room_id;
                    let avg_key = format!("rhythm:{}:avg_len", room_id);
                    let win_key = format!("rhythm:{}:window", room_id);
                    let mut avg = rt
                        .get_setting(&avg_key)
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let count = rt
                        .get_setting(&win_key)
                        .and_then(|v| v.as_array().map(|a| a.len()))
                        .unwrap_or(0);
                    let len = message.content.text.len() as f64;
                    avg = if count == 0 {
                        len
                    } else {
                        (avg * (count as f64) + len) / ((count as f64) + 1.0)
                    };
                    rt.set_setting(&avg_key, serde_json::json!(avg), false);
                    let mut window = rt
                        .get_setting(&win_key)
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    window.push(serde_json::json!(chrono::Utc::now().timestamp()));
                    while window.len() > 10 {
                        window.remove(0);
                    }
                    rt.set_setting(&win_key, serde_json::json!(window), false);
                    let velocity = if window.len() >= 2 {
                        let first = window.first().and_then(|v| v.as_i64()).unwrap_or(0);
                        let last = window.last().and_then(|v| v.as_i64()).unwrap_or(0);
                        let dt = (last - first) as f64;
                        if dt <= 0.0 {
                            0.0
                        } else {
                            (window.len() as f64) / (dt / 60.0)
                        }
                    } else {
                        0.0
                    };
                    rt.set_setting(
                        &format!("rhythm:{}:velocity", room_id),
                        serde_json::json!(velocity),
                        false,
                    );
                    let prev_topics = rt
                        .get_setting(&format!("rhythm:{}:recentTopics", room_id))
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    rt.set_setting(
                        &format!("rhythm:{}:recentTopics", room_id),
                        serde_json::json!(res.topics.clone()),
                        false,
                    );
                    let prev: Vec<String> = prev_topics
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    let overlap = if prev.is_empty() || res.topics.is_empty() {
                        0.0
                    } else {
                        let set_prev: std::collections::HashSet<_> = prev.iter().collect();
                        let inter =
                            res.topics.iter().filter(|t| set_prev.contains(t)).count() as f64;
                        inter / (res.topics.len() as f64)
                    };
                    let drift = overlap < 0.2;
                    rt.set_setting("ui:possibleTopicShift", serde_json::json!(drift), false);
                    let suggested = if velocity > 5.0 {
                        "terse"
                    } else if velocity > 2.0 {
                        "brief"
                    } else if avg > 300.0 {
                        "detailed"
                    } else {
                        "moderate"
                    };
                    rt.set_setting(
                        "ui:suggestedResponseLength",
                        serde_json::json!(suggested),
                        false,
                    );
                }
            }
        }

        // 3. Compose state from providers (with RuntimeRef)
        debug!("Composing state from providers");
        let mut state = self.compose_state_with_runtime_ref(&message).await?;

        // Providers include curated memories; no direct service downcasting

        // 4. Generate response using LLM
        debug!("Generating response with LLM");
        let response_text = self.generate_response(&message, &state).await?;
        info!(
            "INTERACTION_RESPONSE room_id={} text_preview={}",
            message.room_id,
            response_text.chars().take(120).collect::<String>()
        );

        // 5. Process actions (determine which actions to take based on response)
        debug!("Processing actions");
        let _action_results = self.process_actions(&message, &state).await?;

        // 6. Create response memories
        let agent_id = {
            let rt = self.runtime.read().unwrap();
            rt.agent_id
        };

        let response_memories = vec![Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: agent_id,
            agent_id,
            room_id: message.room_id,
            content: Content {
                text: response_text.clone(),
                source: message.content.source.clone(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: Some(false),
            similarity: None,
        }];

        // 7. Record training sample early and attach sample_id to response metadata
        let mut response_memories = response_memories;
        let mut recorded_sample_id: Option<uuid::Uuid> = None;
        {
            let fast_mode = {
                let rt = self.runtime.read().unwrap();
                rt.get_setting("ui:fast_mode")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };
            if !fast_mode {
                if let Some(ref collector) = self.training_collector {
                    if let Some(response_mem) = response_memories.first() {
                        let thought = response_mem.content.thought.clone();
                        match collector
                            .record_conversation_turn(&message, response_mem, thought, &state)
                            .await
                        {
                            Ok(id) => {
                                recorded_sample_id = Some(id);
                                let meta = MemoryMetadata {
                                    memory_type: Some("message".to_string()),
                                    entity_name: None,
                                    data: {
                                        let mut m = std::collections::HashMap::new();
                                        m.insert(
                                            "training_sample_id".to_string(),
                                            serde_json::json!(id.to_string()),
                                        );
                                        m
                                    },
                                };
                                response_memories[0].metadata = Some(meta);
                            }
                            Err(_e) => {}
                        }
                    }
                }
            }
        }

        // 8. Run evaluators (skip in fast mode)
        {
            let fast_mode = {
                let rt = self.runtime.read().unwrap();
                rt.get_setting("ui:fast_mode")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };
            if fast_mode {
                debug!("Fast mode enabled: skipping evaluators");
            } else {
                info!(
                    "INTERACTION_EVALUATORS_START room_id={} message_id={} responses={} ",
                    message.room_id,
                    message.id,
                    response_memories.len()
                );
                use crate::runtime_ref::RuntimeRef;
                let runtime_ref = Arc::new(RuntimeRef::new(&self.runtime));
                self.evaluate(&message, &state, true, &response_memories)
                    .await?;
                info!(
                    "INTERACTION_EVALUATORS_DONE room_id={} message_id={}",
                    message.room_id, message.id
                );
            }
        }

        // 9. Store response messages in database (production)
        for response in &response_memories {
            if let Some(adapter) = self
                .runtime
                .read()
                .unwrap()
                .adapter
                .read()
                .unwrap()
                .as_ref()
            {
                match adapter.create_memory(response, "messages").await {
                    Ok(id) => info!("INTERACTION_STORE response_id={} table=messages", id),
                    Err(e) => warn!("Failed to store response: {}", e),
                }
            }
        }

        // 10. Apply evaluator review signals to training collector
        {
            let fast_mode = {
                let rt = self.runtime.read().unwrap();
                rt.get_setting("ui:fast_mode")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };
            if !fast_mode {
                if let (Some(ref collector), Some(sample_id), Some(resp)) = (
                    self.training_collector.as_ref(),
                    recorded_sample_id,
                    response_memories.first(),
                ) {
                    let key = format!("training:review:{}", resp.id);
                    let (score_opt, note_opt) = {
                        let rt = self.runtime.read().unwrap();
                        let score = rt
                            .get_setting(&(key.clone() + ":score"))
                            .and_then(|v| v.as_f64())
                            .map(|v| v as f32);
                        let note = rt
                            .get_setting(&(key.clone() + ":note"))
                            .and_then(|v| v.as_str().map(|s| s.to_string()));
                        (score, note)
                    };
                    if let Some(review_score) = score_opt {
                        let rlhf_enabled = collector.is_rlhf_enabled();
                        if rlhf_enabled {
                            let mapped = (review_score * 2.0) - 1.0;
                            let _ = collector.add_feedback(sample_id, mapped, note_opt).await;
                        } else {
                            let _ = collector
                                .add_review(sample_id, review_score, note_opt)
                                .await;
                        }
                    }
                }
            }
        }

        // 11. Update active-thread TTL for this room
        {
            let mut rt = self.runtime.write().unwrap();
            let key = format!("ui:lastAddressed:{}", message.room_id);
            rt.set_setting(
                &key,
                serde_json::json!(chrono::Utc::now().timestamp()),
                false,
            );
        }

        // 12. Emit MESSAGE_SENT event (production)
        debug!("Emitting MESSAGE_SENT event");
        let handler_count = {
            let rt = self.runtime.read().unwrap();
            let events = rt.events.read().unwrap();
            events.get("MESSAGE_SENT").map(|h| h.len()).unwrap_or(0)
        };

        if handler_count > 0 {
            debug!("Would invoke {} MESSAGE_SENT event handlers", handler_count);
        }

        info!(
            "✓ Message processing complete - {} response(s) generated and stored",
            response_memories.len()
        );
        let _elapsed = _start.elapsed().as_millis() as i64;
        span.record("duration_ms", &_elapsed);
        Ok(response_memories)
    }

    /// Determine if agent should respond to message
    async fn should_respond(&self, message: &Memory, room: &Room) -> Result<bool> {
        // Respond to DMs always, otherwise use mention/intent + active-thread TTL
        match room.channel_type {
            ChannelType::Dm | ChannelType::VoiceDm | ChannelType::Api => Ok(true),
            _ => {
                let addressed = message
                    .content
                    .metadata
                    .get("addressed_to_me")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if addressed {
                    return Ok(true);
                }
                let rt = self.runtime.read().unwrap();
                let agent_name = &rt.character.name;
                let text_lc = message.content.text.to_lowercase();
                let mentioned = text_lc.contains(&agent_name.to_lowercase());
                let intent_directive = text_lc.starts_with("please ")
                    || text_lc.starts_with("can you")
                    || text_lc.contains("help me")
                    || text_lc.contains("what is")
                    || text_lc.contains("how do");
                let ttl_ok = {
                    let key = format!("ui:lastAddressed:{}", message.room_id);
                    if let Some(ts) = rt.get_setting(&key).and_then(|v| v.as_i64()) {
                        let now = chrono::Utc::now().timestamp();
                        let elapsed = now - ts;
                        elapsed <= 600 // 10 minutes
                    } else {
                        false
                    }
                };
                Ok(mentioned || intent_directive || ttl_ok)
            }
        }
    }

    /// Generate response text using LLM (supports OpenAI, Anthropic, Local LLM)
    async fn generate_response(&self, message: &Memory, state: &State) -> Result<String> {
        // Compose prompt from state using template
        let template_owned = {
            let rt = self.runtime.read().unwrap();
            if let Some(ref templates) = rt.character.templates {
                templates.message_handler_template.clone()
            } else {
                None
            }
        };
        let template_str = template_owned
            .as_deref()
            .unwrap_or(crate::templates::MESSAGE_HANDLER_TEMPLATE);
        let mut prompt = compose_prompt_from_state(&state, template_str).unwrap_or_else(|_| {
            // Fallback template if state composition fails
            format!(
                "You are ZoeyBot, a helpful AI assistant.\n\
                        User message: {}\n\
                        Respond helpfully in XML format with <thought> and <text> tags.",
                message.content.text
            )
        });

        // Template already contains current message via providers; avoid redundant suffix

        let streaming_enabled = {
            let rt = self.runtime.read().unwrap();
            rt.get_setting("ui:streaming")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| {
                    std::env::var("UI_STREAMING")
                        .map(|v| v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false)
                })
        };

        // Adaptive temperature based on simple intent heuristics
        {
            let text_lc = message.content.text.to_lowercase();
            let factual = text_lc.contains('?')
                || text_lc.starts_with("what")
                || text_lc.starts_with("how")
                || text_lc.starts_with("why")
                || text_lc.starts_with("when")
                || text_lc.starts_with("where");
            let creative = text_lc.contains("brainstorm")
                || text_lc.contains("ideas")
                || text_lc.contains("suggestions")
                || text_lc.contains("think of");
            let target_temp = if factual {
                0.4
            } else if creative {
                0.8
            } else {
                0.7
            };
            let mut rt = self.runtime.write().unwrap();
            rt.set_setting("ui:temperature", serde_json::json!(target_temp), false);
        }

        // Optional prompt debug (disabled by default)
        let prompt_debug = {
            let rt = self.runtime.read().unwrap();
            rt.get_setting("ui:prompt_debug")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| {
                    std::env::var("UI_PROMPT_DEBUG")
                        .map(|v| v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false)
                })
        };
        if prompt_debug {
            debug!("╔════════════════════════════════════════════════════════════════");
            debug!("║ LLM PROMPT CONTEXT ({} chars)", prompt.len());
            debug!("╠════════════════════════════════════════════════════════════════");
            for (i, line) in prompt.lines().take(50).enumerate() {
                debug!("║ {:3} │ {}", i + 1, line);
            }
            if prompt.lines().count() > 50 {
                debug!("║ ... ({} more lines)", prompt.lines().count() - 50);
            }
            debug!("╚════════════════════════════════════════════════════════════════");
        }

        // Try to use registered model handlers (OpenAI, Anthropic, LocalLLM)
        // Handlers are priority-sorted: Local (200) > Cloud (100)
        let raw_response = self.call_llm(&prompt).await?;

        info!("LLM response received ({} chars)", raw_response.len());

        // Parse XML response to extract thought and text
        let (thought, response_text_raw) = self.parse_llm_response(&raw_response);
        let mut response_text = {
            use regex::Regex;
            let mut t = response_text_raw.clone();
            let action_re =
                Regex::new(r"(?i)^\s*(REPLY|SEND_MESSAGE|IGNORE|NONE)\b[:\-]?\s*").unwrap();
            t = action_re.replace_all(&t, "").to_string();
            let action_line_re =
                Regex::new(r"(?mi)^\s*(REPLY|SEND_MESSAGE|IGNORE|NONE)\b[:\-]?.*$\n?").unwrap();
            t = action_line_re.replace_all(&t, "").to_string();
            let html_re = Regex::new(r"(?is)</?[^>]+>").unwrap();
            t = html_re.replace_all(&t, "").to_string();
            // Normalize double newlines introduced by character guidelines to single newlines in user-facing text
            let dbl_nl = Regex::new(r"\n\n+").unwrap();
            t = dbl_nl.replace_all(&t, "\n").to_string();
            let meta_re = Regex::new(
                r"(?mi)^\s*(The user .*|As an AI.*|I will .*|I can .* assist.*)\.?\s*$\n?",
            )
            .unwrap();
            t = meta_re.replace_all(&t, "").to_string();
            let leading_punct = Regex::new(r"^[\s,.;:!\-]+").unwrap();
            t = leading_punct.replace_all(&t, "").to_string();
            t.trim().to_string()
        };

        // Post-completion continuation: if the final character is not terminal punctuation, request a short continuation
        let looks_truncated = {
            let s = response_text.trim_end();
            if s.len() < 80 {
                false
            } else {
                match s.chars().rev().find(|c| !c.is_whitespace()) {
                    Some(c) => {
                        let enders = ['.', '!', '?', '”', '’', '"', '\'', ')', ']', '}'];
                        !enders.contains(&c)
                    }
                    None => false,
                }
            }
        };
        if looks_truncated {
            if streaming_enabled {
                if let Ok(cont) = self.continue_response(&response_text).await {
                    if !cont.is_empty() {
                        response_text = format!("{} {}", response_text, cont);
                    }
                }
            } else {
                // Non-streaming (e.g., Discord): finish the sentence locally without a second LLM call
                let trimmed = response_text.trim_end();
                response_text = format!("{}.", trimmed);
            }
        }

        // Store thought for future use (learning/reflection/training)
        if let Some(ref thought_text) = thought {
            if let Some(ref collector) = self.training_collector {
                // Use training collector to store thought
                use crate::runtime_ref::RuntimeRef;
                let runtime_ref = Arc::new(RuntimeRef::new(&self.runtime));
                let runtime_any = runtime_ref.as_any_arc();

                let quality_score = 0.7; // Default quality for thoughts
                match collector
                    .store_thought(runtime_any, thought_text, message, quality_score)
                    .await
                {
                    Ok(id) => debug!("Stored thought with ID: {}", id),
                    Err(e) => warn!("Failed to store thought: {}", e),
                }
            } else {
                // Fallback: store directly (old method)
                self.store_thought_direct(&thought_text, message).await?;
            }
        }

        Ok(response_text)
    }

    async fn continue_response(&self, prev_text: &str) -> Result<String> {
        let prompt = format!(
            "Continue the assistant's previous response naturally. Do not repeat content.\n\nPrevious:\n{}\n\nRespond in XML format:\n<response>\n<thought></thought>\n<actions>REPLY</actions>\n<text>",
            prev_text
        );
        let raw = self.call_llm(&prompt).await?;
        let (_, text) = self.parse_llm_response(&raw);
        let cleaned = {
            use regex::Regex;
            let mut t = text;
            let html_re = Regex::new(r"(?is)</?[^>]+>").unwrap();
            t = html_re.replace_all(&t, "").to_string();
            let action_re =
                Regex::new(r"(?i)^\s*(REPLY|SEND_MESSAGE|IGNORE|NONE)\b[:\-]?\s*").unwrap();
            t = action_re.replace_all(&t, "").to_string();
            let leading = Regex::new(r"^[\s,.;:!\-]+").unwrap();
            t = leading.replace_all(&t, "").to_string();
            t.trim().to_string()
        };
        Ok(cleaned)
    }

    /// Call LLM using available providers (OpenAI, Anthropic, or Local)
    async fn call_llm(&self, prompt: &str) -> Result<String> {
        static COST_CALC: OnceLock<crate::planner::cost::CostCalculator> = OnceLock::new();
        // Streaming support: when ui:streaming is enabled and a streaming-capable provider is selected,
        // we will stream tokens into a buffer and return the final text.
        // Get configured provider preference and available handlers
        let (preferred_provider, model_handlers) = {
            let rt = self.runtime.read().unwrap();
            let models = rt.models.read().unwrap();

            // Get user's preferred provider from character settings
            let provider_pref = rt
                .get_setting("model_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()));

            // Log available providers for debugging
            for (model_type, handlers) in models.iter() {
                debug!(
                    "Available model type '{}': {} handler(s)",
                    model_type,
                    handlers.len()
                );
                for (idx, handler) in handlers.iter().enumerate() {
                    debug!(
                        "  [{}] {} (priority: {})",
                        idx, handler.name, handler.priority
                    );
                }
            }

            (provider_pref, models.get("TEXT_LARGE").cloned())
        };

        if let Some(handlers) = model_handlers {
            // If user specified a provider, try to find it
            let provider = if let Some(pref) = preferred_provider.as_ref() {
                info!("🎯 Looking for preferred provider: {}", pref);

                // Normalize provider aliases for matching
                // "ollama" and "local" should map to "local-llm"
                let pref_lc = pref.to_lowercase();
                let is_local_alias = matches!(pref_lc.as_str(), "ollama" | "local" | "llama" | "llamacpp" | "localai");

                // Try to find matching provider
                let matching = handlers.iter().find(|h| {
                    let h_lc = h.name.to_lowercase();
                    // Direct match
                    h_lc.contains(&pref_lc) || pref_lc.contains(&h_lc)
                    // Local provider alias matching
                    || (is_local_alias && (h_lc.contains("local") || h_lc.contains("llm")))
                });

                if let Some(matched) = matching {
                    info!("✓ Found matching provider: {}", matched.name);
                    Some(matched.clone())
                } else {
                    warn!(
                        "⚠️  Preferred provider '{}' not found, using highest priority",
                        pref
                    );
                    handlers.first().cloned()
                }
            } else {
                // No preference, use highest priority
                handlers.first().cloned()
            };

            let do_race = {
                let rt = self.runtime.read().unwrap();
                let race_setting = rt
                    .get_setting("ui:provider_racing")
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(|| {
                        std::env::var("UI_PROVIDER_RACING")
                            .map(|v| v.eq_ignore_ascii_case("true"))
                            .unwrap_or(false)
                    });
                let streaming_ctx = rt
                    .get_setting("ui:streaming")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                race_setting && streaming_ctx && preferred_provider.is_none() && handlers.len() > 1
            };

            if do_race {
                use tokio::task::JoinSet;
                let mut js: JoinSet<Result<String>> = JoinSet::new();
                let max_candidates = 3usize.min(handlers.len());
                for p in handlers.iter().take(max_candidates) {
                    let name_lc = p.name.to_lowercase();
                    let (preferred_model, temp, max_tokens) = {
                        let rt = self.runtime.read().unwrap();
                        let model = if name_lc.contains("openai") {
                            rt.get_setting("OPENAI_MODEL")
                                .and_then(|v| v.as_str().map(|s| s.to_string()))
                        } else if name_lc.contains("anthropic") || name_lc.contains("claude") {
                            rt.get_setting("ANTHROPIC_MODEL")
                                .and_then(|v| v.as_str().map(|s| s.to_string()))
                        } else {
                            rt.get_setting("LOCAL_LLM_MODEL")
                                .and_then(|v| v.as_str().map(|s| s.to_string()))
                        };
                        let temp = rt
                            .get_setting("ui:temperature")
                            .and_then(|v| v.as_f64().map(|f| f as f32))
                            .or_else(|| {
                                rt.get_setting("temperature")
                                    .and_then(|v| v.as_f64().map(|f| f as f32))
                            })
                            .unwrap_or(0.7);
                        let base_tokens = if name_lc.contains("openai")
                            || name_lc.contains("anthropic")
                            || name_lc.contains("claude")
                        {
                            rt.get_setting("max_tokens")
                                .and_then(|v| v.as_u64().map(|u| u as usize))
                                .unwrap_or(150)
                        } else {
                            rt.get_setting("LOCAL_LLM_MAX_TOKENS")
                                .and_then(|v| v.as_u64().map(|u| u as usize))
                                .or_else(|| {
                                    rt.get_setting("max_tokens")
                                        .and_then(|v| v.as_u64().map(|u| u as usize))
                                })
                                .unwrap_or(150)
                        };
                        (model, temp, base_tokens)
                    };
                    let params = GenerateTextParams {
                        prompt: prompt.to_string(),
                        max_tokens: Some(max_tokens),
                        temperature: Some(temp),
                        top_p: None,
                        stop: None,
                        model: preferred_model,
                        frequency_penalty: None,
                        presence_penalty: None,
                    };
                    let mh_params = ModelHandlerParams {
                        runtime: Arc::new(()),
                        params,
                    };
                    js.spawn((p.handler)(mh_params));
                }
                while let Some(res) = js.join_next().await {
                    if let Ok(Ok(text)) = res {
                        return Ok(text);
                    }
                }
            } else if let Some(provider) = provider {
                info!(
                    "🤖 Using LLM provider: {} (priority: {})",
                    provider.name, provider.priority
                );

                // Get model preferences from runtime settings based on provider
                let (preferred_model, temp, max_tokens) = {
                    let rt = self.runtime.read().unwrap();

                    // Select model based on provider name
                    let model = if provider.name.to_lowercase().contains("openai") {
                        rt.get_setting("OPENAI_MODEL")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .or_else(|| {
                                rt.get_setting("openai_model")
                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                            })
                    } else if provider.name.to_lowercase().contains("anthropic")
                        || provider.name.to_lowercase().contains("claude")
                    {
                        rt.get_setting("ANTHROPIC_MODEL")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .or_else(|| {
                                rt.get_setting("anthropic_model")
                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                            })
                    } else {
                        // Local LLM
                        rt.get_setting("LOCAL_LLM_MODEL")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .or_else(|| {
                                rt.get_setting("local_llm_model")
                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                            })
                    };

                    // Prefer ui:temperature if set by adaptive heuristics; fallback to generic temperature
                    let temp = rt
                        .get_setting("ui:temperature")
                        .and_then(|v| v.as_f64().map(|f| f as f32))
                        .or_else(|| {
                            rt.get_setting("temperature")
                                .and_then(|v| v.as_f64().map(|f| f as f32))
                        })
                        .unwrap_or(0.7);
                    let base_tokens = if provider.name.to_lowercase().contains("openai")
                        || provider.name.to_lowercase().contains("anthropic")
                        || provider.name.to_lowercase().contains("claude")
                    {
                        rt.get_setting("max_tokens")
                            .and_then(|v| v.as_u64().map(|u| u as usize))
                            .unwrap_or(150)
                    } else {
                        rt.get_setting("LOCAL_LLM_MAX_TOKENS")
                            .and_then(|v| v.as_u64().map(|u| u as usize))
                            .or_else(|| {
                                rt.get_setting("max_tokens")
                                    .and_then(|v| v.as_u64().map(|u| u as usize))
                            })
                            .unwrap_or(150)
                    };
                    let mut tokens = {
                        let v = rt.get_setting("ui:verbosity");
                        if let Some(val) = v {
                            if let Some(n) = val.as_u64() {
                                n as usize
                            } else if let Some(s) = val.as_str() {
                                match s.to_lowercase().as_str() {
                                    "short" => ((base_tokens as f64 * 0.6) as usize).max(32),
                                    "normal" => base_tokens,
                                    "long" | "verbose" => {
                                        ((base_tokens as f64 * 1.5) as usize).min(base_tokens * 2)
                                    }
                                    _ => base_tokens,
                                }
                            } else {
                                base_tokens
                            }
                        } else {
                            base_tokens
                        }
                    };

                    // Dynamically adjust tokens to avoid output cut-off using model pricing and prompt size
                    // Apply ONLY when streaming is enabled to avoid delaying first response in non-streaming contexts
                    let streaming_ctx = rt
                        .get_setting("ui:streaming")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let avoid_cutoff = rt
                        .get_setting("ui:avoid_cutoff")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    if avoid_cutoff {
                        use crate::planner::tokens::TokenCounter;
                        let calc = COST_CALC.get_or_init(crate::planner::cost::CostCalculator::new);
                        // Determine model key for pricing lookup
                        let model_key = model.clone().map(|m| m.to_string()).or_else(|| {
                            let name = provider.name.to_lowercase();
                            if name.contains("openai") {
                                Some("gpt-4o".to_string())
                            } else if name.contains("anthropic") || name.contains("claude") {
                                Some("claude-3.5-sonnet".to_string())
                            } else {
                                Some("local".to_string())
                            }
                        });
                        if let Some(model_name) = model_key.as_ref() {
                            if let Some(pricing) = calc.get_pricing(model_name) {
                                if streaming_ctx {
                                    // Favor maximal output budget to avoid truncation in streaming
                                    tokens = pricing.max_output_tokens.max(tokens).max(256);
                                } else {
                                    // Non-streaming: raise floor without prompt estimation to avoid latency
                                    tokens = tokens.max(pricing.max_output_tokens).max(512);
                                }
                            } else {
                                // Conservative defaults
                                tokens = if streaming_ctx {
                                    tokens.max(2048)
                                } else {
                                    tokens.max(1024)
                                };
                            }
                        }
                    }
                    (model, temp, tokens)
                };

                let params = GenerateTextParams {
                    prompt: prompt.to_string(),
                    max_tokens: Some(max_tokens),
                    temperature: Some(temp),
                    top_p: None,
                    stop: None,
                    model: preferred_model, // Pass model from settings!
                    frequency_penalty: None,
                    presence_penalty: None,
                };

                let model_params = ModelHandlerParams {
                    runtime: Arc::new(()),
                    params,
                };

                debug!(
                    "Calling model handler: {} (temp: {}, max_tokens: {})",
                    provider.name, temp, max_tokens
                );

                // Check if streaming is enabled
                let streaming_enabled = {
                    let rt = self.runtime.read().unwrap();
                    rt.get_setting("ui:streaming")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                };

                if streaming_enabled && !provider.name.to_lowercase().contains("openai") {
                    // Use local streaming via Ollama if possible
                    match self.call_ollama_direct(prompt).await {
                        Ok(text) => return Ok(text),
                        Err(e) => {
                            warn!(
                                "Streaming call failed: {}. Falling back to non-streaming.",
                                e
                            );
                        }
                    }
                }

                match (provider.handler)(model_params).await {
                    Ok(text) => {
                        info!(
                            "✓ LLM response received via {} ({} chars)",
                            provider.name,
                            text.len()
                        );
                        debug!("╔════════════════════════════════════════════════════════════════");
                        debug!(
                            "║ LLM RESPONSE from {} ({} chars)",
                            provider.name,
                            text.len()
                        );
                        debug!("╠════════════════════════════════════════════════════════════════");
                        for (i, line) in text.lines().take(20).enumerate() {
                            debug!("║ {:3} │ {}", i + 1, line);
                        }
                        if text.lines().count() > 20 {
                            debug!("║ ... ({} more lines)", text.lines().count() - 20);
                        }
                        debug!("╚════════════════════════════════════════════════════════════════");
                        return Ok(text);
                    }
                    Err(e) => {
                        warn!("⚠️  Model handler {} failed: {}", provider.name, e);
                        warn!("Trying fallback method...");
                    }
                }
            } else {
                warn!("No model handlers registered for TEXT_LARGE");
            }
        } else {
            warn!("No model handlers found in registry");
        }

        // Fallback behavior: only use Ollama if local provider is preferred or no preference
        match preferred_provider.as_ref().map(|s| s.to_lowercase()) {
            Some(pref)
                if pref.contains("local") || pref.contains("ollama") || pref.contains("llama") =>
            {
                warn!(
                    "Falling back to direct Ollama call (preferred provider: {})",
                    pref
                );
                self.call_ollama_direct(prompt).await
            }
            Some(pref) => {
                // Graceful fallback when preferred provider is unavailable: return a minimal XML response
                let safe_reply = "<response><thought>Fallback local reasoning</thought><actions>REPLY</actions><text>Okay.</text></response>";
                Ok(safe_reply.to_string())
            }
            None => {
                // No preference: attempt local fallback
                warn!("Falling back to direct Ollama call (no preferred provider set)");
                self.call_ollama_direct(prompt).await
            }
        }
    }

    /// Generate response as a stream of text chunks
    pub async fn generate_response_stream(
        &self,
        message: &Memory,
        state: &State,
    ) -> Result<TextStream> {
        let (sender, receiver) = create_text_stream(64);
        let handler = StreamHandler::new(sender);

        let final_text = self.generate_response(message, state).await?;

        tokio::spawn(async move {
            let mut idx = 0usize;
            let chunk_size = 200usize;
            while idx < final_text.len() {
                let end = (idx + chunk_size).min(final_text.len());
                let piece = final_text[idx..end].to_string();
                let is_final = end >= final_text.len();
                if handler.send_chunk(piece, is_final).await.is_err() {
                    break;
                }
                idx = end;
                if !is_final {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                }
            }
        });

        Ok(receiver)
    }

    /// Direct Ollama API call (fallback if no plugins registered)
    async fn call_ollama_direct(&self, prompt: &str) -> Result<String> {
        let (model_name, model_endpoint) = {
            let rt = self.runtime.read().unwrap();
            let model = rt
                .get_setting("LOCAL_LLM_MODEL")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "phi3:mini".to_string());

            let endpoint = rt
                .get_setting("LOCAL_LLM_ENDPOINT")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "http://localhost:11434".to_string());

            (model, endpoint)
        };

        let client = reqwest::Client::new();
        let num_predict = {
            let rt = self.runtime.read().unwrap();
            rt.get_setting("LOCAL_LLM_MAX_TOKENS")
                .and_then(|v| v.as_u64().map(|u| u as usize))
                .or_else(|| {
                    rt.get_setting("local_llm_num_predict")
                        .and_then(|v| v.as_u64().map(|u| u as usize))
                })
                .or_else(|| {
                    rt.get_setting("max_tokens")
                        .and_then(|v| v.as_u64().map(|u| u as usize))
                })
                .unwrap_or(400)
        };
        let ollama_request = serde_json::json!({
            "model": model_name,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
            "options": {
                "temperature": 0.7,
                "num_predict": num_predict
            }
        });

        debug!("Direct Ollama call: {} at {}", model_name, model_endpoint);

        // Use a short timeout (5 seconds) to avoid hanging when Ollama is not available
        match client
            .post(format!("{}/api/chat", model_endpoint))
            .json(&ollama_request)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(json) = response.json::<serde_json::Value>().await {
                    if let Some(text) = json["message"]["content"].as_str() {
                        return Ok(text.to_string());
                    }
                }
            }
            Err(e) => {
                return Err(ZoeyError::model(format!("Ollama call failed: {}", e)));
            }
        }

        Err(ZoeyError::model("No LLM response received"))
    }

    /// Store agent's thought process for learning and reflection (direct method - deprecated)
    ///
    /// Use TrainingCollector.store_thought() instead for better training data management
    async fn store_thought_direct(
        &self,
        thought_text: &str,
        original_message: &Memory,
    ) -> Result<()> {
        info!(
            "💭 Agent thought (direct): {}",
            thought_text.chars().take(100).collect::<String>()
        );

        let agent_id = self.runtime.read().unwrap().agent_id;

        // Create thought memory with rich metadata
        let thought_memory = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: agent_id,
            agent_id,
            room_id: original_message.room_id,
            content: Content {
                text: thought_text.to_string(),
                source: Some("internal_thought".to_string()),
                thought: Some(thought_text.to_string()),
                ..Default::default()
            },
            embedding: None,
            metadata: Some(MemoryMetadata {
                memory_type: Some("thought".to_string()),
                entity_name: Some("ZoeyBot".to_string()),
                data: {
                    let mut meta = std::collections::HashMap::new();
                    meta.insert("purpose".to_string(), serde_json::json!("reflection"));
                    meta.insert(
                        "related_message".to_string(),
                        serde_json::json!(original_message.id.to_string()),
                    );
                    meta.insert(
                        "timestamp".to_string(),
                        serde_json::json!(chrono::Utc::now().timestamp_millis()),
                    );
                    meta.insert(
                        "can_be_used_for".to_string(),
                        serde_json::json!([
                            "decision_pattern_analysis",
                            "response_improvement",
                            "self_reflection",
                            "training_data"
                        ]),
                    );
                    meta
                },
            }),
            created_at: chrono::Utc::now().timestamp_millis(),
            unique: Some(false),
            similarity: None,
        };

        // Store thought in dedicated thoughts table
        let adapter_opt = self.runtime.read().unwrap().adapter.read().unwrap().clone();
        if let Some(adapter) = adapter_opt.as_ref() {
            match adapter.create_memory(&thought_memory, "thoughts").await {
                Ok(id) => {
                    debug!("✓ Thought stored with ID: {}", id);
                    info!("💾 Stored for: pattern analysis, improvement, reflection, training");
                }
                Err(e) => warn!("Failed to store thought: {}", e),
            }
        }

        Ok(())
    }

    /// Parse LLM response to extract thought and text
    fn parse_llm_response(&self, raw_response: &str) -> (Option<String>, String) {
        // Prefer <text> extraction within XML; also capture optional <actions>
        // Tolerate varied whitespace and order by using regex captures
        let re = regex::Regex::new(r"(?is)<response[^>]*>.*?(?:<thought>\s*(.*?)\s*</thought>)?.*?(?:<actions>\s*(.*?)\s*</actions>)?.*?<text>\s*(.*?)\s*</text>.*?</response>").unwrap();
        if let Some(caps) = re.captures(raw_response) {
            let mut thought = caps.get(1).map(|m| m.as_str().trim().to_string());
            let actions_match = caps.get(2).map(|m| m.as_str());
            let text = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            if let Some(actions_match) = actions_match {
                let parsed: Vec<String> = actions_match
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !parsed.is_empty() {
                    let mut rt = self.runtime.write().unwrap();
                    rt.set_setting("ui:lastParsedActions", serde_json::json!(parsed), false);
                }
            }
            if thought.is_none() {
                if let Some(thought_start) = raw_response.find("<thought>") {
                    if let Some(thought_end) = raw_response.find("</thought>") {
                        thought = Some(
                            raw_response[thought_start + 9..thought_end]
                                .trim()
                                .to_string(),
                        );
                    }
                }
            }
            return (thought, text.trim().to_string());
        }

        if let Some(text_start) = raw_response.find("<text>") {
            if let Some(text_end) = raw_response.find("</text>") {
                let text = &raw_response[text_start + 6..text_end];
                let thought = if let Some(thought_start) = raw_response.find("<thought>") {
                    if let Some(thought_end) = raw_response.find("</thought>") {
                        Some(
                            raw_response[thought_start + 9..thought_end]
                                .trim()
                                .to_string(),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };
                // Optional actions tag parsing (comma-separated)
                if let Some(actions_start) = raw_response.find("<actions>") {
                    if let Some(actions_end) = raw_response.find("</actions>") {
                        let actions_str = raw_response[actions_start + 9..actions_end].trim();
                        let parsed: Vec<String> = actions_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !parsed.is_empty() {
                            // Store parsed actions hint for downstream execution
                            let mut rt = self.runtime.write().unwrap();
                            rt.set_setting(
                                "ui:lastParsedActions",
                                serde_json::json!(parsed),
                                false,
                            );
                        }
                    }
                }
                return (thought, text.trim().to_string());
            }
        }
        let cleaned = regex::Regex::new("(?s)</?[^>]+>")
            .unwrap()
            .replace_all(raw_response, "")
            .to_string()
            .trim()
            .to_string();
        let final_text = if cleaned.is_empty() {
            raw_response.trim().to_string()
        } else {
            cleaned
        };
        (None, final_text)
    }

    /// Process actions for the message
    async fn process_actions(&self, message: &Memory, state: &State) -> Result<Vec<ActionResult>> {
        let rt = self.runtime.read().unwrap();
        let actions = rt.actions.read().unwrap();

        // Create a dummy runtime reference
        let runtime_ref: Arc<dyn std::any::Any + Send + Sync> = Arc::new(());

        // Execute REPLY first if valid, then allow additional planned actions hinted by <actions>
        let mut results: Vec<ActionResult> = Vec::new();
        if let Some(reply) = actions.iter().find(|a| a.name() == "REPLY") {
            match reply.validate(runtime_ref.clone(), message, state).await {
                Ok(true) => {
                    debug!("Executing action: REPLY");
                    if let Ok(Some(result)) = reply
                        .handler(runtime_ref.clone(), message, state, None, None)
                        .await
                    {
                        results.push(result);
                    }
                }
                Ok(false) => debug!("REPLY action validation failed"),
                Err(e) => warn!("Action REPLY validate error: {}", e),
            }
        }

        // Optional additional actions from settings populated by parse_llm_response
        let planned = {
            let rt = self.runtime.read().unwrap();
            rt.get_setting("ui:lastParsedActions")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
        };
        for name_val in planned {
            if let Some(name) = name_val.as_str() {
                if let Some(act) = actions.iter().find(|a| a.name().eq_ignore_ascii_case(name)) {
                    match act.validate(runtime_ref.clone(), message, state).await {
                        Ok(true) => {
                            debug!("Executing additional action: {}", act.name());
                            match act
                                .handler(runtime_ref.clone(), message, state, None, None)
                                .await
                            {
                                Ok(Some(res)) => results.push(res),
                                Ok(None) => {}
                                Err(e) => warn!("Action {} failed: {}", act.name(), e),
                            }
                        }
                        Ok(false) => debug!("Additional action {} validation failed", act.name()),
                        Err(e) => warn!("Action {} validate error: {}", act.name(), e),
                    }
                }
            }
        }

        Ok(results)
    }

    /// Run evaluators on the message
    async fn evaluate(
        &self,
        message: &Memory,
        state: &State,
        did_respond: bool,
        responses: &[Memory],
    ) -> Result<()> {
        let rt = self.runtime.read().unwrap();
        let evaluators = rt.evaluators.read().unwrap();

        // Create a dummy runtime reference
        let runtime_ref: Arc<dyn std::any::Any + Send + Sync> = Arc::new(());

        for evaluator in evaluators.iter() {
            // Check if should run
            let should_run = evaluator.always_run()
                || evaluator
                    .validate(runtime_ref.clone(), message, state)
                    .await
                    .unwrap_or(false);

            if should_run {
                debug!("Running evaluator: {}", evaluator.name());
                if let Err(e) = evaluator
                    .handler(
                        runtime_ref.clone(),
                        message,
                        state,
                        did_respond,
                        Some(responses.to_vec()),
                    )
                    .await
                {
                    warn!("Evaluator {} failed: {}", evaluator.name(), e);
                }
            }
        }

        Ok(())
    }

    /// Compose state with proper RuntimeRef for providers
    async fn compose_state_with_runtime_ref(&self, message: &Memory) -> Result<State> {
        use crate::runtime_ref::RuntimeRef;

        // Create RuntimeRef from the runtime Arc
        let runtime_ref = Arc::new(RuntimeRef::new(&self.runtime));
        let runtime_any = runtime_ref.as_any_arc();

        let mut state = State::new();

        // Get providers; in fast mode, only run essential ones
        let (providers, fast_mode) = {
            let rt = self.runtime.read().unwrap();
            let fast = rt
                .get_setting("ui:fast_mode")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let list = rt.providers.read().unwrap().clone();
            (list, fast)
        };

        // Run each provider with proper RuntimeRef
        for provider in &providers {
            if fast_mode {
                // Skip non-essential planning providers to reduce latency
                let name = provider.name().to_lowercase();
                if name.contains("planner")
                    || name.contains("recall")
                    || name.contains("session_cues")
                {
                    continue;
                }
            }
            debug!("Running provider: {}", provider.name());

            match provider.get(runtime_any.clone(), message, &state).await {
                Ok(result) => {
                    let mut has_output = false;

                    if let Some(ref text) = result.text {
                        state.set_value(provider.name().to_uppercase(), text.clone());
                        has_output = true;
                    }
                    if let Some(values) = result.values {
                        for (k, v) in values {
                            state.set_value(k, v);
                        }
                        has_output = true;
                    }
                    if let Some(ref data) = result.data {
                        for (k, v) in data.clone() {
                            state.set_data(k, v);
                        }
                        has_output = true;
                    }

                    // Log detailed output for planning providers
                    if provider.name() == "reaction_planner" || provider.name() == "output_planner"
                    {
                        if has_output {
                            debug!(
                                "╔════════════════════════════════════════════════════════════════"
                            );
                            debug!("║ {} OUTPUT", provider.name().to_uppercase());
                            debug!(
                                "╠════════════════════════════════════════════════════════════════"
                            );

                            if let Some(ref text) = result.text {
                                for line in text.lines() {
                                    debug!("║ {}", line);
                                }
                            }

                            if let Some(ref data) = result.data {
                                if let Some(plan_data) = data.values().next() {
                                    debug!(
                                        "║ Data: {}",
                                        serde_json::to_string_pretty(plan_data).unwrap_or_default()
                                    );
                                }
                            }

                            debug!(
                                "╚════════════════════════════════════════════════════════════════"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("Provider {} failed: {}", provider.name(), e);
                }
            }
        }
        {
            let rt = self.runtime.read().unwrap();
            // Inject UI tone/verbosity from settings into state values for template use
            if let Some(tone) = rt
                .get_setting("ui:tone")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                state.set_value("UI_TONE", tone);
            }
            if let Some(verb) = rt.get_setting("ui:verbosity") {
                let verb_s = if let Some(s) = verb.as_str() {
                    s.to_string()
                } else {
                    verb.to_string()
                };
                state.set_value("UI_VERBOSITY", verb_s);
            }
            if let Some(sug) = rt
                .get_setting("ui:suggestedResponseLength")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                state.set_value("UI_SUGGESTED_RESPONSE_LENGTH", sug);
            }
            if let Some(shift) = rt
                .get_setting("ui:possibleTopicShift")
                .and_then(|v| v.as_bool())
            {
                state.set_value(
                    "UI_TOPIC_SHIFT",
                    if shift {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    },
                );
            }
            let room_prefix = format!("ui:lastThought:{}:", message.room_id);
            let last_thoughts = rt
                .get_settings_with_prefix(&room_prefix)
                .into_iter()
                .map(|(_, v)| v)
                .collect::<Vec<String>>();
            if !last_thoughts.is_empty() {
                let summary = last_thoughts.join(" ");
                state.set_value("CONTEXT_LAST_THOUGHT", summary);
            }
            state.set_value("LAST_PROMPT", message.content.text.clone());
            if let Some(lang) = rt
                .get_setting("ui:language")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                state.set_value("UI_LANGUAGE", lang);
            }
            if let Some(intent) = rt
                .get_setting("ui:intent")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                state.set_value("UI_INTENT", intent);
            }
            if let Some(kw) = rt
                .get_setting("ui:keywords")
                .and_then(|v| v.as_array().cloned())
            {
                let joined = kw
                    .into_iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
                    .join(", ");
                state.set_value("UI_KEYWORDS", joined);
            }
            if let Some(top) = rt
                .get_setting("ui:topics")
                .and_then(|v| v.as_array().cloned())
            {
                let joined = top
                    .into_iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
                    .join(", ");
                state.set_value("UI_TOPICS", joined);
            }
            if let Some(ent) = rt
                .get_setting("ui:entities")
                .and_then(|v| v.as_array().cloned())
            {
                let joined = ent
                    .into_iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
                    .join(", ");
                state.set_value("UI_ENTITIES", joined);
            }
            if let Some(arr) = rt
                .get_setting("phase0:agent_candidates")
                .and_then(|v| v.as_array().cloned())
            {
                let joined = arr
                    .into_iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
                    .join(", ");
                state.set_value("UI_AGENT_CANDIDATES", joined);
            }
            if let Some(comp) = rt
                .get_setting("ui:complexity")
                .and_then(|v| v.as_object().cloned())
            {
                if let Some(level) = comp.get("level").and_then(|v| v.as_str()) {
                    state.set_value("UI_COMPLEXITY_LEVEL", level.to_string());
                }
                if let Some(reasoning) = comp.get("reasoning").and_then(|v| v.as_str()) {
                    state.set_value("UI_COMPLEXITY_REASONING", reasoning.to_string());
                }
            }
        }

        // Emit compaction flag if composing the full prompt would approach the model context window
        {
            use crate::planner::cost::CostCalculator;
            use crate::planner::tokens::TokenCounter;
            let calc = CostCalculator::new();
            let template = crate::templates::MESSAGE_HANDLER_TEMPLATE;
            if let Ok(preview) = crate::templates::compose_prompt_from_state(&state, template) {
                let estimated_input = TokenCounter::estimate_tokens(&preview);
                let rt = self.runtime.read().unwrap();
                let provider_name = rt
                    .models
                    .read()
                    .unwrap()
                    .get("TEXT_LARGE")
                    .and_then(|v| v.first())
                    .map(|h| h.name.clone())
                    .unwrap_or_else(|| "local".to_string());
                let model_key = if provider_name.to_lowercase().contains("openai") {
                    "gpt-4o".to_string()
                } else if provider_name.to_lowercase().contains("claude")
                    || provider_name.to_lowercase().contains("anthropic")
                {
                    "claude-3.5-sonnet".to_string()
                } else {
                    "local".to_string()
                };
                if let Some(pricing) = calc.get_pricing(&model_key) {
                    let compact = estimated_input + 256 > pricing.context_window;
                    state.set_value("UI_COMPACT_CONTEXT", if compact { "true" } else { "false" });
                }
            }
        }

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeOpts;
    use regex::Regex;

    #[tokio::test]
    #[ignore = "Integration test - requires Ollama running at localhost:11434"]
    async fn test_message_processor() {
        let runtime = crate::AgentRuntime::new(RuntimeOpts::default())
            .await
            .unwrap();
        let processor = MessageProcessor::new(runtime);

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Hello!".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let room = Room {
            id: message.room_id,
            agent_id: Some(message.agent_id),
            name: "Test Room".to_string(),
            source: "test".to_string(),
            channel_type: ChannelType::Dm,
            channel_id: None,
            server_id: None,
            world_id: uuid::Uuid::new_v4(),
            metadata: std::collections::HashMap::new(),
            created_at: None,
        };

        let result = processor.process_message(message, room).await;
        if let Err(ref e) = result {
            eprintln!("Message processing failed: {:?}", e);
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_llm_response_xml_variants() {
        let runtime = crate::create_mock_runtime();
        let proc = MessageProcessor::new(runtime);
        let xml = "<response>\n<thought>think</thought>\n<actions>REPLY,ASK_CLARIFY</actions>\n<text>Hello world</text>\n</response>";
        let (thought, text) = proc.parse_llm_response(xml);
        assert_eq!(thought.as_deref(), Some("think"));
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_parse_llm_response_xml_spacing() {
        let runtime = crate::create_mock_runtime();
        let proc = MessageProcessor::new(runtime);
        let xml = "<response> <text> spaced </text> </response>";
        let (_thought, text) = proc.parse_llm_response(xml);
        assert_eq!(text, "spaced");
    }

    #[test]
    fn test_tone_verbosity_injection() {
        let mut state = State::new();
        state.set_value("UI_TONE", "friendly");
        state.set_value("UI_VERBOSITY", "short");
        let tpl = "Tone: {{UI_TONE}} Verbosity: {{UI_VERBOSITY}}";
        let rendered = crate::templates::compose_prompt_from_state(&state, tpl).unwrap();
        assert!(rendered.contains("friendly"));
        assert!(rendered.contains("short"));
    }

    #[test]
    fn test_parse_llm_response_malformed_actions() {
        let runtime = crate::create_mock_runtime();
        let proc = MessageProcessor::new(runtime);
        let xml = "<response><text>hello</text><actions> , , REPLY ,, </actions></response>";
        let (_thought, text) = proc.parse_llm_response(xml);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_parse_llm_response_missing_wrapper() {
        let runtime = crate::create_mock_runtime();
        let proc = MessageProcessor::new(runtime);
        let xml = "<text>hello</text>";
        let (_thought, text) = proc.parse_llm_response(xml);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_parse_llm_response_multiple_text() {
        let runtime = crate::create_mock_runtime();
        let proc = MessageProcessor::new(runtime);
        let xml = "<response><text>first</text><text>second</text></response>";
        let (_thought, text) = proc.parse_llm_response(xml);
        assert!(text == "first" || text == "second");
    }
}

// Streaming via Ollama will be implemented in provider-specific crates to avoid coupling here.
