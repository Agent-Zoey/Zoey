//! ElizaOS Cloud AI integration plugin for ZoeyOS

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

/// ElizaOS Cloud API client
pub struct ElizaOSClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ElizaOSClient {
    /// Create a new ElizaOS Cloud client with shared connection pool
    pub fn new(api_key: String) -> Self {
        Self {
            client: get_http_client(),
            api_key,
            base_url: "https://www.elizacloud.ai/api/v1".to_string(),
        }
    }

    /// Create a new ElizaOS Cloud client with custom base URL
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            client: get_http_client(),
            api_key,
            base_url,
        }
    }

    /// Generate text using ElizaOS Cloud (OpenAI-compatible API)
    /// Returns (text, usage, ttft_ms) where ttft_ms is time-to-first-token in milliseconds
    pub async fn generate_text(
        &self,
        params: GenerateTextParams,
        start_time: std::time::Instant,
    ) -> Result<(String, Option<ElizaOSUsage>, Option<u64>)> {
        let model = params.model.clone().unwrap_or_else(|| {
            std::env::var("ELIZAOS_CLOUD_SMALL_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string())
        });

        let request = ElizaOSRequest {
            model,
            messages: vec![ElizaOSMessage {
                role: "user".to_string(),
                content: params.prompt,
            }],
            max_tokens: params.max_tokens,
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
                "ElizaOS Cloud API error: {}",
                error_text
            )));
        }

        let mut assembled = String::new();
        let mut buffer = String::new();
        let mut total_usage: Option<ElizaOSUsage> = None;
        let mut ttft_ms: Option<u64> = None;

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
                                    // Record time-to-first-token on first content
                                    if ttft_ms.is_none() && !content.is_empty() {
                                        ttft_ms = Some(start_time.elapsed().as_millis() as u64);
                                    }
                                    assembled.push_str(content);
                                }
                            }
                        }
                    }
                    // Capture usage if present
                    if let Some(usage) = json.get("usage") {
                        if let Ok(u) = serde_json::from_value::<ElizaOSUsage>(usage.clone()) {
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

        Ok((assembled, total_usage, ttft_ms))
    }

    /// Generate embeddings using ElizaOS Cloud
    pub async fn generate_embedding(
        &self,
        text: &str,
        model: Option<String>,
    ) -> Result<Vec<f32>> {
        let model = model.unwrap_or_else(|| {
            std::env::var("ELIZAOS_CLOUD_EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".to_string())
        });

        let request = ElizaOSEmbeddingRequest {
            model,
            input: text.to_string(),
        };

        let resp = self
            .client
            .post(&format!("{}/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ZoeyError::model(e.to_string()))?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(ZoeyError::model(format!(
                "ElizaOS Cloud API error: {}",
                error_text
            )));
        }

        let embedding_response: ElizaOSEmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| ZoeyError::model(format!("Failed to parse embedding response: {}", e)))?;

        Ok(embedding_response.data[0].embedding.clone())
    }
}

#[derive(Debug, Serialize)]
struct ElizaOSRequest {
    model: String,
    messages: Vec<ElizaOSMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ElizaOSMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ElizaOSResponse {
    choices: Vec<ElizaOSChoice>,
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    model: String,
    usage: Option<ElizaOSUsage>,
}

/// Token usage from ElizaOS Cloud API
#[derive(Debug, Deserialize, Clone)]
pub struct ElizaOSUsage {
    /// Number of tokens in the prompt
    pub prompt_tokens: usize,
    /// Number of tokens in the completion
    pub completion_tokens: usize,
    /// Total tokens used
    pub total_tokens: usize,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ElizaOSChoice {
    message: ElizaOSMessage,
    #[allow(dead_code)]
    index: usize,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ElizaOSEmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct ElizaOSEmbeddingResponse {
    data: Vec<ElizaOSEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct ElizaOSEmbeddingData {
    embedding: Vec<f32>,
    #[allow(dead_code)]
    index: usize,
}

/// ElizaOS Cloud plugin
pub struct ElizaOSPlugin {
    api_key: Option<String>,
    base_url: Option<String>,
    small_model: Option<String>,
    large_model: Option<String>,
    embedding_model: Option<String>,
}

impl ElizaOSPlugin {
    /// Create a new ElizaOS Cloud plugin
    pub fn new() -> Self {
        Self {
            api_key: None,
            base_url: None,
            small_model: None,
            large_model: None,
            embedding_model: None,
        }
    }

    /// Create with API key
    pub fn with_api_key(api_key: String) -> Self {
        Self {
            api_key: Some(api_key),
            base_url: None,
            small_model: None,
            large_model: None,
            embedding_model: None,
        }
    }

    /// Create with full configuration
    pub fn with_config(
        api_key: String,
        base_url: Option<String>,
        small_model: Option<String>,
        large_model: Option<String>,
        embedding_model: Option<String>,
    ) -> Self {
        Self {
            api_key: Some(api_key),
            base_url,
            small_model,
            large_model,
            embedding_model,
        }
    }
}

impl Default for ElizaOSPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl zoey_core::types::Provider for ElizaOSPlugin {
    fn name(&self) -> &str {
        "elizaos"
    }
    fn capabilities(&self) -> Option<Vec<String>> {
        Some(vec!["CHAT".to_string(), "VISION".to_string(), "EMBEDDING".to_string()])
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
impl Plugin for ElizaOSPlugin {
    fn name(&self) -> &str {
        "elizaos"
    }

    fn description(&self) -> &str {
        "ElizaOS Cloud AI LLM integration"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        tracing::info!("ElizaOS Cloud plugin initialized");
        Ok(())
    }

    fn models(&self) -> HashMap<String, ModelHandler> {
        let api_key = self.api_key.clone().unwrap_or_default();
        let base_url = self.base_url.clone();
        let small_model = self.small_model.clone();
        let large_model = self.large_model.clone();
        let embedding_model = self.embedding_model.clone();

        let small_handler = create_elizaos_handler(api_key.clone(), base_url.clone(), small_model, "TEXT_SMALL".to_string());
        let large_handler = create_elizaos_handler(api_key.clone(), base_url.clone(), large_model, "TEXT_LARGE".to_string());
        let embedding_handler = create_elizaos_embedding_handler(api_key, base_url, embedding_model);

        let mut models = HashMap::new();
        models.insert("TEXT_SMALL".to_string(), small_handler);
        models.insert("TEXT_LARGE".to_string(), large_handler);
        models.insert("TEXT_EMBEDDING".to_string(), embedding_handler);

        models
    }
}

/// Create ElizaOS Cloud text generation handler
fn create_elizaos_handler(
    api_key: String,
    base_url: Option<String>,
    model_override: Option<String>,
    model_type: String,
) -> ModelHandler {
    Arc::new(move |params: ModelHandlerParams| {
        let api_key = api_key.clone();
        let base_url = base_url.clone();
        let model_override = model_override.clone();
        let model_type = model_type.clone();
        Box::pin(async move {
            let gen_params = params.params.clone();
            
            // Determine model based on type and overrides
            let model = if let Some(custom_model) = model_override {
                custom_model
            } else {
                gen_params.model.clone().unwrap_or_else(|| {
                    match model_type.as_str() {
                        "TEXT_SMALL" => std::env::var("ELIZAOS_CLOUD_SMALL_MODEL")
                            .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
                        "TEXT_LARGE" => std::env::var("ELIZAOS_CLOUD_LARGE_MODEL")
                            .unwrap_or_else(|_| "gpt-4o".to_string()),
                        _ => "gpt-4o-mini".to_string(),
                    }
                })
            };

            // Track start time for latency measurement
            let start_time = std::time::Instant::now();

            let effective_api_key = if let Some(runtime) = params
                .runtime
                .downcast_ref::<zoey_core::runtime::AgentRuntime>()
            {
                zoey_core::secrets::get_secret(&runtime.character, "ELIZAOS_CLOUD_API_KEY")
                    .unwrap_or_else(|| api_key.clone())
            } else {
                api_key.clone()
            };

            let effective_base_url = if let Some(runtime) = params
                .runtime
                .downcast_ref::<zoey_core::runtime::AgentRuntime>()
            {
                zoey_core::secrets::get_secret(&runtime.character, "ELIZAOS_CLOUD_BASE_URL")
                    .or_else(|| base_url.clone())
                    .unwrap_or_else(|| "https://www.elizacloud.ai/api/v1".to_string())
            } else {
                base_url.unwrap_or_else(|| "https://www.elizacloud.ai/api/v1".to_string())
            };

            let client = if effective_base_url != "https://www.elizacloud.ai/api/v1" {
                ElizaOSClient::with_base_url(effective_api_key, effective_base_url)
            } else {
                ElizaOSClient::new(effective_api_key)
            };

            let (text, usage, ttft_ms) = client.generate_text(gen_params.clone(), start_time).await?;

            // Calculate total latency
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
                            ttft_ms,
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
                                "elizaos",
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

/// Create ElizaOS Cloud embedding handler
fn create_elizaos_embedding_handler(
    api_key: String,
    base_url: Option<String>,
    model_override: Option<String>,
) -> ModelHandler {
    Arc::new(move |params: ModelHandlerParams| {
        let api_key = api_key.clone();
        let base_url = base_url.clone();
        let model_override = model_override.clone();
        Box::pin(async move {
            let gen_params = params.params.clone();
            
            // For embeddings, we use the prompt as the text to embed
            let text = gen_params.prompt;
            
            let model = model_override.unwrap_or_else(|| {
                std::env::var("ELIZAOS_CLOUD_EMBEDDING_MODEL")
                    .unwrap_or_else(|_| "text-embedding-3-small".to_string())
            });

            let effective_api_key = if let Some(runtime) = params
                .runtime
                .downcast_ref::<zoey_core::runtime::AgentRuntime>()
            {
                zoey_core::secrets::get_secret(&runtime.character, "ELIZAOS_CLOUD_API_KEY")
                    .unwrap_or_else(|| api_key.clone())
            } else {
                api_key.clone()
            };

            let effective_base_url = if let Some(runtime) = params
                .runtime
                .downcast_ref::<zoey_core::runtime::AgentRuntime>()
            {
                zoey_core::secrets::get_secret(&runtime.character, "ELIZAOS_CLOUD_BASE_URL")
                    .or_else(|| base_url.clone())
                    .unwrap_or_else(|| "https://www.elizacloud.ai/api/v1".to_string())
            } else {
                base_url.unwrap_or_else(|| "https://www.elizacloud.ai/api/v1".to_string())
            };

            let client = if effective_base_url != "https://www.elizacloud.ai/api/v1" {
                ElizaOSClient::with_base_url(effective_api_key, effective_base_url)
            } else {
                ElizaOSClient::new(effective_api_key)
            };

            let embedding = client.generate_embedding(&text, Some(model)).await?;
            
            // Return embedding as JSON string (since ModelHandler returns String)
            Ok(serde_json::to_string(&embedding)
                .map_err(|e| ZoeyError::model(format!("Failed to serialize embedding: {}", e)))?)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elizaos_plugin_creation() {
        let plugin = ElizaOSPlugin::new();
        assert_eq!(zoey_core::Plugin::name(&plugin), "elizaos");
    }

    #[test]
    fn test_elizaos_plugin_models() {
        let plugin = ElizaOSPlugin::with_api_key("test_key".to_string());
        let models = plugin.models();

        assert!(models.contains_key("TEXT_SMALL"));
        assert!(models.contains_key("TEXT_LARGE"));
        assert!(models.contains_key("TEXT_EMBEDDING"));
    }
}
