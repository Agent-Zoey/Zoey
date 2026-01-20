//! Request handlers for Agent API
//!
//! Implements the core endpoint logic for agent interaction

use super::{state::ServerState, task::TaskResult, types::*};
use crate::observability::{get_global_cost_tracker, LLMCallContext};
use crate::planner::cost::CostCalculator;
use crate::planner::tokens::TokenCounter;
use crate::streaming::{create_text_stream, StreamHandler, TextChunk};
use crate::types::database::IDatabaseAdapter;
use crate::types::memory::MemoryQuery;
use crate::{
    types::{ChannelType, Content, Memory, Room},
    AgentRuntime, ZoeyError, MessageProcessor, Result,
};
use axum::response::sse::{Event, Sse};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use futures_util::stream::{BoxStream, StreamExt};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

async fn run_chat_stream_job(
    runtime: Arc<RwLock<AgentRuntime>>,
    req_clone: ChatRequest,
    stream_handler: StreamHandler,
) {
    let (provider, available_providers) = {
        let rt_guard = runtime.read().unwrap();
        let pref = rt_guard
            .get_setting("model_provider")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let providers: Vec<String> = rt_guard.get_providers().iter().map(|p| p.name().to_string()).collect();
        (pref, providers)
    };
    info!(
        "INTERACTION_PROVIDER provider_pref={} available=[{}]",
        provider.clone().unwrap_or_else(|| "<none>".to_string()),
        available_providers.join(", ")
    );

    if provider
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("openai"))
        .unwrap_or(false)
    {
        let entity_id = req_clone.entity_id.unwrap_or_else(Uuid::new_v4);
        let (agent_id, adapter) = {
            let rt = runtime.read().unwrap();
            let adapter = rt.adapter.read().unwrap().clone();
            (rt.agent_id, adapter)
        };
        let recent_conversation = if let Some(ref adapter) = adapter {
            fetch_recent_conversation(
                adapter.as_ref(),
                req_clone.room_id,
                agent_id,
                &{
                    let rt = runtime.read().unwrap();
                    rt.character.name.clone()
                },
                5, // Reduced from 10 to prevent context explosion
            )
            .await
        } else {
            String::new()
        };
        let (character_name, character_bio, ui_tone, ui_verbosity, last, prev) = {
            let rt = runtime.read().unwrap();
            let name = rt.character.name.clone();
            let bio = rt.character.bio.clone().join(" ");
            let tone = rt
                .get_setting("ui:tone")
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            let verbosity = rt.get_setting("ui:verbosity").map(|v| v.to_string());
            let last_key = format!("ui:lastPrompt:{}:last", req_clone.room_id);
            let prev_key = format!("ui:lastPrompt:{}:prev", req_clone.room_id);
            let last = rt
                .get_setting(&last_key)
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            let prev = rt
                .get_setting(&prev_key)
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            (name, bio, tone, verbosity, last, prev)
        };
        let mut state = crate::types::State::new();
        state.set_value(
            "CHARACTER",
            format!("Name: {}\nBio: {}", character_name, character_bio),
        );
        if let Some(t) = ui_tone {
            state.set_value("UI_TONE", t);
        }
        if let Some(v) = ui_verbosity {
            state.set_value("UI_VERBOSITY", v);
        }
        if let Some(p) = prev.clone() {
            state.set_value("PREV_PROMPT", p);
        }
        if let Some(l) = last.clone() {
            state.set_value("LAST_PROMPT", l);
        }
        state.set_value("ENTITY_NAME", "User");
        state.set_value("MESSAGE_TEXT", req_clone.text.clone());
        let recent = if !recent_conversation.is_empty() {
            format!("{}\nUser: {}", recent_conversation, req_clone.text)
        } else {
            format!(
                "{}\n{}\nUser: {}",
                prev.map(|p| format!("User: {}", p)).unwrap_or_default(),
                last.map(|l| format!("User: {}", l)).unwrap_or_default(),
                req_clone.text
            )
        };
        state.set_value("RECENT_MESSAGES", recent);
        
        // Run all providers to enrich state with their context
        let message = crate::types::Memory {
            id: uuid::Uuid::new_v4(),
            entity_id,
            agent_id,
            room_id: req_clone.room_id,
            content: crate::types::Content {
                text: req_clone.text.clone(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };
        let providers = runtime.read().unwrap().providers.read().unwrap().clone();
        let runtime_ref: std::sync::Arc<dyn std::any::Any + Send + Sync> = std::sync::Arc::new(());
        for provider in &providers {
            // Skip heavy planning providers for streaming (already skipped in fast mode)
            let name = provider.name().to_lowercase();
            if name.contains("planner") || name.contains("recall") {
                continue;
            }
            if let Ok(result) = provider.get(runtime_ref.clone(), &message, &state).await {
                if let Some(text) = result.text {
                    state.set_value(provider.name().to_uppercase(), text);
                }
                if let Some(values) = result.values {
                    for (k, v) in values {
                        state.set_value(k, v);
                    }
                }
            }
        }
        
        let template = crate::templates::MESSAGE_HANDLER_TEMPLATE;
        let prompt = crate::templates::compose_prompt_from_state(&state, template)
            .unwrap_or_else(|_| req_clone.text.clone());
        
        // Log composed prompt for debugging
        let prompt_preview: String = prompt.chars().take(500).collect();
        let prompt_len = prompt.len();
        debug!(
            "INTERACTION_PROMPT room_id={} prompt_len={} preview={}...",
            req_clone.room_id, prompt_len, prompt_preview
        );
        // Store user's text only (not full prompt) to avoid context explosion
        {
            let mut rt = runtime.write().unwrap();
            rt.set_setting("ui:lastPrompt", serde_json::json!(req_clone.text.clone()), false);
            let last_key = format!("ui:lastPrompt:{}:prev", req_clone.room_id);
            if let Some(old_last) = rt.get_setting(&format!("ui:lastPrompt:{}:last", req_clone.room_id)) {
                rt.set_setting(&last_key, old_last, false);
            }
            rt.set_setting(&format!("ui:lastPrompt:{}:last", req_clone.room_id), serde_json::json!(req_clone.text.clone()), false);
        }
        
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            let _ = stream_handler
                .send_error(ZoeyError::other("OPENAI_API_KEY is not set"))
                .await;
            return;
        }
        static OPENAI_CLIENT: OnceLock<HttpClient> = OnceLock::new();
        let model = {
            let rt = runtime.read().unwrap();
            rt.get_setting("OPENAI_MODEL")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "gpt-4o-mini".to_string())
        };
        let dynamic_max = {
            let calc = CostCalculator::new();
            let mk = if model.contains("gpt-4o") {
                "gpt-4o".to_string()
            } else if model.contains("gpt-4") {
                "gpt-4".to_string()
            } else {
                "gpt-4o-mini".to_string()
            };
            if let Some(pricing) = calc.get_pricing(&mk) {
                let est_in = TokenCounter::estimate_tokens(&prompt);
                let mut avail = if pricing.context_window > est_in {
                    pricing.context_window - est_in
                } else {
                    0
                };
                avail = avail.min(pricing.max_output_tokens);
                let safety = 64usize;
                if avail > safety {
                    avail.saturating_sub(safety)
                } else {
                    256
                }
            } else {
                768
            }
        };
        let client = OPENAI_CLIENT
            .get_or_init(|| {
                reqwest::Client::builder()
                    .pool_max_idle_per_host(50)
                    .pool_idle_timeout(std::time::Duration::from_secs(300))
                    .tcp_keepalive(std::time::Duration::from_secs(60))
                    .timeout(std::time::Duration::from_secs(120))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new())
            })
            .clone();
        let req_body = serde_json::json!({
            "model": model,
            "stream": true,
            "max_tokens": std::cmp::max(dynamic_max, 2048),
            "messages": [
                {"role": "user", "content": prompt}
            ]
        });
        let stream_timeout = std::env::var("OPENAI_STREAM_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(45);
        let stream_start = Instant::now();
        let prompt_tokens = TokenCounter::estimate_tokens(&prompt);
        let resp = tokio::time::timeout(
            Duration::from_secs(stream_timeout),
            client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(api_key)
                .json(&req_body)
                .send(),
        )
        .await;
        match resp {
            Err(_) => {
                let _ = stream_handler
                    .send_error(ZoeyError::other("OpenAI streaming request timed out"))
                    .await;
            }
            Ok(Err(e)) => {
                let _ = stream_handler
                    .send_error(ZoeyError::other(format!(
                        "OpenAI streaming request failed: {}",
                        e
                    )))
                    .await;
            }
            Ok(Ok(mut r)) => {
                let mut buffer = String::new();
                let mut full_text = String::new();
                let mut last_chunk_at = Instant::now();
                while let Ok(chunk_result) = tokio::time::timeout(
                    Duration::from_secs(stream_timeout),
                    r.chunk(),
                )
                .await
                {
                    last_chunk_at = Instant::now();
                    let chunk = match chunk_result {
                        Ok(opt) => match opt {
                            Some(c) => c,
                            None => break,
                        },
                        Err(e) => {
                            let _ = stream_handler
                                .send_error(ZoeyError::other(format!(
                                    "OpenAI streaming chunk failed: {}",
                                    e
                                )))
                                .await;
                            break;
                        }
                    };
                    let s = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&s);
                    let mut parts: Vec<&str> = buffer.split('\n').collect();
                    let tail = parts.pop().unwrap_or("");
                    for line in parts {
                        let l = line.trim();
                        if !l.starts_with("data:") {
                            continue;
                        }
                        let payload = l.trim_start_matches("data:").trim();
                        if payload == "[DONE]" {
                            let _ = stream_handler.send_chunk(String::new(), true).await;
                            let latency_ms = stream_start.elapsed().as_millis() as u64;
                            let completion_tokens = TokenCounter::estimate_tokens(&full_text);
                            // store response and cost
                            let adapter = {
                                let rt = runtime.read().unwrap();
                                let x = rt.adapter.read().unwrap().clone();
                                x
                            };
                            if let Some(adapter) = adapter.as_ref() {
                                let agent_id = {
                                    let rt = runtime.read().unwrap();
                                    rt.agent_id
                                };
                                let response = Memory {
                                    id: Uuid::new_v4(),
                                    entity_id: agent_id,
                                    agent_id,
                                    room_id: req_clone.room_id,
                                    content: Content {
                                        text: full_text.clone(),
                                        source: Some(req_clone.source.clone()),
                                        ..Default::default()
                                    },
                                    embedding: None,
                                    metadata: None,
                                    created_at: chrono::Utc::now().timestamp(),
                                    unique: Some(false),
                                    similarity: None,
                                };
                                let _ = adapter.create_memory(&response, "messages").await;
                            }
                            // Track cost using global cost tracker
                            if let Some(tracker) = get_global_cost_tracker() {
                                let context = LLMCallContext {
                                    agent_id,
                                    user_id: req_clone.entity_id.map(|u| u.to_string()),
                                    conversation_id: Some(req_clone.room_id),
                                    action_name: None,
                                    evaluator_name: None,
                                    temperature: Some(0.7),
                                    cached_tokens: None,
                                    ttft_ms: None,
                                    prompt_hash: None,
                                    prompt_preview: Some(req_clone.text.chars().take(100).collect()),
                                };
                                match tracker.record_llm_call(
                                    "openai",
                                    &model,
                                    prompt_tokens,
                                    completion_tokens,
                                    latency_ms,
                                    agent_id,
                                    context,
                                ).await {
                                    Ok(record) => {
                                        info!("COST_TRACKED provider=openai model={} prompt_tokens={} completion_tokens={} cost_usd={:.6} latency_ms={}", 
                                            model, prompt_tokens, completion_tokens, record.total_cost_usd, latency_ms);
                                    }
                                    Err(e) => {
                                        error!("Failed to track cost: {}", e);
                                    }
                                }
                            }
                            {
                                let mut rt = runtime.write().unwrap();
                                let key = format!("ui:lastAddressed:{}", req_clone.room_id);
                                rt.set_setting(
                                    &key,
                                    serde_json::json!(chrono::Utc::now().timestamp()),
                                    false,
                                );
                            }
                            
                            // Record training sample for RLHF
                            let sample_id = {
                                let collector = {
                                    let rt = runtime.read().unwrap();
                                    rt.get_training_collector()
                                };
                                if let Some(collector) = collector {
                                    match collector.record_interaction(
                                        req_clone.text.clone(),
                                        full_text.clone(),
                                        None, // thought - could extract from response if formatted
                                        0.7,  // default quality score
                                    ).await {
                                        Ok(id) => {
                                            info!("TRAINING_SAMPLE_RECORDED sample_id={} prompt_len={} response_len={}", 
                                                id, req_clone.text.len(), full_text.len());
                                            Some(id)
                                        }
                                        Err(e) => {
                                            debug!("Training sample not recorded: {}", e);
                                            None
                                        }
                                    }
                                } else {
                                    None
                                }
                            };
                            
                            // Send final chunk with sample_id in metadata for feedback
                            if let Some(sid) = sample_id {
                                let _ = stream_handler.send_chunk_with_meta(
                                    String::new(),
                                    true,
                                    Some(serde_json::json!({ "sampleId": sid.to_string() }))
                                ).await;
                            }
                            
                            break;
                        }
                        if let Ok(json) = serde_json::from_str::<JsonValue>(payload) {
                            if let Some(choices) = json.get("choices").and_then(|v| v.as_array()) {
                                if let Some(delta) = choices.get(0).and_then(|c| c.get("delta")) {
                                    if let Some(content) =
                                        delta.get("content").and_then(|v| v.as_str())
                                    {
                                        let _ = stream_handler
                                            .send_chunk(content.to_string(), false)
                                            .await;
                                        full_text.push_str(content);
                                    }
                                }
                            }
                        }
                    }
                    buffer = tail.to_string();
                }
            }
            Err(e) => {
                let _ = stream_handler
                    .send_error(ZoeyError::other(format!(
                        "OpenAI streaming request failed: {}",
                        e
                    )))
                    .await;
            }
        }
        return;
    }

    // Ollama/local LLM streaming
    let is_local = provider
        .as_deref()
        .map(|s| {
            let lc = s.to_lowercase();
            lc == "ollama" || lc == "local" || lc == "local-llm" || lc == "llama" || lc == "llamacpp"
        })
        .unwrap_or(false);
    info!("OLLAMA_CHECK is_local={} provider={:?}", is_local, provider);
    if is_local {
        let entity_id = req_clone.entity_id.unwrap_or_else(Uuid::new_v4);
        let (agent_id, adapter) = {
            let rt = runtime.read().unwrap();
            let adapter = rt.adapter.read().unwrap().clone();
            (rt.agent_id, adapter)
        };
        let recent_conversation = if let Some(ref adapter) = adapter {
            fetch_recent_conversation(
                adapter.as_ref(),
                req_clone.room_id,
                agent_id,
                &{
                    let rt = runtime.read().unwrap();
                    rt.character.name.clone()
                },
                5, // Reduced from 10 to prevent context explosion
            )
            .await
        } else {
            String::new()
        };
        let (character_name, character_bio, ui_tone, ui_verbosity, last, prev) = {
            let rt = runtime.read().unwrap();
            let name = rt.character.name.clone();
            let bio = rt.character.bio.clone().join(" ");
            let tone = rt
                .get_setting("ui:tone")
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            let verbosity = rt.get_setting("ui:verbosity").map(|v| v.to_string());
            let last_key = format!("ui:lastPrompt:{}:last", req_clone.room_id);
            let prev_key = format!("ui:lastPrompt:{}:prev", req_clone.room_id);
            let last = rt
                .get_setting(&last_key)
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            let prev = rt
                .get_setting(&prev_key)
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            (name, bio, tone, verbosity, last, prev)
        };
        let mut state = crate::types::State::new();
        state.set_value(
            "CHARACTER",
            format!("Name: {}\nBio: {}", character_name, character_bio),
        );
        if let Some(t) = ui_tone {
            state.set_value("UI_TONE", t);
        }
        if let Some(v) = ui_verbosity {
            state.set_value("UI_VERBOSITY", v);
        }
        if let Some(p) = prev.clone() {
            state.set_value("PREV_PROMPT", p);
        }
        if let Some(l) = last.clone() {
            state.set_value("LAST_PROMPT", l);
        }
        state.set_value("ENTITY_NAME", "User");
        state.set_value("MESSAGE_TEXT", req_clone.text.clone());
        let recent = if !recent_conversation.is_empty() {
            format!("{}\nUser: {}", recent_conversation, req_clone.text)
        } else {
            format!(
                "{}\n{}\nUser: {}",
                prev.map(|p| format!("User: {}", p)).unwrap_or_default(),
                last.map(|l| format!("User: {}", l)).unwrap_or_default(),
                req_clone.text
            )
        };
        state.set_value("RECENT_MESSAGES", recent);

        // Inject relevant knowledge context from uploaded documents
        if let Some(knowledge_context) = retrieve_knowledge_context(req_clone.room_id, &req_clone.text, 5) {
            info!(
                "KNOWLEDGE_CONTEXT_INJECTED room_id={} context_len={}",
                req_clone.room_id,
                knowledge_context.len()
            );
            state.set_value("KNOWLEDGE_CONTEXT", knowledge_context);
        }

        let template = crate::templates::MESSAGE_HANDLER_TEMPLATE;
        let prompt = crate::templates::compose_prompt_from_state(&state, template)
            .unwrap_or_else(|_| req_clone.text.clone());

        // Store user's text only (not full prompt) to avoid context explosion
        {
            let mut rt = runtime.write().unwrap();
            rt.set_setting("ui:lastPrompt", serde_json::json!(req_clone.text.clone()), false);
            let last_key = format!("ui:lastPrompt:{}:prev", req_clone.room_id);
            if let Some(old_last) = rt.get_setting(&format!("ui:lastPrompt:{}:last", req_clone.room_id)) {
                rt.set_setting(&last_key, old_last, false);
            }
            rt.set_setting(&format!("ui:lastPrompt:{}:last", req_clone.room_id), serde_json::json!(req_clone.text.clone()), false);
        }

        // Get Ollama endpoint and model
        let (ollama_base, ollama_model, max_tokens) = {
            let rt = runtime.read().unwrap();
            let base = rt.get_setting("LOCAL_LLM_ENDPOINT")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string()));
            let model = rt.get_setting("LOCAL_LLM_MODEL")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string()));
            let max = rt.get_setting("LOCAL_LLM_MAX_TOKENS")
                .and_then(|v| v.as_u64().map(|u| u as usize))
                .unwrap_or(800);
            (base, model, max)
        };

        info!(
            "OLLAMA_STREAMING endpoint={} model={} prompt_len={}",
            ollama_base, ollama_model, prompt.len()
        );

        static OLLAMA_CLIENT: OnceLock<HttpClient> = OnceLock::new();
        let client = OLLAMA_CLIENT
            .get_or_init(|| {
                reqwest::Client::builder()
                    .pool_max_idle_per_host(10)
                    .pool_idle_timeout(std::time::Duration::from_secs(120))
                    .timeout(std::time::Duration::from_secs(300))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new())
            })
            .clone();

        let req_body = serde_json::json!({
            "model": ollama_model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": true,
            "options": {
                "temperature": 0.7,
                "num_predict": max_tokens
            }
        });

        let stream_timeout = std::env::var("OLLAMA_STREAM_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(120);
        let stream_start = Instant::now();

        let resp = tokio::time::timeout(
            Duration::from_secs(stream_timeout),
            client
                .post(format!("{}/api/chat", ollama_base))
                .json(&req_body)
                .send(),
        )
        .await;

        match resp {
            Err(_) => {
                let _ = stream_handler
                    .send_error(ZoeyError::other("Ollama streaming request timed out"))
                    .await;
            }
            Ok(Err(e)) => {
                let _ = stream_handler
                    .send_error(ZoeyError::other(format!(
                        "Ollama streaming request failed: {}. Check if Ollama is running at {}",
                        e, ollama_base
                    )))
                    .await;
            }
            Ok(Ok(mut r)) => {
                info!("OLLAMA_RESPONSE status={}", r.status());
                if !r.status().is_success() {
                    let status = r.status();
                    let error_text = r.text().await.unwrap_or_default();
                    let _ = stream_handler
                        .send_error(ZoeyError::other(format!(
                            "Ollama API error {}: {}",
                            status, error_text
                        )))
                        .await;
                    return;
                }

                let mut buffer = String::new();
                let mut full_text = String::new();
                let mut chunks_received = 0usize;
                while let Ok(chunk_result) = tokio::time::timeout(
                    Duration::from_secs(stream_timeout),
                    r.chunk(),
                )
                .await
                {
                    let chunk = match chunk_result {
                        Ok(opt) => match opt {
                            Some(c) => c,
                            None => break,
                        },
                        Err(e) => {
                            let _ = stream_handler
                                .send_error(ZoeyError::other(format!(
                                    "Ollama streaming chunk failed: {}",
                                    e
                                )))
                                .await;
                            break;
                        }
                    };
                    let s = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&s);

                    // Ollama returns newline-delimited JSON objects
                    let mut parts: Vec<&str> = buffer.split('\n').collect();
                    let tail = parts.pop().unwrap_or("");
                    for line in parts {
                        let l = line.trim();
                        if l.is_empty() {
                            continue;
                        }
                        if let Ok(json) = serde_json::from_str::<JsonValue>(l) {
                            if let Some(content) = json.get("message").and_then(|m| m.get("content")).and_then(|v| v.as_str()) {
                                chunks_received += 1;
                                if chunks_received == 1 {
                                    info!("OLLAMA_FIRST_CHUNK received, len={}", content.len());
                                }
                                let _ = stream_handler
                                    .send_chunk(content.to_string(), false)
                                    .await;
                                full_text.push_str(content);
                            }
                            // Check if done
                            if json.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                                info!("OLLAMA_DONE total_chunks={} response_len={}", chunks_received, full_text.len());
                                
                                // Record training sample for RLHF
                                let sample_id = {
                                    let collector = {
                                        let rt = runtime.read().unwrap();
                                        rt.get_training_collector()
                                    };
                                    if let Some(collector) = collector {
                                        match collector.record_interaction(
                                            req_clone.text.clone(),
                                            full_text.clone(),
                                            None,
                                            0.7,
                                        ).await {
                                            Ok(id) => {
                                                info!("TRAINING_SAMPLE_RECORDED sample_id={} prompt_len={} response_len={}", 
                                                    id, req_clone.text.len(), full_text.len());
                                                Some(id)
                                            }
                                            Err(e) => {
                                                debug!("Training sample not recorded: {}", e);
                                                None
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                };
                                
                                // Send final chunk with sample_id in metadata for feedback
                                if let Some(sid) = sample_id {
                                    let _ = stream_handler.send_chunk_with_meta(
                                        String::new(),
                                        true,
                                        Some(serde_json::json!({ "sampleId": sid.to_string() }))
                                    ).await;
                                } else {
                                    let _ = stream_handler.send_chunk(String::new(), true).await;
                                }
                                // Store response in database
                                if let Some(adapter) = adapter.as_ref() {
                                    let response = Memory {
                                        id: Uuid::new_v4(),
                                        entity_id: agent_id,
                                        agent_id,
                                        room_id: req_clone.room_id,
                                        content: Content {
                                            text: full_text.clone(),
                                            source: Some(req_clone.source.clone()),
                                            ..Default::default()
                                        },
                                        embedding: None,
                                        metadata: None,
                                        created_at: chrono::Utc::now().timestamp(),
                                        unique: Some(false),
                                        similarity: None,
                                    };
                                    let _ = adapter.create_memory(&response, "messages").await;
                                }
                                // Track cost if tracker available
                                if let Some(tracker) = get_global_cost_tracker() {
                                    let latency_ms = stream_start.elapsed().as_millis() as u64;
                                    let prompt_tokens = TokenCounter::estimate_tokens(&prompt);
                                    let completion_tokens = TokenCounter::estimate_tokens(&full_text);
                                    let context = LLMCallContext {
                                        agent_id,
                                        user_id: req_clone.entity_id.map(|u| u.to_string()),
                                        conversation_id: Some(req_clone.room_id),
                                        action_name: None,
                                        evaluator_name: None,
                                        temperature: Some(0.7),
                                        cached_tokens: None,
                                        ttft_ms: None,
                                        prompt_hash: None,
                                        prompt_preview: Some(req_clone.text.chars().take(100).collect()),
                                    };
                                    let _ = tracker.record_llm_call(
                                        "ollama",
                                        &ollama_model,
                                        prompt_tokens,
                                        completion_tokens,
                                        latency_ms,
                                        agent_id,
                                        context,
                                    ).await;
                                }
                                {
                                    let mut rt = runtime.write().unwrap();
                                    let key = format!("ui:lastAddressed:{}", req_clone.room_id);
                                    rt.set_setting(
                                        &key,
                                        serde_json::json!(chrono::Utc::now().timestamp()),
                                        false,
                                    );
                                }
                                break;
                            }
                        }
                    }
                    buffer = tail.to_string();
                }
                // If we exit the loop without getting done, log warning
                if chunks_received > 0 {
                    info!("OLLAMA_STREAM_END chunks={} response_len={}", chunks_received, full_text.len());
                } else {
                    error!("OLLAMA_STREAM_END no chunks received");
                }
            }
        }
        return;
    }

    // Fallback: process and chunk final output
    match process_chat_task(runtime.clone(), req_clone.clone()).await {
        Ok(resp) => {
            let final_text = resp
                .messages
                .as_ref()
                .and_then(|v| v.first())
                .map(|m| m.content.text.clone())
                .unwrap_or_default();
            let chunk_size = 80usize;
            let mut idx = 0;
            if final_text.is_empty() {
                let _ = stream_handler.send_chunk(String::new(), true).await;
            } else {
                while idx < final_text.len() {
                    let end = (idx + chunk_size).min(final_text.len());
                    let piece = final_text[idx..end].to_string();
                    let is_final = end >= final_text.len();
                    if stream_handler.send_chunk(piece, is_final).await.is_err() {
                        break;
                    }
                    idx = end;
                    if !is_final {
                        tokio::task::yield_now().await;
                    }
                }
            }
        }
        Err(e) => {
            let _ = stream_handler
                .send_error(ZoeyError::other(format!("Streaming failed: {}", e)))
                .await;
        }
    }
}

/// Fetch recent conversation history from database including both user and agent messages.
/// Returns formatted string suitable for RECENT_MESSAGES context.
/// Fetches up to `limit` messages, sorted by creation time (oldest first for natural reading order).
async fn fetch_recent_conversation(
    adapter: &dyn IDatabaseAdapter,
    room_id: Uuid,
    agent_id: Uuid,
    agent_name: &str,
    limit: usize,
) -> String {
    let query = MemoryQuery {
        room_id: Some(room_id),
        table_name: "messages".to_string(),
        count: Some(limit),
        ..Default::default()
    };

    match adapter.get_memories(query).await {
        Ok(mut memories) => {
            if memories.is_empty() {
                return String::new();
            }

            // Sort by created_at ascending (oldest first) for natural conversation flow
            memories.sort_by_key(|m| m.created_at);

            // Format messages with speaker labels
            memories
                .iter()
                .map(|m| {
                    // If entity_id == agent_id, it's an agent message
                    let speaker = if m.entity_id == agent_id {
                        agent_name.to_string()
                    } else {
                        // Try to get entity name from metadata, fallback to "User"
                        m.metadata
                            .as_ref()
                            .and_then(|meta| meta.entity_name.clone())
                            .unwrap_or_else(|| "User".to_string())
                    };
                    format!("{}: {}", speaker, m.content.text)
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        Err(e) => {
            eprintln!("[WARN] Failed to fetch recent conversation: {}", e);
            String::new()
        }
    }
}

/// Health check handler
pub async fn health_check(State(state): State<ServerState>) -> Json<HealthResponse> {
    let runtime = state.api_state.runtime.read().unwrap();
    Json(HealthResponse {
        status: "ok".to_string(),
        agent_id: runtime.agent_id,
        agent_name: runtime.character.name.clone(),
        uptime: state.api_state.start_time.elapsed().as_secs(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// List available model providers
///
/// Returns the list of registered MODEL providers (those with valid API keys)
/// and the currently selected provider. These are LLM providers like openai,
/// anthropic, local-llm - not context providers.
pub async fn providers_list_handler(
    State(server_state): State<ServerState>,
) -> impl IntoResponse {
    let rt = server_state.api_state.runtime.read().unwrap();
    
    // Get registered MODEL providers from rt.models (not rt.providers which are context providers)
    // Model providers are keyed by model type (TEXT_LARGE, etc.) - extract unique provider names
    let models = rt.get_models();
    let mut provider_names: Vec<String> = models
        .values()
        .flat_map(|handlers| handlers.iter().map(|h| h.name.clone()))
        .collect();
    
    // Deduplicate and sort for consistent output
    provider_names.sort();
    provider_names.dedup();
    
    // Get current provider setting
    let current = rt
        .get_setting("model_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    
    info!(
        "PROVIDERS_LIST available={:?} current={:?}",
        provider_names, current
    );
    
    Json(ProvidersListResponse {
        success: true,
        providers: provider_names,
        current,
    })
}

/// Switch the active model provider
///
/// Validates that the requested provider is registered (has valid API keys)
/// before allowing the switch. Uses flexible case-insensitive matching.
pub async fn provider_switch_handler(
    State(server_state): State<ServerState>,
    Json(request): Json<ProviderSwitchRequest>,
) -> impl IntoResponse {
    // Get available MODEL providers (from rt.models, not rt.providers)
    let available: Vec<String> = {
        let rt = server_state.api_state.runtime.read().unwrap();
        let models = rt.get_models();
        let mut names: Vec<String> = models
            .values()
            .flat_map(|handlers| handlers.iter().map(|h| h.name.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    };
    
    // Simple case-insensitive matching - any registered provider is valid
    let req_lc = request.provider.to_lowercase();
    
    let matched = available.iter().find(|p| {
        let p_lc = p.to_lowercase();
        // Case-insensitive match or partial match
        p_lc == req_lc || p_lc.contains(&req_lc) || req_lc.contains(&p_lc)
    });
    
    if let Some(provider_name) = matched {
        // Update the runtime setting
        {
            let mut rt = server_state.api_state.runtime.write().unwrap();
            rt.set_setting(
                "model_provider",
                serde_json::json!(provider_name),
                false,
            );
            // Also update MODEL_PROVIDER for consistency
            rt.set_setting(
                "MODEL_PROVIDER",
                serde_json::json!(provider_name),
                false,
            );
        }
        
        info!(
            "PROVIDER_SWITCH success provider={} (requested={})",
            provider_name, request.provider
        );
        
        Json(ProviderSwitchResponse {
            success: true,
            provider: Some(provider_name.clone()),
            error: None,
        })
    } else {
        warn!(
            "PROVIDER_SWITCH failed provider={} available={:?}",
            request.provider, available
        );
        
        Json(ProviderSwitchResponse {
            success: false,
            provider: None,
            error: Some(format!(
                "Provider '{}' not available. Available providers: {:?}",
                request.provider, available
            )),
        })
    }
}

/// Chat handler - send message to agent using async task pattern
pub async fn chat_handler(
    State(server_state): State<ServerState>,
    Json(request): Json<ChatRequest>,
) -> Response {
    let agent_name = {
        let rt = server_state.api_state.runtime.read().unwrap();
        rt.character.name.clone()
    };
    info!(
        "[{}] chat request room_id={}, stream={}, text_len={}",
        agent_name,
        request.room_id,
        request.stream,
        request.text.len()
    );

    // Route streaming requests to the SSE handler
    if request.stream {
        return chat_stream_handler(State(server_state), Json(request))
            .await
            .into_response();
    }

    // Validate non-streaming request
    if request.text.trim().is_empty() {
        return ApiError::BadRequest("Message text cannot be empty".to_string()).into_response();
    }
    let max_len = std::env::var("API_MAX_MESSAGE_BYTES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(512_000); // ~0.5MB default
    if request.text.len() > max_len {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "success": false,
                "error": "Message too large",
                "code": StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
            })),
        )
            .into_response();
    }

    // Track last prompt/room ownership (mirrors streaming handler side effects)
    {
        let runtime = server_state.api_state.runtime.clone();
        let mut rt = runtime.write().unwrap();
        let last_key = format!("ui:lastPrompt:{}:last", request.room_id);
        let prev_key = format!("ui:lastPrompt:{}:prev", request.room_id);
        let prev = rt
            .get_setting(&last_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        if let Some(p) = prev {
            rt.set_setting(&prev_key, serde_json::json!(p), false);
        }
        rt.set_setting(&last_key, serde_json::json!(request.text.clone()), false);
        if let Some(owner) = request.entity_id {
            let owner_key = format!("ROOM_OWNER:{}", request.room_id);
            if rt.get_setting(&owner_key).is_none() {
                rt.set_setting(&owner_key, serde_json::json!(owner.to_string()), false);
            }
        }
    }

    // Submit chat processing as an async task and return task ID for polling
    let task_id = server_state.task_manager.create_task();
    let task_manager = server_state.task_manager.clone();
    let runtime = server_state.api_state.runtime.clone();
    let req_clone = request.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("chat task runtime");
        rt.block_on(async move {
            task_manager.mark_running(task_id);
            let timeout_res = tokio::time::timeout(
                Duration::from_secs(90),
                process_chat_task(runtime, req_clone),
            )
            .await;

            match timeout_res {
                Ok(Ok(response)) => task_manager.complete_task(task_id, TaskResult::Chat(response)),
                Ok(Err(e)) => task_manager.fail_task(task_id, e.to_string()),
                Err(_) => task_manager.fail_task(task_id, "Chat task timed out".to_string()),
            }
        });
    });

    Json(TaskResponse {
        success: true,
        task_id,
        message: "Chat task submitted successfully. Poll /agent/task/{task_id} for results."
            .to_string(),
        estimated_time_ms: Some(3000),
    })
    .into_response()
}

/// Chat streaming handler (SSE)
pub async fn chat_stream_handler(
    State(server_state): State<ServerState>,
    Json(request): Json<ChatRequest>,
) -> impl IntoResponse {
    let agent_name = {
        let rt = server_state.api_state.runtime.read().unwrap();
        rt.character.name.clone()
    };
    info!(
        "INTERACTION_REQUEST_STREAM agent={} room_id={} text_len={} text_preview={}",
        agent_name,
        request.room_id,
        request.text.len(),
        request.text.chars().take(120).collect::<String>()
    );

    if request.text.trim().is_empty() {
        return ApiError::BadRequest("Message text cannot be empty".to_string()).into_response();
    }
    {
        let max_len = std::env::var("API_MAX_MESSAGE_BYTES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(512_000); // ~0.5MB default
        if request.text.len() > max_len {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Message too large",
                    "code": StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
                })),
            )
                .into_response();
        }
    }

    {
        let runtime = server_state.api_state.runtime.clone();
        let mut rt = runtime.write().unwrap();
        let last_key = format!("ui:lastPrompt:{}:last", request.room_id);
        let prev_key = format!("ui:lastPrompt:{}:prev", request.room_id);
        let prev = rt
            .get_setting(&last_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        if let Some(p) = prev {
            rt.set_setting(&prev_key, serde_json::json!(p), false);
        }
        rt.set_setting(&last_key, serde_json::json!(request.text.clone()), false);
        if let Some(owner) = request.entity_id {
            let owner_key = format!("ROOM_OWNER:{}", request.room_id);
            if rt.get_setting(&owner_key).is_none() {
                rt.set_setting(&owner_key, serde_json::json!(owner.to_string()), false);
            }
        }
    }

    // Prepare streaming channel
    let (sender, receiver) = create_text_stream(64);
    let stream_handler = StreamHandler::new(sender.clone());

    // Build SSE stream from receiver (do this early so we can return it in error case)
    let sse_stream: BoxStream<'static, std::result::Result<Event, std::convert::Infallible>> = ReceiverStream::new(receiver)
        .filter_map(|res| async move {
            match res {
                Ok(TextChunk { text, is_final, metadata }) => {
                    let data = serde_json::json!({ "text": text, "final": is_final, "meta": metadata });
                    Some(Ok(Event::default().event(if is_final { "complete" } else { "chunk" }).data(data.to_string())))
                }
                Err(e) => {
                    let data = serde_json::json!({ "error": e.to_string() });
                    Some(Ok(Event::default().event("error").data(data.to_string())))
                }
            }
        })
        .boxed();

    // Clone dependencies
    let runtime = server_state.api_state.runtime.clone();
    let req_clone = request.clone();

    // Limit concurrent streaming requests to prevent resource exhaustion
    // Each request needs ~16MB stack, so 64 concurrent = ~1GB memory
    static STREAM_SEMAPHORE: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    let semaphore = STREAM_SEMAPHORE
        .get_or_init(|| {
            let max_concurrent = std::env::var("MAX_CONCURRENT_STREAMS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(64); // Default 64 concurrent streams (~1GB stack memory)
            std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent))
        })
        .clone();

    // Try to acquire permit, return error if at capacity
    let permit = match semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            // At capacity - send error via the stream
            tokio::spawn(async move {
                let handler = StreamHandler::new(sender);
                let _ = handler
                    .send_error(ZoeyError::other("Server at capacity, please retry"))
                    .await;
            });
            return Sse::new(sse_stream).into_response();
        }
    };

    // Enqueue job to single-thread streaming executor and return SSE immediately
    static STREAM_EXECUTOR: std::sync::OnceLock<
        tokio::sync::mpsc::Sender<(
            tokio::sync::OwnedSemaphorePermit,
            Arc<RwLock<AgentRuntime>>,
            ChatRequest,
            StreamHandler,
        )>,
    > = std::sync::OnceLock::new();
    let tx = STREAM_EXECUTOR
        .get_or_init(|| {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<(
                tokio::sync::OwnedSemaphorePermit,
                Arc<RwLock<AgentRuntime>>,
                ChatRequest,
                StreamHandler,
            )>(256);
            std::thread::Builder::new()
                .name("chat_stream_executor".to_string())
                .stack_size(16 * 1024 * 1024)
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    rt.block_on(async move {
                        while let Some((permit, runtime, req, handler)) = rx.recv().await {
                            let _p = permit;
                            run_chat_stream_job(runtime.clone(), req.clone(), handler).await;
                        }
                    });
                })
                .expect("stream executor thread");
            tx
        })
        .clone();
    let _ = tx
        .send((permit, runtime.clone(), req_clone.clone(), stream_handler))
        .await;
    return Sse::new(sse_stream).into_response();
}

/// Process chat task asynchronously
async fn process_chat_task(
    runtime: Arc<RwLock<AgentRuntime>>,
    request: ChatRequest,
) -> Result<ChatResponse> {
    eprintln!("[TRACE] process_chat_task: START");
    if request
        .metadata
        .get("skip_double_processing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(ChatResponse {
            success: true,
            messages: None,
            error: None,
            metadata: None,
        });
    }
    let entity_id = request.entity_id.unwrap_or_else(Uuid::new_v4);
    let agent_id = {
        let rt = runtime.read().unwrap();
        rt.agent_id
    };
    eprintln!("[TRACE] process_chat_task: agent_id={}", agent_id);

    // Create message memory
    let message = Memory {
        id: Uuid::new_v4(),
        entity_id,
        agent_id,
        room_id: request.room_id,
        content: Content {
            text: request.text.clone(),
            source: Some(request.source.clone()),
            ..Default::default()
        },
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: Some(false),
        similarity: None,
    };

    // Ensure world, room, and entity exist in database (for foreign key constraints)
    let world_id = Uuid::new_v4();
    let adapter_opt = {
        let rt = runtime.read().unwrap();
        let adapter_lock = rt.adapter.read().unwrap();
        adapter_lock.clone()
    };
    if let Some(adapter) = adapter_opt.as_ref() {
            // Create world if needed
            let world = crate::types::World {
                id: world_id,
                name: format!("API World {}", world_id),
                agent_id,
                server_id: None,
                metadata: HashMap::new(),
                created_at: Some(chrono::Utc::now().timestamp()),
            };
            let _ = adapter.ensure_world(&world).await;

            // Create entity if needed
            let entity = crate::types::Entity {
                id: entity_id,
                agent_id,
                name: Some(format!("User {}", entity_id)),
                username: None,
                email: None,
                avatar_url: None,
                metadata: HashMap::new(),
                created_at: Some(chrono::Utc::now().timestamp()),
            };
            let _ = adapter.create_entities(vec![entity]).await;

            // Create room if needed  
            let room_record = crate::types::Room {
                id: request.room_id,
                agent_id: Some(agent_id),
                name: format!("Room {}", request.room_id),
                source: request.source.clone(),
                channel_type: ChannelType::Api,
                channel_id: None,
                server_id: None,
                world_id,
                metadata: HashMap::new(),
                created_at: Some(chrono::Utc::now().timestamp()),
            };
            let _ = adapter.create_room(&room_record).await;

        // Add participant
        let _ = adapter.add_participant(entity_id, request.room_id).await;
    }

    // Create a Room for this conversation
    let room = Room {
        id: request.room_id,
        agent_id: Some(agent_id),
        name: format!("Room {}", request.room_id),
        source: request.source.clone(),
        channel_type: ChannelType::Api,
        channel_id: None,
        server_id: None,
        world_id, // Use the created world
        metadata: HashMap::new(),
        created_at: Some(chrono::Utc::now().timestamp()),
    };

    // Process message through the agent
    eprintln!("[TRACE] process_chat_task: calling MessageProcessor::process_message");
    let processor = MessageProcessor::new(runtime.clone());
    let responses = processor.process_message(message, room).await?;
    eprintln!(
        "[TRACE] process_chat_task: MessageProcessor returned {} responses",
        responses.len()
    );
    let agent_name = {
        let rt = runtime.read().unwrap();
        rt.character.name.clone()
    };
    let preview = responses
        .get(0)
        .map(|m| m.content.text.chars().take(120).collect::<String>())
        .unwrap_or_default();
    info!(
        "[{}] chat completed responses={}, preview={}",
        agent_name,
        responses.len(),
        preview
    );

    Ok(ChatResponse {
        success: true,
        messages: Some(responses),
        error: None,
        metadata: None,
    })
}

/// Process a message through the agent
async fn process_message(
    runtime: Arc<RwLock<AgentRuntime>>,
    message: Memory,
    room: Room,
) -> Result<Vec<Memory>> {
    // Get message processor
    let processor = MessageProcessor::new(runtime.clone());

    // Process message
    processor.process_message(message, room).await
}

/// Execute action handler
pub async fn action_handler(
    State(state): State<ServerState>,
    Json(request): Json<ActionRequest>,
) -> impl IntoResponse {
    let state = state.api_state;
    let agent_name = {
        let rt = state.runtime.read().unwrap();
        rt.character.name.clone()
    };
    info!("[{}] action request action={}", agent_name, request.action);

    // Validate input
    if request.action.trim().is_empty() {
        return ApiError::BadRequest("Action name cannot be empty".to_string()).into_response();
    }

    let runtime = state.runtime.read().unwrap();

    // Find the action
    let actions = runtime.actions.read().unwrap();
    let action = match actions.iter().find(|a| a.name() == request.action) {
        Some(a) => a,
        None => {
            return ApiError::NotFound(format!("Action '{}' not found", request.action))
                .into_response();
        }
    };

    // For now, we'll return a success response
    // In the future, this should actually execute the action
    info!("Would execute action: {}", action.name());

    Json(ActionResponse {
        success: true,
        result: Some(serde_json::json!({
            "action": request.action,
            "status": "acknowledged"
        })),
        error: None,
    })
    .into_response()
}

/// Get state handler using async task pattern
pub async fn state_handler(
    State(server_state): State<ServerState>,
    Json(request): Json<StateRequest>,
) -> impl IntoResponse {
    let agent_name = {
        let rt = server_state.api_state.runtime.read().unwrap();
        rt.character.name.clone()
    };
    info!("[{}] state request room_id={}", agent_name, request.room_id);

    // Create task
    let task_id = server_state.task_manager.create_task();
    let task_manager = server_state.task_manager.clone();
    let runtime = server_state.api_state.runtime.clone();

    // Spawn async task in separate thread to avoid Send requirement
    std::thread::spawn(move || {
        // Create a new tokio runtime for this task
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            task_manager.mark_running(task_id);

            let timeout_res = tokio::time::timeout(
                Duration::from_secs(15),
                process_state_task(runtime, request),
            )
            .await;

            match timeout_res {
                Ok(Ok(response)) => {
                    task_manager.complete_task(task_id, super::task::TaskResult::State(response));
                }
                Ok(Err(e)) => {
                    task_manager.fail_task(task_id, e.to_string());
                }
                Err(_) => {
                    task_manager.fail_task(task_id, "State task timed out".to_string());
                }
            }
        });
    });

    // Return task ID immediately
    Json(TaskResponse {
        success: true,
        task_id,
        message:
            "State composition task submitted successfully. Poll /agent/task/{task_id} for results."
                .to_string(),
        estimated_time_ms: Some(2000), // Estimate 2 seconds
    })
    .into_response()
}

/// Process state composition task asynchronously
async fn process_state_task(
    runtime: Arc<RwLock<AgentRuntime>>,
    request: StateRequest,
) -> Result<StateResponse> {
    let rt = runtime.read().unwrap();
    let entity_id = request.entity_id.unwrap_or_else(Uuid::new_v4);
    let agent_id = rt.agent_id;
    drop(rt); // Release the lock

    // Create a simple message to build state from
    let message = Memory {
        id: Uuid::new_v4(),
        entity_id,
        agent_id,
        room_id: request.room_id,
        content: Content::default(),
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: Some(false),
        similarity: None,
    };

    // Compose state
    let agent_state = {
        let rt = runtime.read().unwrap();
        rt.compose_state(&message, None, false, false).await?
    };
    let agent_name = {
        let rt = runtime.read().unwrap();
        rt.character.name.clone()
    };
    info!(
        "[{}] state composed values={}",
        agent_name,
        agent_state.values.len()
    );

    Ok(StateResponse {
        success: true,
        state: Some(agent_state),
        error: None,
    })
}

/// Task status polling handler
pub async fn task_status_handler(
    State(server_state): State<ServerState>,
    axum::extract::Path(task_id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    debug!("Task status request: task_id={}", task_id);

    match server_state.task_manager.get_task(task_id) {
        Some(task) => {
            // Convert task to response
            let result_json = task
                .result
                .as_ref()
                .map(|r| serde_json::to_value(r).ok())
                .flatten();

            Json(TaskStatusResponse {
                task_id,
                status: format!("{:?}", task.status).to_lowercase(),
                result: result_json,
                error: task.error.clone(),
                duration_ms: task.duration_ms(),
                created_at: format!("{:?}", task.created_at),
                completed_at: task.completed_at.map(|t| format!("{:?}", t)),
            })
            .into_response()
        }
        None => ApiError::NotFound(format!("Task {} not found", task_id)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ContextAddPayload {
    room_id: Uuid,
    key: String,
    value: String,
}

/// Add a lightweight context hint (stored in runtime settings) without blocking
pub async fn context_add_handler(
    State(server_state): State<ServerState>,
    Json(body): Json<ContextAddPayload>,
) -> impl IntoResponse {
    let runtime = server_state.api_state.runtime.clone();
    {
        // Store under a namespaced key so prompts can pick it up later
        let mut rt = runtime.write().unwrap();
        let key = format!("ui:lastThought:{}:{}", body.room_id, body.key);
        rt.set_setting(&key, serde_json::json!(body.value), false);
    }
    Json(serde_json::json!({"success": true})).into_response()
}

#[derive(Deserialize)]
pub struct ContextSavePayload {
    room_id: Uuid,
    steps: Vec<String>,
}

/// Persist multiple thought steps into memories (table: "thoughts")
pub async fn context_save_handler(
    State(server_state): State<ServerState>,
    Json(body): Json<ContextSavePayload>,
) -> impl IntoResponse {
    let runtime = server_state.api_state.runtime.clone();
    let adapter = {
        let rt = runtime.read().unwrap();
        let x = rt.adapter.read().unwrap().clone();
        x
    };
    if let Some(adapter) = adapter {
        for step in body.steps.iter() {
            let mem = Memory {
                id: Uuid::new_v4(),
                entity_id: Uuid::new_v4(),
                agent_id: {
                    let rt = runtime.read().unwrap();
                    rt.agent_id
                },
                room_id: body.room_id,
                content: Content {
                    text: step.clone(),
                    source: Some("simpleui-thought".to_string()),
                    ..Default::default()
                },
                embedding: None,
                metadata: None,
                created_at: chrono::Utc::now().timestamp(),
                unique: Some(false),
                similarity: None,
            };
            if let Err(e) = adapter.create_memory(&mem, "thoughts").await {
                error!("Failed to persist thought step: {}", e);
            }
        }
        return Json(serde_json::json!({"success": true})).into_response();
    }
    ApiError::Internal("No database adapter configured".to_string()).into_response()
}

/// List available characters (from characters directory or current runtime)
pub async fn character_list_handler(State(server_state): State<ServerState>) -> impl IntoResponse {
    // Get current character name from runtime
    let current_character = {
        let rt = server_state.api_state.runtime.read().unwrap();
        rt.character.name.clone()
    };
    
    // For simplicity, scan the 'characters' directory for *.xml files
    let mut list: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir("characters") {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if name.ends_with(".xml") {
                    list.push(name.to_string());
                }
            }
        }
    }
    Json(serde_json::json!({
        "success": true, 
        "characters": list,
        "current": current_character
    })).into_response()
}

/// Select a character by filename (XML) and apply to runtime
pub async fn character_select_handler(
    State(server_state): State<ServerState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(filename) = body.get("filename").and_then(|v| v.as_str()) else {
        return ApiError::BadRequest("Missing filename".to_string()).into_response();
    };
    let path = format!("characters/{}", filename);
    let xml = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            return ApiError::NotFound("Character file not found".to_string()).into_response()
        }
    };

    // Minimal XML extraction for name, bio, lore, knowledge
    fn section<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
        let start = xml.find(&format!("<{}>", tag))?;
        let end = xml.find(&format!("</{}>", tag))?;
        Some(&xml[start + tag.len() + 2..end])
    }
    fn entries(xml: &str, section_name: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(sec) = section(xml, section_name) {
            let mut rest = sec;
            loop {
                if let Some(i) = rest.find("<entry>") {
                    let r = &rest[i + 7..];
                    if let Some(j) = r.find("</entry>") {
                        out.push(r[..j].trim().to_string());
                        rest = &r[j + 8..];
                        continue;
                    }
                }
                break;
            }
        }
        out
    }

    let name = section(&xml, "name")
        .and_then(|s| s.lines().next())
        .unwrap_or("ZoeyAI")
        .trim()
        .to_string();
    let bio = entries(&xml, "bio");
    let lore = entries(&xml, "lore");
    let knowledge = entries(&xml, "knowledge");

    // Apply to runtime
    {
        let mut rt = server_state.api_state.runtime.write().unwrap();
        rt.character.name = name;
        if !bio.is_empty() {
            rt.character.bio = bio;
        }
        if !lore.is_empty() {
            rt.character.lore = lore;
        }
        if !knowledge.is_empty() {
            rt.character.knowledge = knowledge;
        }
    }

    Json(serde_json::json!({"success": true})).into_response()
}

/// API error types
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    RateLimited(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::RateLimited(msg) => (StatusCode::TOO_MANY_REQUESTS, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(serde_json::json!({
            "success": false,
            "error": message,
            "code": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

impl From<ZoeyError> for ApiError {
    fn from(err: ZoeyError) -> Self {
        error!("ZoeyError: {}", err);
        ApiError::Internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_response() {
        let err = ApiError::BadRequest("test error".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
#[derive(Deserialize)]
pub struct DeleteRoomPayload {
    room_id: Uuid,
    entity_id: Uuid,
    #[serde(default)]
    purge_memories: bool,
}

/// Delete room and optionally purge persisted memories for that room
pub async fn delete_room_handler(
    State(server_state): State<ServerState>,
    Json(body): Json<DeleteRoomPayload>,
) -> impl IntoResponse {
    let runtime = server_state.api_state.runtime.clone();
    let (adapter, authorized) = {
        let rt = runtime.read().unwrap();
        let owner_key = format!("ROOM_OWNER:{}", body.room_id);
        let authorized = rt
            .get_setting(&owner_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .map(|owner| owner == body.entity_id.to_string())
            .unwrap_or(false);
        (rt.get_adapter(), authorized)
    };

    if !authorized {
        return ApiError::Forbidden("Only the room owner can delete this room".to_string())
            .into_response();
    }

    if let Some(adapter) = adapter {
        if body.purge_memories {
            if let Ok(memories) = adapter
                .get_memories(MemoryQuery {
                    room_id: Some(body.room_id),
                    table_name: "messages".to_string(),
                    ..Default::default()
                })
                .await
            {
                for m in memories {
                    let _ = adapter.remove_memory(m.id, "messages").await;
                }
            }
            if let Ok(thoughts) = adapter
                .get_memories(MemoryQuery {
                    room_id: Some(body.room_id),
                    table_name: "thoughts".to_string(),
                    ..Default::default()
                })
                .await
            {
                for t in thoughts {
                    let _ = adapter.remove_memory(t.id, "thoughts").await;
                }
            }
            
            // Delete knowledge documents for this room
            if let Err(e) = delete_room_knowledge(body.room_id) {
                warn!("Failed to delete room knowledge: {}", e);
            }
        }
        return Json(serde_json::json!({"success": true})).into_response();
    }
    ApiError::Internal("No database adapter configured".to_string()).into_response()
}

/// Memory work item for the background queue
struct MemoryWorkItem {
    memory: Memory,
    response_tx: Option<tokio::sync::oneshot::Sender<std::result::Result<Uuid, String>>>,
}

/// Global memory queue - initialized once, processes all memory operations
static MEMORY_QUEUE: OnceLock<tokio::sync::mpsc::Sender<MemoryWorkItem>> = OnceLock::new();

/// Initialize the memory worker pool (call once at startup)
pub fn init_memory_worker_pool(runtime: Arc<RwLock<crate::AgentRuntime>>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<MemoryWorkItem>(1000);

    // Store the sender globally
    let _ = MEMORY_QUEUE.set(tx);

    // Spawn worker threads (pool of 4)
    for i in 0..4 {
        let runtime = runtime.clone();
        let mut rx_clone = {
            // Create a new channel and swap
            let (new_tx, new_rx) = tokio::sync::mpsc::channel::<MemoryWorkItem>(1000);
            // We need to share rx - use a different approach
            // Actually, mpsc::Receiver is not Clone, so we need Arc<Mutex>
            // Let's use a simpler approach - one worker
            if i > 0 {
                continue;
            } // Only spawn one worker for now
            rx
        };

        std::thread::Builder::new()
            .name(format!("memory_worker_{}", i))
            .stack_size(64 * 1024 * 1024) // 64MB stack
            .spawn(move || {
                eprintln!("[DEBUG] memory_worker: thread started");

                // Use blocking recv instead of async to minimize stack usage
                let rt_handle = std::sync::Arc::new(std::sync::Mutex::new(
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap(),
                ));

                loop {
                    // Blocking receive
                    let work = {
                        let rt = rt_handle.lock().unwrap();
                        rt.block_on(rx_clone.recv())
                    };

                    let work = match work {
                        Some(w) => w,
                        None => break,
                    };

                    eprintln!(
                        "[DEBUG] memory_worker: got work item, id={}",
                        work.memory.id
                    );

                    // Get adapter
                    let adapter = {
                        let rt_guard = runtime.read().unwrap();
                        rt_guard.get_adapter()
                    };

                    if let Some(adapter) = adapter {
                        let mem = work.memory.clone();
                        let memory_id = mem.id;

                        // Use the main tokio runtime's spawn_blocking which has proper stack
                        // Send work to main runtime via a channel
                        let (result_tx, result_rx) = std::sync::mpsc::channel();

                        // Spawn the DB work on the main tokio runtime
                        let rt = rt_handle.lock().unwrap();
                        rt.spawn(async move {
                            let result = adapter.create_memory(&mem, "messages").await;
                            let _ = result_tx.send(result.map(|_| ()).map_err(|e| e.to_string()));
                        });
                        drop(rt);

                        // Wait for result with timeout
                        let result =
                            match result_rx.recv_timeout(std::time::Duration::from_secs(10)) {
                                Ok(r) => r.map(|_| memory_id),
                                Err(_) => Err("Memory creation timed out".to_string()),
                            };

                        if let Some(tx) = work.response_tx {
                            let _ = tx.send(result);
                        }
                        eprintln!("[DEBUG] memory_worker: work item processed");
                    } else {
                        if let Some(tx) = work.response_tx {
                            let _ = tx.send(Ok(work.memory.id));
                        }
                    }
                }
                eprintln!("[DEBUG] memory_worker: thread exiting");
            })
            .ok();

        break; // Only one worker for now since we can't clone rx
    }
}

/// Memory creation handler - async endpoint for all clients to persist memories
/// Uses a background worker queue to avoid stack overflow
pub async fn memory_create_handler(
    State(server_state): State<ServerState>,
    Json(request): Json<super::types::MemoryCreateRequest>,
) -> Response {
    let runtime = server_state.api_state.runtime.clone();

    // Get agent_id
    let agent_id = {
        let rt = runtime.read().unwrap();
        rt.agent_id
    };

    // Build memory object
    let memory_id = Uuid::new_v4();
    let mut content = Content {
        text: request.text,
        source: Some(request.source),
        ..Default::default()
    };
    for (k, v) in request.metadata {
        content.metadata.insert(k, v);
    }

    let memory = Memory {
        id: memory_id,
        entity_id: request.entity_id,
        agent_id,
        room_id: request.room_id,
        content,
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: Some(false),
        similarity: None,
    };

    // Get or initialize the queue
    let queue = match MEMORY_QUEUE.get() {
        Some(q) => q,
        None => {
            // Initialize on first use if not already done
            init_memory_worker_pool(runtime.clone());
            match MEMORY_QUEUE.get() {
                Some(q) => q,
                None => {
                    return Json(super::types::MemoryCreateResponse {
                        success: false,
                        memory_id: None,
                        error: Some("Memory queue not initialized".to_string()),
                    })
                    .into_response();
                }
            }
        }
    };

    // Create response channel
    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();

    // Send to queue
    if queue
        .send(MemoryWorkItem {
            memory,
            response_tx: Some(resp_tx),
        })
        .await
        .is_err()
    {
        return Json(super::types::MemoryCreateResponse {
            success: false,
            memory_id: None,
            error: Some("Memory queue full".to_string()),
        })
        .into_response();
    }

    // Wait for result with timeout
    match tokio::time::timeout(Duration::from_secs(10), resp_rx).await {
        Ok(Ok(Ok(id))) => Json(super::types::MemoryCreateResponse {
            success: true,
            memory_id: Some(id),
            error: None,
        })
        .into_response(),
        Ok(Ok(Err(e))) => Json(super::types::MemoryCreateResponse {
            success: false,
            memory_id: None,
            error: Some(e),
        })
        .into_response(),
        _ => Json(super::types::MemoryCreateResponse {
            success: false,
            memory_id: None,
            error: Some("Memory operation failed".to_string()),
        })
        .into_response(),
    }
}

/// Fire-and-forget memory creation (for clients that don't need confirmation)
pub async fn memory_create_async(
    runtime: Arc<RwLock<crate::AgentRuntime>>,
    room_id: Uuid,
    entity_id: Uuid,
    text: String,
    source: String,
) {
    let agent_id = {
        let rt = runtime.read().unwrap();
        rt.agent_id
    };

    let memory = Memory {
        id: Uuid::new_v4(),
        entity_id,
        agent_id,
        room_id,
        content: Content {
            text,
            source: Some(source),
            ..Default::default()
        },
        embedding: None,
        metadata: None,
        created_at: chrono::Utc::now().timestamp(),
        unique: Some(false),
        similarity: None,
    };

    if let Some(queue) = MEMORY_QUEUE.get() {
        let _ = queue
            .send(MemoryWorkItem {
                memory,
                response_tx: None, // Fire and forget
            })
            .await;
    }
}

// ============================================================================
// Knowledge Ingestion System
// ============================================================================

/// Security limits for knowledge ingestion
const KNOWLEDGE_MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024; // 10 MB max
const KNOWLEDGE_MAX_FILENAME_LENGTH: usize = 255;
const KNOWLEDGE_MIN_CONTENT_LENGTH: usize = 10; // Minimum content length
const KNOWLEDGE_MAX_CHUNKS_PER_DOC: usize = 1000; // Max chunks per document
const KNOWLEDGE_CHUNK_SIZE: usize = 512; // Characters per chunk
const KNOWLEDGE_CHUNK_OVERLAP: usize = 64; // Overlap between chunks

/// Represents a stored knowledge document
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeDocument {
    pub id: Uuid,
    pub room_id: Uuid,
    pub entity_id: Uuid,
    pub agent_id: Uuid,
    pub filename: String,
    pub doc_type: String,
    pub content: String,
    pub chunks: Vec<KnowledgeChunk>,
    pub word_count: usize,
    pub created_at: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A chunk of knowledge for retrieval
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub text: String,
    pub index: usize,
    pub char_start: usize,
    pub char_end: usize,
}

/// BM25 search implementation for knowledge retrieval
/// Uses the Okapi BM25 algorithm for term-based ranking
mod bm25 {
    use rust_stemmers::{Algorithm, Stemmer};
    use std::collections::HashMap;
    
    pub struct BM25Search {
        corpus: Vec<String>,
        stemmer: Stemmer,
        k1: f64,
        b: f64,
    }
    
    impl BM25Search {
        pub fn new(corpus: Vec<String>) -> Self {
            Self {
                corpus,
                stemmer: Stemmer::create(Algorithm::English),
                k1: 1.2,
                b: 0.75,
            }
        }
        
        pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f64)> {
            if self.corpus.is_empty() {
                return Vec::new();
            }
            
            let query_terms = self.tokenize_and_stem(query);
            let avg_doc_len = self.average_document_length();
            
            let mut scores: Vec<(usize, f64)> = self.corpus
                .iter()
                .enumerate()
                .map(|(idx, doc)| {
                    let score = self.bm25_score(&query_terms, doc, avg_doc_len);
                    (idx, score)
                })
                .collect();
            
            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            
            scores
                .into_iter()
                .take(top_k)
                .filter(|(_, score)| *score > 0.0)
                .map(|(idx, score)| (self.corpus[idx].clone(), score))
                .collect()
        }
        
        fn bm25_score(&self, query_terms: &[String], document: &str, avg_doc_len: f64) -> f64 {
            let doc_terms = self.tokenize_and_stem(document);
            let doc_len = doc_terms.len() as f64;
            
            if doc_len == 0.0 {
                return 0.0;
            }
            
            let term_freqs = self.term_frequencies(&doc_terms);
            
            query_terms
                .iter()
                .map(|term| {
                    let tf = *term_freqs.get(term).unwrap_or(&0) as f64;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let idf = self.inverse_document_frequency(term);
                    
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator = tf + self.k1 * (1.0 - self.b + self.b * (doc_len / avg_doc_len.max(1.0)));
                    
                    idf * (numerator / denominator)
                })
                .sum()
        }
        
        fn inverse_document_frequency(&self, term: &str) -> f64 {
            let n = self.corpus.len() as f64;
            let df = self.corpus
                .iter()
                .filter(|doc| self.tokenize_and_stem(doc).contains(&term.to_string()))
                .count() as f64;
            
            ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
        }
        
        fn tokenize_and_stem(&self, text: &str) -> Vec<String> {
            text.to_lowercase()
                .split_whitespace()
                .filter(|word| word.len() > 2)
                .map(|word| {
                    let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
                    self.stemmer.stem(cleaned).to_string()
                })
                .collect()
        }
        
        fn term_frequencies(&self, terms: &[String]) -> HashMap<String, usize> {
            let mut freqs = HashMap::new();
            for term in terms {
                *freqs.entry(term.clone()).or_insert(0) += 1;
            }
            freqs
        }
        
        fn average_document_length(&self) -> f64 {
            if self.corpus.is_empty() {
                return 1.0;
            }
            
            let total: usize = self.corpus
                .iter()
                .map(|doc| self.tokenize_and_stem(doc).len())
                .sum();
            
            (total as f64 / self.corpus.len() as f64).max(1.0)
        }
    }
}

/// Knowledge store with file-based persistence (per room)
/// Documents are stored in: .zoey/db/knowledge/{room_id}.json
static KNOWLEDGE_STORE: OnceLock<Arc<RwLock<HashMap<Uuid, Vec<KnowledgeDocument>>>>> =
    OnceLock::new();

/// Get the knowledge storage directory
fn get_knowledge_dir() -> std::path::PathBuf {
    let base = std::env::var("KNOWLEDGE_STORAGE_DIR")
        .unwrap_or_else(|_| ".zoey/db/knowledge".to_string());
    std::path::PathBuf::from(base)
}

/// Get the path for a room's knowledge file
fn get_room_knowledge_path(room_id: Uuid) -> std::path::PathBuf {
    get_knowledge_dir().join(format!("{}.json", room_id))
}

/// Load knowledge from disk for a specific room
fn load_room_knowledge(room_id: Uuid) -> Vec<KnowledgeDocument> {
    let path = get_room_knowledge_path(room_id);
    if !path.exists() {
        return Vec::new();
    }
    
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<Vec<KnowledgeDocument>>(&content) {
                Ok(docs) => {
                    info!("KNOWLEDGE_LOADED room_id={} documents={}", room_id, docs.len());
                    docs
                }
                Err(e) => {
                    error!("KNOWLEDGE_LOAD_ERROR room_id={} error={}", room_id, e);
                    Vec::new()
                }
            }
        }
        Err(e) => {
            error!("KNOWLEDGE_READ_ERROR room_id={} error={}", room_id, e);
            Vec::new()
        }
    }
}

/// Save knowledge to disk for a specific room
fn save_room_knowledge(room_id: Uuid, documents: &[KnowledgeDocument]) -> std::result::Result<(), String> {
    let dir = get_knowledge_dir();
    
    // Create directory if needed
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(format!("Failed to create knowledge directory: {}", e));
    }
    
    let path = get_room_knowledge_path(room_id);
    
    let json = match serde_json::to_string_pretty(documents) {
        Ok(j) => j,
        Err(e) => return Err(format!("Failed to serialize knowledge: {}", e)),
    };
    
    match std::fs::write(&path, json) {
        Ok(_) => {
            info!("KNOWLEDGE_SAVED room_id={} documents={} path={:?}", room_id, documents.len(), path);
            Ok(())
        }
        Err(e) => Err(format!("Failed to write knowledge file: {}", e)),
    }
}

/// Delete knowledge for a room (called when room is deleted)
pub fn delete_room_knowledge(room_id: Uuid) -> std::result::Result<(), String> {
    // Remove from memory
    {
        let store = get_knowledge_store();
        if let Ok(mut store_guard) = store.write() {
            store_guard.remove(&room_id);
        };
    }
    
    // Remove from disk
    let path = get_room_knowledge_path(room_id);
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            error!("KNOWLEDGE_DELETE_ERROR room_id={} error={}", room_id, e);
            return Err(format!("Failed to delete knowledge file: {}", e));
        }
        info!("KNOWLEDGE_DELETED room_id={}", room_id);
    }
    
    Ok(())
}

fn get_knowledge_store() -> Arc<RwLock<HashMap<Uuid, Vec<KnowledgeDocument>>>> {
    KNOWLEDGE_STORE
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

/// Get documents for a room, loading from disk if not in memory
fn get_room_documents(room_id: Uuid) -> Vec<KnowledgeDocument> {
    let store = get_knowledge_store();
    
    // Check memory first
    {
        let store_guard = store.read().unwrap();
        if let Some(docs) = store_guard.get(&room_id) {
            return docs.clone();
        }
    }
    
    // Load from disk
    let docs = load_room_knowledge(room_id);
    
    // Cache in memory
    if !docs.is_empty() {
        let mut store_guard = store.write().unwrap();
        store_guard.insert(room_id, docs.clone());
    }
    
    docs
}

/// Validate and sanitize filename
fn validate_filename(filename: &str) -> std::result::Result<String, String> {
    // Check length
    if filename.is_empty() {
        return Err("Filename cannot be empty".to_string());
    }
    if filename.len() > KNOWLEDGE_MAX_FILENAME_LENGTH {
        return Err(format!(
            "Filename too long (max {} characters)",
            KNOWLEDGE_MAX_FILENAME_LENGTH
        ));
    }

    // Sanitize: remove path components and dangerous characters
    let sanitized: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-' || *c == ' ')
        .collect();

    // Extract just the filename (no path)
    let sanitized = sanitized
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&sanitized)
        .trim()
        .to_string();

    if sanitized.is_empty() {
        return Err("Invalid filename after sanitization".to_string());
    }

    // Prevent directory traversal
    if sanitized.contains("..") {
        return Err("Invalid filename: path traversal detected".to_string());
    }

    Ok(sanitized)
}

/// Validate content for security issues
fn validate_content(content: &str) -> std::result::Result<(), String> {
    // Check size
    if content.len() > KNOWLEDGE_MAX_CONTENT_SIZE {
        return Err(format!(
            "Content too large (max {} bytes)",
            KNOWLEDGE_MAX_CONTENT_SIZE
        ));
    }

    if content.len() < KNOWLEDGE_MIN_CONTENT_LENGTH {
        return Err(format!(
            "Content too short (min {} characters)",
            KNOWLEDGE_MIN_CONTENT_LENGTH
        ));
    }

    // Check for null bytes (could indicate binary content)
    if content.contains('\0') {
        return Err("Content contains invalid null bytes".to_string());
    }

    // Check for excessive non-printable characters (possible binary/malformed data)
    let non_printable_count = content
        .chars()
        .filter(|c| !c.is_ascii_graphic() && !c.is_whitespace())
        .count();

    if non_printable_count > content.len() / 10 {
        return Err("Content contains too many non-printable characters".to_string());
    }

    Ok(())
}

/// Split content into chunks for retrieval
fn chunk_content(content: &str, document_id: Uuid) -> Vec<KnowledgeChunk> {
    let mut chunks = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let total_len = chars.len();

    if total_len == 0 {
        return chunks;
    }

    let mut start = 0;
    let mut index = 0;

    while start < total_len && index < KNOWLEDGE_MAX_CHUNKS_PER_DOC {
        let end = (start + KNOWLEDGE_CHUNK_SIZE).min(total_len);

        // Try to break at sentence/paragraph boundary
        let actual_end = if end < total_len {
            // Look for a good break point
            let slice: String = chars[start..end].iter().collect();
            let last_period = slice.rfind(|c| c == '.' || c == '!' || c == '?' || c == '\n');
            match last_period {
                Some(pos) if pos > KNOWLEDGE_CHUNK_SIZE / 2 => start + pos + 1,
                _ => end,
            }
        } else {
            end
        };

        let chunk_text: String = chars[start..actual_end].iter().collect();
        let trimmed = chunk_text.trim();

        if !trimmed.is_empty() {
            chunks.push(KnowledgeChunk {
                id: Uuid::new_v4(),
                document_id,
                text: trimmed.to_string(),
                index,
                char_start: start,
                char_end: actual_end,
            });
            index += 1;
        }

        // Move start with overlap
        start = if actual_end >= total_len {
            total_len
        } else {
            (actual_end).saturating_sub(KNOWLEDGE_CHUNK_OVERLAP)
        };

        // Prevent infinite loop
        if start == 0 && actual_end == 0 {
            break;
        }
    }

    chunks
}

/// Scrub PII from content before storage (basic protection)
fn scrub_pii_basic(content: &str) -> String {
    use regex::Regex;

    let mut scrubbed = content.to_string();

    // SSN pattern
    if let Ok(re) = Regex::new(r"\b\d{3}-\d{2}-\d{4}\b") {
        scrubbed = re.replace_all(&scrubbed, "[SSN_REDACTED]").to_string();
    }

    // Credit card pattern (simplified)
    if let Ok(re) = Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b") {
        scrubbed = re.replace_all(&scrubbed, "[CC_REDACTED]").to_string();
    }

    // API keys pattern
    if let Ok(re) = Regex::new(r"\b(sk-|api[_-]?key[:\s=]+)[A-Za-z0-9]{20,}\b") {
        scrubbed = re.replace_all(&scrubbed, "[API_KEY_REDACTED]").to_string();
    }

    scrubbed
}

/// Extract text from PDF bytes
fn extract_text_from_pdf(bytes: &[u8]) -> std::result::Result<String, String> {
    // Write bytes to a temporary approach using pdf_extract
    // pdf_extract works with paths, so we use a temp file approach
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!("knowledge_pdf_{}.pdf", Uuid::new_v4()));
    
    // Write bytes to temp file
    if let Err(e) = std::fs::write(&temp_path, bytes) {
        return Err(format!("Failed to write temp PDF file: {}", e));
    }
    
    // Extract text
    let result = pdf_extract::extract_text(&temp_path)
        .map_err(|e| format!("PDF extraction error: {}", e));
    
    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);
    
    result
}

/// Extract text from Excel bytes (xlsx or xls)
fn extract_text_from_excel(bytes: &[u8]) -> std::result::Result<String, String> {
    use calamine::{Reader, Xlsx, Data};
    use std::io::Cursor;
    
    let cursor = Cursor::new(bytes.to_vec());
    
    // Try to open as xlsx (most common format)
    let mut workbook: Xlsx<_> = match Xlsx::new(cursor) {
        Ok(wb) => wb,
        Err(e) => {
            return Err(format!("Failed to open Excel file: {}", e));
        }
    };
    
    let mut all_text = Vec::new();
    
    // Get sheet names first
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    
    for sheet_name in sheet_names {
        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            all_text.push(format!("## Sheet: {}\n", sheet_name));
            
            for row in range.rows() {
                let row_text: Vec<String> = row
                    .iter()
                    .map(|cell| {
                        match cell {
                            Data::Empty => String::new(),
                            Data::String(s) => s.clone(),
                            Data::Float(f) => f.to_string(),
                            Data::Int(i) => i.to_string(),
                            Data::Bool(b) => b.to_string(),
                            Data::Error(e) => format!("#ERR:{:?}", e),
                            Data::DateTime(dt) => format!("{}", dt),
                            Data::DateTimeIso(s) => s.clone(),
                            Data::DurationIso(s) => s.clone(),
                        }
                    })
                    .collect();
                
                // Only add non-empty rows
                let row_str = row_text.join(" | ");
                if !row_str.trim().is_empty() && row_str.trim() != "|" {
                    all_text.push(row_str);
                }
            }
            
            all_text.push(String::new()); // Blank line between sheets
        }
    }
    
    let result = all_text.join("\n");
    
    if result.trim().is_empty() {
        return Err("Excel file appears to be empty".to_string());
    }
    
    Ok(result)
}

/// Knowledge ingestion handler
/// 
/// This endpoint accepts documents and processes them through the knowledge pipeline:
/// 1. Validates filename, content, and document type
/// 2. Sanitizes content and scrubs basic PII patterns
/// 3. Chunks the document for efficient retrieval
/// 4. Stores in the knowledge store for the room
/// 
/// Security features:
/// - Size limits on content and filename
/// - Content validation (no binary, no null bytes)
/// - Path traversal prevention
/// - PII scrubbing
/// - Per-room isolation
pub async fn knowledge_ingest_handler(
    State(server_state): State<ServerState>,
    Json(request): Json<super::types::KnowledgeIngestRequest>,
) -> Response {
    use super::types::{KnowledgeDocumentType, KnowledgeIngestResponse};

    let runtime = server_state.api_state.runtime.clone();
    let mut warnings: Vec<String> = Vec::new();

    // Log the ingestion attempt
    info!(
        "KNOWLEDGE_INGEST_START room_id={} entity_id={} filename={}",
        request.room_id, request.entity_id, request.filename
    );

    // Validate filename
    let filename = match validate_filename(&request.filename) {
        Ok(f) => f,
        Err(e) => {
            error!("KNOWLEDGE_INGEST_ERROR filename validation failed: {}", e);
            return Json(KnowledgeIngestResponse::error(format!(
                "Invalid filename: {}",
                e
            )))
            .into_response();
        }
    };

    // Determine document type
    let doc_type = match request.document_type {
        Some(dt) => dt,
        None => match KnowledgeDocumentType::from_filename(&filename) {
            Some(dt) => dt,
            None => {
                error!(
                    "KNOWLEDGE_INGEST_ERROR unsupported file type for {}",
                    filename
                );
                return Json(KnowledgeIngestResponse::error(
                    "Unsupported file type. Allowed: .txt, .md, .csv, .json",
                ))
                .into_response();
            }
        },
    };

    // Validate MIME type if provided
    if let Some(ref mime) = request.mime_type {
        if !doc_type.valid_mime_type(mime) {
            warnings.push(format!(
                "MIME type '{}' may not match document type {:?}",
                mime, doc_type
            ));
        }
    }

    // Decode content - handle binary formats (PDF, Excel) specially
    let content = if request.base64_encoded {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let bytes = match STANDARD.decode(&request.content) {
            Ok(b) => b,
            Err(_) => {
                error!("KNOWLEDGE_INGEST_ERROR invalid base64 encoding");
                return Json(KnowledgeIngestResponse::error("Invalid base64 encoding"))
                    .into_response();
            }
        };

        // Handle binary formats
        match doc_type {
            KnowledgeDocumentType::Pdf => {
                // Extract text from PDF
                match extract_text_from_pdf(&bytes) {
                    Ok(text) => text,
                    Err(e) => {
                        error!("KNOWLEDGE_INGEST_ERROR PDF extraction failed: {}", e);
                        return Json(KnowledgeIngestResponse::error(format!(
                            "Failed to extract text from PDF: {}",
                            e
                        )))
                        .into_response();
                    }
                }
            }
            KnowledgeDocumentType::Excel => {
                // Extract text from Excel
                match extract_text_from_excel(&bytes) {
                    Ok(text) => text,
                    Err(e) => {
                        error!("KNOWLEDGE_INGEST_ERROR Excel extraction failed: {}", e);
                        return Json(KnowledgeIngestResponse::error(format!(
                            "Failed to extract text from Excel: {}",
                            e
                        )))
                        .into_response();
                    }
                }
            }
            _ => {
                // For text-based formats, convert bytes to string
                match String::from_utf8(bytes) {
                    Ok(s) => s,
                    Err(_) => {
                        error!("KNOWLEDGE_INGEST_ERROR base64 content is not valid UTF-8");
                        return Json(KnowledgeIngestResponse::error(
                            "Base64 content is not valid UTF-8 text",
                        ))
                        .into_response();
                    }
                }
            }
        }
    } else {
        request.content.clone()
    };

    // Validate content
    if let Err(e) = validate_content(&content) {
        error!("KNOWLEDGE_INGEST_ERROR content validation failed: {}", e);
        return Json(KnowledgeIngestResponse::error(format!(
            "Invalid content: {}",
            e
        )))
        .into_response();
    }

    // Scrub basic PII patterns
    let scrubbed_content = scrub_pii_basic(&content);
    if scrubbed_content.len() != content.len() {
        warnings.push("Some PII patterns were automatically redacted".to_string());
    }

    // Get agent ID
    let agent_id = {
        let rt = runtime.read().unwrap();
        rt.agent_id
    };

    // Create document
    let document_id = Uuid::new_v4();
    let word_count = scrubbed_content.split_whitespace().count();

    // Chunk the content
    let chunks = chunk_content(&scrubbed_content, document_id);
    let chunks_count = chunks.len();

    if chunks.is_empty() {
        error!("KNOWLEDGE_INGEST_ERROR no valid chunks created");
        return Json(KnowledgeIngestResponse::error(
            "Content produced no valid chunks",
        ))
        .into_response();
    }

    // Create document record
    let document = KnowledgeDocument {
        id: document_id,
        room_id: request.room_id,
        entity_id: request.entity_id,
        agent_id,
        filename: filename.clone(),
        doc_type: format!("{:?}", doc_type),
        content: scrubbed_content,
        chunks,
        word_count,
        created_at: chrono::Utc::now().timestamp(),
        metadata: request.metadata,
    };

    // Store in knowledge store (memory + disk)
    {
        let store = get_knowledge_store();
        let mut store_guard = store.write().unwrap();
        let room_docs = store_guard.entry(request.room_id).or_insert_with(|| {
            // Load existing documents from disk
            load_room_knowledge(request.room_id)
        });
        room_docs.push(document);
        
        // Persist to disk
        if let Err(e) = save_room_knowledge(request.room_id, room_docs) {
            warnings.push(format!("Warning: Failed to persist knowledge: {}", e));
        }
    }

    info!(
        "KNOWLEDGE_INGEST_SUCCESS document_id={} filename={} chunks={} words={}",
        document_id, filename, chunks_count, word_count
    );

    Json(
        KnowledgeIngestResponse::success(document_id, chunks_count, word_count)
            .with_warnings(warnings),
    )
    .into_response()
}

/// Query knowledge for a room
pub async fn knowledge_query_handler(
    State(server_state): State<ServerState>,
    Json(request): Json<super::types::KnowledgeQueryRequest>,
) -> Response {
    use super::types::{KnowledgeChunkResult, KnowledgeQueryResponse};

    info!(
        "KNOWLEDGE_QUERY room_id={} query_len={}",
        request.room_id,
        request.query.len()
    );

    use bm25::BM25Search;
    
    // Get documents (loads from disk if needed)
    let documents = get_room_documents(request.room_id);
    
    if documents.is_empty() {
        return Json(KnowledgeQueryResponse {
            success: true,
            results: Some(vec![]),
            total_documents: Some(0),
            error: None,
        })
        .into_response();
    }

    // Build corpus and chunk mapping
    let mut corpus: Vec<String> = Vec::new();
    let mut chunk_map: Vec<(Uuid, Uuid, String)> = Vec::new(); // (chunk_id, doc_id, filename)
    
    for doc in &documents {
        for chunk in &doc.chunks {
            corpus.push(chunk.text.clone());
            chunk_map.push((chunk.id, doc.id, doc.filename.clone()));
        }
    }

    // Use BM25 for proper retrieval
    let bm25 = BM25Search::new(corpus.clone());
    let bm25_results = bm25.search(&request.query, request.max_results);

    // Map results back to KnowledgeChunkResult
    let final_results: Vec<KnowledgeChunkResult> = bm25_results
        .into_iter()
        .filter_map(|(text, score)| {
            // Find the chunk info
            corpus.iter().position(|c| c == &text).map(|idx| {
                let (chunk_id, doc_id, filename) = &chunk_map[idx];
                KnowledgeChunkResult {
                    id: *chunk_id,
                    document_id: *doc_id,
                    text,
                    score,
                    filename: Some(filename.clone()),
                }
            })
        })
        .collect();

    Json(KnowledgeQueryResponse {
        success: true,
        results: Some(final_results),
        total_documents: Some(documents.len()),
        error: None,
    })
    .into_response()
}

/// List knowledge documents for a room
pub async fn knowledge_list_handler(
    State(server_state): State<ServerState>,
    axum::extract::Path(room_id): axum::extract::Path<String>,
) -> Response {
    let room_uuid = match Uuid::parse_str(&room_id) {
        Ok(id) => id,
        Err(_) => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Invalid room ID"
            }))
            .into_response();
        }
    };

    // Get documents (loads from disk if needed)
    let documents = get_room_documents(room_uuid);

    let doc_list: Vec<serde_json::Value> = documents
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "filename": d.filename,
                "docType": d.doc_type,
                "wordCount": d.word_count,
                "chunksCount": d.chunks.len(),
                "createdAt": d.created_at,
            })
        })
        .collect();

    Json(serde_json::json!({
        "success": true,
        "documents": doc_list,
        "totalDocuments": doc_list.len(),
    }))
    .into_response()
}

/// Retrieve relevant knowledge context for a query (internal API)
/// 
/// This function is used by the chat handlers to inject relevant knowledge
/// into the prompt context before sending to the LLM.
/// 
/// Uses BM25 algorithm for proper relevance scoring.
/// Only returns chunks that score above a threshold to keep prompts concise.
pub fn retrieve_knowledge_context(room_id: Uuid, query: &str, max_chunks: usize) -> Option<String> {
    use bm25::BM25Search;
    
    // Get documents (loads from disk if needed)
    let documents = get_room_documents(room_id);
    if documents.is_empty() {
        return None;
    }

    // Build corpus from all chunks with source tracking
    let mut corpus: Vec<String> = Vec::new();
    let mut chunk_sources: Vec<(usize, String, Uuid)> = Vec::new(); // (chunk_idx, filename, chunk_id)
    
    for doc in &documents {
        for chunk in &doc.chunks {
            let chunk_idx = corpus.len();
            corpus.push(chunk.text.clone());
            chunk_sources.push((chunk_idx, doc.filename.clone(), chunk.id));
        }
    }

    if corpus.is_empty() {
        return None;
    }

    // Use BM25 for proper retrieval
    let bm25 = BM25Search::new(corpus.clone());
    let results = bm25.search(query, max_chunks * 2); // Get more than needed for filtering

    if results.is_empty() {
        info!("KNOWLEDGE_NO_MATCHES room_id={} query_len={}", room_id, query.len());
        return None;
    }

    // Filter by minimum score threshold and limit results
    const MIN_BM25_SCORE: f64 = 0.5;
    let filtered_results: Vec<(String, f64, String)> = results
        .into_iter()
        .filter(|(_, score)| *score >= MIN_BM25_SCORE)
        .take(max_chunks)
        .map(|(text, score)| {
            // Find the source filename for this chunk
            let filename = corpus.iter()
                .position(|c| c == &text)
                .and_then(|idx| chunk_sources.iter().find(|(i, _, _)| *i == idx))
                .map(|(_, f, _)| f.clone())
                .unwrap_or_else(|| "unknown".to_string());
            (text, score, filename)
        })
        .collect();

    if filtered_results.is_empty() {
        info!(
            "KNOWLEDGE_LOW_RELEVANCE room_id={} query_len={} (scores below threshold)",
            room_id, query.len()
        );
        return None;
    }

    info!(
        "KNOWLEDGE_MATCHED room_id={} chunks={} top_score={:.2}",
        room_id,
        filtered_results.len(),
        filtered_results.first().map(|r| r.1).unwrap_or(0.0)
    );

    // Format as concise context
    let context_parts: Vec<String> = filtered_results
        .iter()
        .map(|(text, _score, filename)| {
            // Truncate very long chunks to save prompt space
            let truncated = if text.len() > 600 {
                format!("{}...", &text[..600])
            } else {
                text.clone()
            };
            format!("[{}]: {}", filename, truncated)
        })
        .collect();

    Some(format!(
        "**Relevant excerpts from case documents:**\n\n{}",
        context_parts.join("\n\n")
    ))
}

/// Get a summary of knowledge available in a room
pub fn get_knowledge_summary(room_id: Uuid) -> Option<String> {
    let store = get_knowledge_store();
    let store_guard = store.read().ok()?;

    let documents = store_guard.get(&room_id)?;
    if documents.is_empty() {
        return None;
    }

    let total_words: usize = documents.iter().map(|d| d.word_count).sum();
    let total_chunks: usize = documents.iter().map(|d| d.chunks.len()).sum();

    let doc_list: Vec<String> = documents
        .iter()
        .map(|d| format!("- **{}** ({} words, {} chunks)", d.filename, d.word_count, d.chunks.len()))
        .collect();

    Some(format!(
        "### Case Knowledge Base\n\n\
         **{} documents** | **{} words** | **{} searchable chunks**\n\n\
         {}",
        documents.len(),
        total_words,
        total_chunks,
        doc_list.join("\n")
    ))
}

// ============================================================================
// TRAINING & RLHF HANDLERS
// ============================================================================

/// Get training statistics
pub async fn training_statistics_handler(
    State(state): State<ServerState>,
) -> impl IntoResponse {
    let runtime = state.api_state.runtime.read().unwrap();
    
    // Get training collector from runtime if available
    if let Some(collector) = runtime.get_training_collector() {
        let stats = collector.get_statistics();
        let response = serde_json::json!({
            "status": "success",
            "data": {
                "type": "statistics",
                "totalSamples": stats.total_samples,
                "highQualityCount": stats.high_quality_count,
                "mediumQualityCount": stats.medium_quality_count,
                "lowQualityCount": stats.low_quality_count,
                "withThoughtsCount": stats.with_thoughts_count,
                "withFeedbackCount": stats.with_feedback_count,
                "avgQualityScore": stats.avg_quality_score,
                "avgFeedbackScore": stats.avg_feedback_score,
                "categories": stats.categories,
                "tags": stats.tags,
                "rlhfEnabled": collector.is_rlhf_enabled()
            }
        });
        (StatusCode::OK, Json(response))
    } else {
        let response = serde_json::json!({
            "status": "error",
            "code": "NOT_AVAILABLE",
            "message": "Training collector not initialized"
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(response))
    }
}

/// Add feedback to a training sample
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFeedbackRequest {
    pub sample_id: Uuid,
    pub feedback_score: f32,
    pub feedback_text: Option<String>,
}

pub async fn training_feedback_handler(
    State(state): State<ServerState>,
    Json(payload): Json<AddFeedbackRequest>,
) -> Response {
    // Get collector without holding lock across await
    let collector = {
        let runtime = state.api_state.runtime.read().unwrap();
        runtime.get_training_collector()
    };
    
    if let Some(collector) = collector {
        match collector.add_feedback(payload.sample_id, payload.feedback_score, payload.feedback_text).await {
            Ok(_) => {
                info!("Training feedback added for sample {}: score={}", payload.sample_id, payload.feedback_score);
                let response = serde_json::json!({
                    "status": "success",
                    "data": {
                        "type": "feedbackAdded",
                        "sampleId": payload.sample_id.to_string()
                    }
                });
                (StatusCode::OK, Json(response)).into_response()
            }
            Err(e) => {
                error!("Failed to add feedback: {}", e);
                let response = serde_json::json!({
                    "status": "error",
                    "code": "FEEDBACK_FAILED",
                    "message": e.to_string()
                });
                (StatusCode::BAD_REQUEST, Json(response)).into_response()
            }
        }
    } else {
        let response = serde_json::json!({
            "status": "error",
            "code": "NOT_AVAILABLE",
            "message": "Training collector not initialized"
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(response)).into_response()
    }
}

/// Export training data request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDataRequest {
    pub format: Option<String>,
    pub include_negative: Option<bool>,
}

pub async fn training_export_handler(
    State(state): State<ServerState>,
    Json(payload): Json<ExportDataRequest>,
) -> Response {
    // Get collector without holding lock across await
    let collector = {
        let runtime = state.api_state.runtime.read().unwrap();
        runtime.get_training_collector()
    };
    
    if let Some(collector) = collector {
        let format = payload.format.as_deref().unwrap_or("jsonl");
        
        let export_result = match format.to_lowercase().as_str() {
            "jsonl" => collector.export_jsonl().await,
            "alpaca" => collector.export_alpaca().await,
            "sharegpt" => collector.export_sharegpt().await,
            "openai" => collector.export_openai().await,
            _ => {
                let response = serde_json::json!({
                    "status": "error",
                    "code": "INVALID_FORMAT",
                    "message": "Unsupported format. Use: jsonl, alpaca, sharegpt, openai"
                });
                return (StatusCode::BAD_REQUEST, Json(response)).into_response();
            }
        };
        
        match export_result {
            Ok(data) => {
                let sample_count = data.lines().count();
                info!("Training data exported: {} samples in {} format", sample_count, format);
                let response = serde_json::json!({
                    "status": "success",
                    "data": {
                        "type": "exportedData",
                        "format": format,
                        "sampleCount": sample_count,
                        "data": data
                    }
                });
                (StatusCode::OK, Json(response)).into_response()
            }
            Err(e) => {
                error!("Failed to export training data: {}", e);
                let response = serde_json::json!({
                    "status": "error",
                    "code": "EXPORT_FAILED",
                    "message": e.to_string()
                });
                (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response()
            }
        }
    } else {
        let response = serde_json::json!({
            "status": "error",
            "code": "NOT_AVAILABLE",
            "message": "Training collector not initialized"
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(response)).into_response()
    }
}

/// List training samples
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSamplesQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub min_quality: Option<f32>,
}

pub async fn training_samples_handler(
    State(state): State<ServerState>,
    axum::extract::Query(query): axum::extract::Query<ListSamplesQuery>,
) -> impl IntoResponse {
    let runtime = state.api_state.runtime.read().unwrap();
    
    if let Some(collector) = runtime.get_training_collector() {
        let all_samples = collector.get_samples();
        let total = all_samples.len();
        
        // Filter by quality if specified
        let filtered: Vec<_> = if let Some(min_q) = query.min_quality {
            all_samples.into_iter().filter(|s| s.quality_score >= min_q).collect()
        } else {
            all_samples
        };
        
        // Paginate
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(50).min(500);
        
        let samples: Vec<_> = filtered.into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        let response = serde_json::json!({
            "status": "success",
            "data": {
                "type": "sampleList",
                "samples": samples,
                "total": total,
                "offset": offset
            }
        });
        (StatusCode::OK, Json(response))
    } else {
        let response = serde_json::json!({
            "status": "error",
            "code": "NOT_AVAILABLE",
            "message": "Training collector not initialized"
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(response))
    }
}

/// Start training job request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTrainingRequest {
    pub format: Option<String>,
    pub config: Option<TrainingJobConfig>,
    /// Dynamic backend configuration from UI (overrides env config)
    pub backend: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingJobConfig {
    pub name: Option<String>,
    pub min_quality: Option<f32>,
    pub include_negative: Option<bool>,
    pub auto_save: Option<bool>,
}

/// Global training jobs storage
static TRAINING_JOBS: OnceLock<Arc<RwLock<HashMap<Uuid, TrainingJobStatus>>>> = OnceLock::new();

/// Get the global training jobs store
pub fn get_training_jobs() -> &'static Arc<RwLock<HashMap<Uuid, TrainingJobStatus>>> {
    TRAINING_JOBS.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingJobStatus {
    pub job_id: Uuid,
    pub state: String,
    pub progress: f32,
    pub samples_processed: usize,
    pub total_samples: usize,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
    pub result_path: Option<String>,
}

pub async fn training_start_handler(
    State(state): State<ServerState>,
    Json(payload): Json<StartTrainingRequest>,
) -> Response {
    // Check if MCP service is available and proxy to it
    let has_mcp_service = {
        let runtime = state.api_state.runtime.read().unwrap();
        runtime.get_service("mcp-server").is_some()
    };
    
    // Clone config for potential use in fallback
    let config_clone = payload.config.clone();
    
    if has_mcp_service {
        // Proxy to MCP server endpoint
        let mcp_port = std::env::var("MCP_PORT").unwrap_or_else(|_| "8443".to_string());
        let mcp_host = std::env::var("MCP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let mcp_url = format!("http://{}:{}/mcp/training/start", mcp_host, mcp_port);
        
        // Get auth token if configured
        let auth_token = std::env::var("MCP_AUTH_TOKEN").ok();
        
        let client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| HttpClient::new());
        let mut request = client.post(&mcp_url);
        
        // Add auth header if configured
        if let Some(token) = auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        // Convert payload format - MCP expects TrainingFormat enum and required config fields
        let format_str = payload.format.as_deref().unwrap_or("jsonl");
        let config = payload.config.unwrap_or(TrainingJobConfig {
            name: Some(format!("Training job {}", Uuid::new_v4())),
            min_quality: Some(0.6),
            include_negative: Some(true),
            auto_save: Some(true),
        });
        
        // MCP expects config.name as required String, not Option
        let config_name = config.name.unwrap_or_else(|| format!("Training job {}", Uuid::new_v4()));
        let config_min_quality = config.min_quality.unwrap_or(0.6);
        let config_include_negative = config.include_negative.unwrap_or(true);
        let config_auto_save = config.auto_save.unwrap_or(true);
        
        // TrainingFormat enum serializes as lowercase: "jsonl", "alpaca", "sharegpt", "openai", "custom"
        // (due to #[serde(rename_all = "lowercase")])
        let format_enum = format_str.to_lowercase();
        
        // Build MCP payload with optional dynamic backend config from UI
        let mut mcp_payload = serde_json::json!({
            "format": format_enum,
            "config": {
                "name": config_name,
                "minQuality": config_min_quality,
                "includeNegative": config_include_negative,
                "autoSave": config_auto_save,
            }
        });
        
        // If UI sends a dynamic backend configuration, include it
        if let Some(backend_config) = &payload.backend {
            mcp_payload["backend"] = backend_config.clone();
            info!("Using dynamic backend config from UI: {:?}", backend_config);
        }
        
        match request.json(&mcp_payload).send().await {
            Ok(response) => {
                let status = response.status();
                // Read response as text first for debugging
                match response.text().await {
                    Ok(text) => {
                        // Check if we got HTML instead of JSON (MCP server not running on this port)
                        if text.trim_start().starts_with("<!") || text.trim_start().starts_with("<html") {
                            warn!("MCP server not accessible on {}:{} (got HTML response). Falling back to export-only.", mcp_host, mcp_port);
                            // Fall through to export-only behavior below
                        } else {
                            // Try to parse as JSON
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    // Convert MCP response format to agent API format
                                    // MCP response format: {"status": "success", "data": {"type": "jobStarted", "jobId": "...", "estimatedDuration": 30}}
                                    let job_id = data.get("data")
                                        .and_then(|d| d.get("jobId"))
                                        .and_then(|id| id.as_str())
                                        .or_else(|| {
                                            data.get("data")
                                                .and_then(|d| d.get("job_id"))
                                                .and_then(|id| id.as_str())
                                        });
                                    
                                    let response_json = serde_json::json!({
                                        "status": "success",
                                        "data": {
                                            "type": "jobStarted",
                                            "jobId": job_id.unwrap_or("unknown"),
                                            "estimatedDuration": 30
                                        }
                                    });
                                    
                                    return (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::ACCEPTED), Json(response_json)).into_response();
                                }
                                Err(e) => {
                                    warn!("Failed to parse MCP response as JSON: {}. Response text (first 200 chars): {}. Falling back to export-only.", e, text.chars().take(200).collect::<String>());
                                    // Fall through to export-only behavior below
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read MCP response body: {}. Falling back to export-only.", e);
                        // Fall through to export-only behavior below
                    }
                }
            }
            Err(e) => {
                warn!("Failed to proxy to MCP server: {}. Falling back to export-only.", e);
                // Fall through to export-only behavior below
            }
        }
    } else {
        // No MCP service, fall through to export-only behavior
    }
    
    // Fallback: export data only (original behavior)
    let collector = {
        let runtime = state.api_state.runtime.read().unwrap();
        runtime.get_training_collector()
    };
    
    if let Some(collector) = collector {
        let job_id = Uuid::new_v4();
        let format = payload.format.as_deref().unwrap_or("jsonl");
        let _config = config_clone.unwrap_or(TrainingJobConfig {
            name: Some(format!("Training job {}", job_id)),
            min_quality: Some(0.6),
            include_negative: Some(true),
            auto_save: Some(true),
        });
        
        let job_status = TrainingJobStatus {
            job_id,
            state: "pending".to_string(),
            progress: 0.0,
            samples_processed: 0,
            total_samples: collector.count(),
            started_at: chrono::Utc::now().timestamp(),
            completed_at: None,
            error: None,
            result_path: None,
        };
        
        // Store job
        {
            let mut jobs = get_training_jobs().write().unwrap();
            jobs.insert(job_id, job_status.clone());
        }
        
        // Spawn background task to process training
        let collector_clone = collector.clone();
        let format_clone = format.to_string();
        tokio::spawn(async move {
            // Update to running
            {
                let mut jobs = get_training_jobs().write().unwrap();
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.state = "running".to_string();
                }
            }
            
            // Export data
            let training_format = match format_clone.as_str() {
                "alpaca" => crate::training::TrainingFormat::Alpaca,
                "sharegpt" => crate::training::TrainingFormat::ShareGpt,
                "openai" => crate::training::TrainingFormat::OpenAi,
                _ => crate::training::TrainingFormat::Jsonl,
            };
            
            match collector_clone.save_to_file(training_format).await {
                Ok(path) => {
                    let mut jobs = get_training_jobs().write().unwrap();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.state = "completed".to_string();
                        job.progress = 1.0;
                        job.completed_at = Some(chrono::Utc::now().timestamp());
                        job.result_path = Some(path.to_string_lossy().to_string());
                    }
                    info!("Training job {} completed: {}", job_id, path.display());
                }
                Err(e) => {
                    let mut jobs = get_training_jobs().write().unwrap();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.state = "failed".to_string();
                        job.completed_at = Some(chrono::Utc::now().timestamp());
                        job.error = Some(e.to_string());
                    }
                    error!("Training job {} failed: {}", job_id, e);
                }
            }
        });
        
        info!("Training job {} started", job_id);
        let response = serde_json::json!({
            "status": "success",
            "data": {
                "type": "jobStarted",
                "jobId": job_id.to_string(),
                "estimatedDuration": 30
            }
        });
        (StatusCode::ACCEPTED, Json(response)).into_response()
    } else {
        let response = serde_json::json!({
            "status": "error",
            "code": "NOT_AVAILABLE",
            "message": "Training collector not initialized"
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(response)).into_response()
    }
}

/// Get training job status
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatusQuery {
    pub job_id: Uuid,
}

pub async fn training_job_status_handler(
    axum::extract::Query(query): axum::extract::Query<JobStatusQuery>,
) -> impl IntoResponse {
    let jobs = get_training_jobs().read().unwrap();
    
    if let Some(job) = jobs.get(&query.job_id) {
        let response = serde_json::json!({
            "status": "success",
            "data": {
                "type": "jobStatus",
                "job": job
            }
        });
        (StatusCode::OK, Json(response))
    } else {
        let response = serde_json::json!({
            "status": "error",
            "code": "NOT_FOUND",
            "message": format!("Job {} not found", query.job_id)
        });
        (StatusCode::NOT_FOUND, Json(response))
    }
}

/// List all training jobs
pub async fn training_jobs_handler() -> impl IntoResponse {
    let jobs = get_training_jobs().read().unwrap();
    let job_list: Vec<_> = jobs.values().cloned().collect();
    let total = job_list.len();
    
    let response = serde_json::json!({
        "status": "success",
        "data": {
            "type": "jobList",
            "jobs": job_list,
            "total": total
        }
    });
    (StatusCode::OK, Json(response))
}
