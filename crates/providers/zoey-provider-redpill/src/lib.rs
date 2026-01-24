//! RedPill AI integration plugin for ZoeyOS

#![warn(missing_docs)]
#![warn(clippy::all)]

use async_trait::async_trait;
use zoey_core::{types::*, ZoeyError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Shared HTTP client for connection pooling
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// Get or initialize the shared HTTP client
fn get_http_client() -> Client {
    HTTP_CLIENT
        .get_or_init(|| {
            Client::builder()
                .pool_max_idle_per_host(50)
                .pool_idle_timeout(std::time::Duration::from_secs(300))
                .tcp_keepalive(std::time::Duration::from_secs(60))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to create HTTP client")
        })
        .clone()
}

/// RedPill API client
pub struct RedpillClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl RedpillClient {
    /// Create a new RedPill client with shared connection pool
    pub fn new(api_key: String) -> Self {
        Self {
            client: get_http_client(),
            api_key,
            base_url: "https://api.redpill.ai/v1".to_string(),
        }
    }

    /// Create a new RedPill client with custom base URL
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            client: get_http_client(),
            api_key,
            base_url,
        }
    }

    /// Generate text using RedPill (OpenAI-compatible API)
    pub async fn generate_text(
        &self,
        params: GenerateTextParams,
    ) -> Result<(String, Option<RedpillUsage>)> {
        let model = params.model.clone().unwrap_or_else(|| {
            std::env::var("REDPILL_MODEL").unwrap_or_else(|_| "x-ai/grok-4.1-fast".to_string())
        });

        // Newer models (GPT-5, O3, O4) require max_completion_tokens instead of max_tokens
        let uses_new_token_param = model.contains("gpt-5")
            || model.contains("o3")
            || model.contains("o4");

        let (max_tokens, max_completion_tokens) = if uses_new_token_param {
            (None, params.max_tokens)
        } else {
            (params.max_tokens, None)
        };

        let request = RedpillRequest {
            model,
            messages: vec![RedpillMessage {
                role: "user".to_string(),
                content: params.prompt,
            }],
            max_tokens,
            max_completion_tokens,
            temperature: params.temperature,
            stop: params.stop,
            stream: Some(true),
        };

        let mut resp = self
            .client
            .post(&format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ZoeyError::model(e.to_string()))?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(ZoeyError::model(format!(
                "RedPill API error: {}",
                error_text
            )));
        }

        let mut assembled = String::new();
        let mut buffer = String::new();
        let mut total_usage: Option<RedpillUsage> = None;

        while let Ok(opt) = resp.chunk().await {
            let chunk = match opt {
                Some(c) => c,
                None => break,
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
                if payload.is_empty() || payload == "[DONE]" {
                    continue;
                }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
                    // Handle streaming delta
                    if let Some(choices) = json.get("choices").and_then(|v| v.as_array()) {
                        for choice in choices {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                    assembled.push_str(content);
                                }
                            }
                        }
                    }
                    // Capture usage if present
                    if let Some(usage) = json.get("usage") {
                        if let Ok(u) = serde_json::from_value::<RedpillUsage>(usage.clone()) {
                            total_usage = Some(u);
                        }
                    }
                }
            }
            buffer = tail.to_string();
        }

        // Capture rate-limit headers
        let _rate_limit =
            zoey_core::observability::rest::extract_rate_limit_from_headers(resp.headers());

        Ok((assembled, total_usage))
    }
}

#[derive(Debug, Serialize)]
struct RedpillRequest {
    model: String,
    messages: Vec<RedpillMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RedpillMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RedpillResponse {
    choices: Vec<RedpillChoice>,
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    model: String,
    usage: Option<RedpillUsage>,
}

/// Token usage from RedPill API
#[derive(Debug, Deserialize, Clone)]
pub struct RedpillUsage {
    /// Number of tokens in the prompt
    pub prompt_tokens: usize,
    /// Number of tokens in the completion
    pub completion_tokens: usize,
    /// Total tokens used
    pub total_tokens: usize,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RedpillChoice {
    message: RedpillMessage,
    #[allow(dead_code)]
    index: usize,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

/// RedPill plugin
pub struct RedpillPlugin {
    api_key: Option<String>,
}

impl RedpillPlugin {
    /// Create a new RedPill plugin
    pub fn new() -> Self {
        Self { api_key: None }
    }

    /// Create with API key
    pub fn with_api_key(api_key: String) -> Self {
        Self {
            api_key: Some(api_key),
        }
    }
}

impl Default for RedpillPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl zoey_core::types::Provider for RedpillPlugin {
    fn name(&self) -> &str {
        "redpill"
    }
    fn capabilities(&self) -> Option<Vec<String>> {
        Some(vec!["CHAT".to_string(), "VISION".to_string()])
    }
    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<ProviderResult> {
        Ok(ProviderResult::default())
    }
}

#[async_trait]
impl Plugin for RedpillPlugin {
    fn name(&self) -> &str {
        "redpill"
    }

    fn description(&self) -> &str {
        "RedPill AI LLM integration"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        tracing::info!("RedPill plugin initialized");
        Ok(())
    }

    fn models(&self) -> HashMap<String, ModelHandler> {
        let api_key = self.api_key.clone().unwrap_or_default();

        let handler = create_redpill_handler(api_key);

        let mut models = HashMap::new();
        models.insert("TEXT_SMALL".to_string(), handler.clone());
        models.insert("TEXT_MEDIUM".to_string(), handler.clone());
        models.insert("TEXT_LARGE".to_string(), handler);

        models
    }
}

/// Create RedPill text generation handler
fn create_redpill_handler(api_key: String) -> ModelHandler {
    Arc::new(move |params: ModelHandlerParams| {
        let api_key = api_key.clone();
        Box::pin(async move {
            let gen_params = params.params.clone();
            let model = gen_params.model.clone().unwrap_or_else(|| {
                std::env::var("REDPILL_MODEL").unwrap_or_else(|_| "x-ai/grok-4.1-fast".to_string())
            });

            // Track start time for latency measurement
            let start_time = std::time::Instant::now();

            let effective_api_key = if let Some(runtime) = params
                .runtime
                .downcast_ref::<zoey_core::runtime::AgentRuntime>()
            {
                zoey_core::secrets::get_secret(&runtime.character, "REDPILL_API_KEY")
                    .unwrap_or_else(|| api_key.clone())
            } else {
                api_key.clone()
            };
            let client = RedpillClient::new(effective_api_key);
            let (text, usage) = client.generate_text(gen_params.clone()).await?;

            // Calculate latency
            let latency_ms = start_time.elapsed().as_millis() as u64;

            // Extract token usage and record cost
            if let Some(usage) = usage {
                // Try to access runtime and record cost
                if let Some(runtime) = params
                    .runtime
                    .downcast_ref::<zoey_core::runtime::AgentRuntime>()
                {
                    // Clone observability components before dropping the lock
                    let (cost_tracker, security_monitor) = {
                        let obs_lock = runtime.observability.read().unwrap();
                        (
                            obs_lock.as_ref().and_then(|obs| obs.cost_tracker.clone()),
                            obs_lock
                                .as_ref()
                                .and_then(|obs| obs.security_monitor.clone()),
                        )
                    };

                    // Check for PII violations in prompt
                    if let Some(ref monitor) = security_monitor {
                        let _ = monitor
                            .check_pii_violation(
                                runtime.agent_id,
                                None,
                                &gen_params.prompt,
                                "prompt",
                            )
                            .await;
                    }

                    // Check for PII violations in completion
                    if let Some(ref monitor) = security_monitor {
                        let _ = monitor
                            .check_pii_violation(runtime.agent_id, None, &text, "completion")
                            .await;
                    }

                    // Record cost
                    if let Some(cost_tracker) = cost_tracker {
                        let context = zoey_core::observability::LLMCallContext {
                            agent_id: runtime.agent_id,
                            user_id: None,
                            conversation_id: None,
                            action_name: None,
                            evaluator_name: None,
                            temperature: gen_params.temperature,
                            cached_tokens: None,
                            ttft_ms: None,
                            prompt_hash: Some(zoey_core::observability::compute_prompt_hash(
                                &gen_params.prompt,
                            )),
                            prompt_preview: Some(
                                zoey_core::observability::compute_prompt_preview(
                                    &gen_params.prompt,
                                ),
                            ),
                        };

                        let _ = cost_tracker
                            .record_llm_call(
                                "redpill",
                                &model,
                                usage.prompt_tokens,
                                usage.completion_tokens,
                                latency_ms,
                                runtime.agent_id,
                                context,
                            )
                            .await
                            .map_err(|e| {
                                tracing::warn!("Failed to record LLM cost: {}", e);
                                e
                            });

                        // Check for cost anomalies
                        if let Some(ref monitor) = security_monitor {
                            let hourly_cost = cost_tracker.get_hourly_cost(runtime.agent_id).await;
                            let _ = monitor
                                .check_cost_anomaly(runtime.agent_id, hourly_cost, "hourly")
                                .await;
                        }
                    }
                }
            }

            Ok(text)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redpill_plugin_creation() {
        let plugin = RedpillPlugin::new();
        assert_eq!(zoey_core::Plugin::name(&plugin), "redpill");
    }

    #[test]
    fn test_redpill_plugin_models() {
        let plugin = RedpillPlugin::with_api_key("test_key".to_string());
        let models = plugin.models();

        assert!(models.contains_key("TEXT_SMALL"));
        assert!(models.contains_key("TEXT_MEDIUM"));
        assert!(models.contains_key("TEXT_LARGE"));
    }
}
