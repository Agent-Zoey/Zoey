//! Local LLM Integration Plugin
//!
//! Supports local model inference via:
//! - Ollama (llama.cpp wrapper)
//! - llama.cpp HTTP server
//! - LocalAI
//! - Text generation web UI
//!
//! Prioritizes local models for government/HIPAA compliance

#![warn(missing_docs)]
#![warn(clippy::all)]

use async_trait::async_trait;
use zoey_core::infrastructure::sanitization::ValidationRules;
use zoey_core::{types::*, ZoeyError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub mod router;
pub use router::{HardwareConstraints, ModelCapability, ModelRouter, RoutingPreference, TaskType};

/// Shared HTTP client for connection pooling to local LLM servers
static HTTP_CLIENT: OnceLock<Arc<Client>> = OnceLock::new();

/// Get or initialize the shared HTTP client for local LLM connections
/// Returns Arc<Client> to avoid cloning and maintain connection pooling
fn get_http_client() -> Arc<Client> {
    HTTP_CLIENT
        .get_or_init(|| {
            Arc::new(
                Client::builder()
                    .pool_max_idle_per_host(50)
                    .pool_idle_timeout(std::time::Duration::from_secs(300))
                    .tcp_keepalive(std::time::Duration::from_secs(60))
                    .timeout(std::time::Duration::from_secs(300))
                    .connect_timeout(std::time::Duration::from_secs(10))
                    .build()
                    .unwrap_or_else(|e| {
                        panic!(
                            "Failed to create HTTP client: {}. This is a configuration error.",
                            e
                        )
                    }),
            )
        })
        .clone()
}

/// Local LLM backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalBackend {
    /// Ollama (https://ollama.ai)
    Ollama,
    /// llama.cpp HTTP server
    LlamaCpp,
    /// LocalAI (https://localai.io)
    LocalAI,
    /// Text generation web UI
    TextGenWebUI,
}

/// Ollama API request
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

/// Ollama generation options
#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<usize>,
}

/// Ollama API response
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
    #[allow(dead_code)]
    done: bool,
}

/// Local LLM client
pub struct LocalLLMClient {
    client: Arc<Client>,
    backend: LocalBackend,
    base_url: String,
    default_model: String,
    max_response_size: usize,
}

impl LocalLLMClient {
    /// Create a new local LLM client with shared connection pool
    ///
    /// # Arguments
    /// * `backend` - The local LLM backend to use
    /// * `base_url` - Optional base URL (defaults to standard ports)
    /// * `default_model` - Optional default model name
    ///
    /// # Errors
    /// Returns an error if the base_url is invalid
    pub fn new(
        backend: LocalBackend,
        base_url: Option<String>,
        default_model: Option<String>,
    ) -> Result<Self> {
        let base_url = base_url.unwrap_or_else(|| match backend {
            LocalBackend::Ollama => "http://localhost:11434".to_string(),
            LocalBackend::LlamaCpp => "http://localhost:8080".to_string(),
            LocalBackend::LocalAI => "http://localhost:8080".to_string(),
            LocalBackend::TextGenWebUI => "http://localhost:5000".to_string(),
        });

        // Validate URL format
        Self::validate_url(&base_url)?;

        let default_model = default_model.unwrap_or_else(|| match backend {
            LocalBackend::Ollama => "llama2".to_string(),
            LocalBackend::LlamaCpp => "llama-2-7b".to_string(),
            LocalBackend::LocalAI => "ggml-gpt4all-j".to_string(),
            LocalBackend::TextGenWebUI => "llama-2-7b".to_string(),
        });

        // Validate model name (basic sanitization)
        Self::validate_model_name(&default_model)?;

        Ok(Self {
            client: get_http_client(),
            backend,
            base_url,
            default_model,
            max_response_size: 10 * 1024 * 1024, // 10MB default limit
        })
    }

    /// Validate URL format
    ///
    /// Uses the core ValidationRules for URL parsing, with additional
    /// checks specific to local LLM endpoints.
    pub fn validate_url(url: &str) -> Result<()> {
        if url.is_empty() {
            return Err(ZoeyError::config("Base URL cannot be empty"));
        }

        // Check length before parsing
        if url.len() > 2048 {
            return Err(ZoeyError::config("URL is too long (max 2048 characters)"));
        }

        // Use core validation for URL format
        ValidationRules::validate_url(url)?;

        // Additional check: local LLM endpoints should typically use http://
        // (though https:// is also valid for secure local deployments)
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ZoeyError::config(format!(
                "Invalid URL format: '{}'. Must start with http:// or https://",
                url
            )));
        }

        Ok(())
    }

    /// Validate model name (basic sanitization)
    pub fn validate_model_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(ZoeyError::config("Model name cannot be empty"));
        }

        if name.len() > 256 {
            return Err(ZoeyError::config(
                "Model name is too long (max 256 characters)",
            ));
        }

        // Check for potentially dangerous characters
        if name.contains('\0') || name.contains('\n') || name.contains('\r') {
            return Err(ZoeyError::config("Model name contains invalid characters"));
        }

        Ok(())
    }

    /// Set maximum response size in bytes
    pub fn with_max_response_size(mut self, size: usize) -> Self {
        self.max_response_size = size;
        self
    }

    /// Generate text using local model
    pub async fn generate(&self, params: GenerateTextParams) -> Result<String> {
        match self.backend {
            LocalBackend::Ollama => self.generate_ollama(params).await,
            LocalBackend::LlamaCpp => self.generate_llama_cpp(params).await,
            LocalBackend::LocalAI => self.generate_local_ai(params).await,
            LocalBackend::TextGenWebUI => self.generate_text_gen_webui(params).await,
        }
    }

    /// Generate using Ollama
    async fn generate_ollama(&self, params: GenerateTextParams) -> Result<String> {
        // Validate and sanitize model name
        let model = params
            .model
            .as_ref()
            .map(|m| {
                Self::validate_model_name(m)?;
                Ok::<_, ZoeyError>(m.clone())
            })
            .transpose()?
            .unwrap_or_else(|| self.default_model.clone());

        // Validate prompt
        if params.prompt.is_empty() {
            return Err(ZoeyError::validation("Prompt cannot be empty"));
        }

        if params.prompt.len() > 1_000_000 {
            return Err(ZoeyError::validation("Prompt is too long (max 1MB)"));
        }

        // Validate temperature
        if let Some(temp) = params.temperature {
            if temp < 0.0 || temp > 2.0 {
                return Err(ZoeyError::validation(format!(
                    "Temperature must be between 0.0 and 2.0, got {}",
                    temp
                )));
            }
        }

        let request = OllamaRequest {
            model,
            prompt: params.prompt,
            stream: true,
            options: Some(OllamaOptions {
                temperature: params.temperature,
                num_predict: params.max_tokens,
            }),
        };

        let url = format!("{}/api/generate", self.base_url);
        let mut resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                ZoeyError::model(format!(
                    "Ollama API request failed: {}. Check if Ollama is running at {}",
                    e, self.base_url
                ))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let error_text = resp
                .text()
                .await
                .unwrap_or_else(|e| format!("Failed to read error response: {}", e));
            return Err(ZoeyError::model(format!(
                "Ollama API returned error status {}: {}",
                status, error_text
            )));
        }

        let mut assembled = String::new();
        let mut buffer = String::new();
        while let Ok(opt) = resp.chunk().await {
            let chunk = match opt {
                Some(c) => c,
                None => break,
            };
            let s = String::from_utf8_lossy(&chunk);
            buffer.push_str(&s);
            // Parse newline-delimited JSON objects
            let mut parts: Vec<&str> = buffer.split('\n').collect();
            let tail = parts.pop().unwrap_or("");
            for line in parts {
                let l = line.trim();
                if l.is_empty() {
                    continue;
                }
                match serde_json::from_str::<OllamaResponse>(l) {
                    Ok(obj) => {
                        if !obj.response.is_empty() {
                            assembled.push_str(&obj.response);
                        }
                        // obj.done can be used to break, but continue to drain
                    }
                    Err(_) => {}
                }
            }
            buffer = tail.to_string();
            if assembled.len() > self.max_response_size {
                return Err(ZoeyError::model(format!(
                    "Response text too large: {} bytes (max {} bytes)",
                    assembled.len(),
                    self.max_response_size
                )));
            }
        }

        Ok(assembled)
    }

    /// Generate using llama.cpp HTTP server
    async fn generate_llama_cpp(&self, params: GenerateTextParams) -> Result<String> {
        // Validate prompt
        if params.prompt.is_empty() {
            return Err(ZoeyError::validation("Prompt cannot be empty"));
        }

        // Validate parameters
        let max_tokens = params.max_tokens.unwrap_or(512);
        if max_tokens == 0 || max_tokens > 32768 {
            return Err(ZoeyError::validation(format!(
                "max_tokens must be between 1 and 32768, got {}",
                max_tokens
            )));
        }

        let temperature = params.temperature.unwrap_or(0.7);
        if temperature < 0.0 || temperature > 2.0 {
            return Err(ZoeyError::validation(format!(
                "Temperature must be between 0.0 and 2.0, got {}",
                temperature
            )));
        }

        let request = serde_json::json!({
            "prompt": params.prompt,
            "n_predict": max_tokens,
            "temperature": temperature,
            "stop": params.stop.unwrap_or_default(),
        });

        let url = format!("{}/completion", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                ZoeyError::model(format!(
                    "llama.cpp API request failed: {}. Check if server is running at {}",
                    e, self.base_url
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("Failed to read error response: {}", e));
            return Err(ZoeyError::model(format!(
                "llama.cpp API returned error status {}: {}",
                status, error_text
            )));
        }

        // Check response size
        let content_length = response.content_length().unwrap_or(0) as usize;
        if content_length > self.max_response_size {
            return Err(ZoeyError::model(format!(
                "Response too large: {} bytes (max {} bytes)",
                content_length, self.max_response_size
            )));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| {
            ZoeyError::model(format!(
                "Failed to parse llama.cpp response: {}. Response may be malformed.",
                e
            ))
        })?;

        // Safe JSON parsing with proper error handling
        let content = json.get("content").ok_or_else(|| {
            ZoeyError::model(format!(
                "Invalid llama.cpp response: missing 'content' field. Response: {}",
                serde_json::to_string(&json).unwrap_or_else(|_| "invalid JSON".to_string())
            ))
        })?;

        let text = content.as_str().ok_or_else(|| {
            ZoeyError::model(format!(
                "Invalid llama.cpp response: 'content' field is not a string. Got: {:?}",
                content
            ))
        })?;

        // Validate response size
        if text.len() > self.max_response_size {
            return Err(ZoeyError::model(format!(
                "Response text too large: {} bytes (max {} bytes)",
                text.len(),
                self.max_response_size
            )));
        }

        Ok(text.to_string())
    }

    /// Generate using LocalAI
    async fn generate_local_ai(&self, params: GenerateTextParams) -> Result<String> {
        // Validate and sanitize model name
        let model = params
            .model
            .as_ref()
            .map(|m| {
                Self::validate_model_name(m)?;
                Ok::<_, ZoeyError>(m.clone())
            })
            .transpose()?
            .unwrap_or_else(|| self.default_model.clone());

        // Validate prompt
        if params.prompt.is_empty() {
            return Err(ZoeyError::validation("Prompt cannot be empty"));
        }

        // Validate parameters
        let max_tokens = params.max_tokens.unwrap_or(512);
        if max_tokens == 0 || max_tokens > 32768 {
            return Err(ZoeyError::validation(format!(
                "max_tokens must be between 1 and 32768, got {}",
                max_tokens
            )));
        }

        let temperature = params.temperature.unwrap_or(0.7);
        if temperature < 0.0 || temperature > 2.0 {
            return Err(ZoeyError::validation(format!(
                "Temperature must be between 0.0 and 2.0, got {}",
                temperature
            )));
        }

        // LocalAI uses OpenAI-compatible API
        let request = serde_json::json!({
            "model": model,
            "prompt": params.prompt,
            "max_tokens": max_tokens,
            "temperature": temperature,
        });

        let url = format!("{}/v1/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                ZoeyError::model(format!(
                    "LocalAI API request failed: {}. Check if LocalAI is running at {}",
                    e, self.base_url
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("Failed to read error response: {}", e));
            return Err(ZoeyError::model(format!(
                "LocalAI API returned error status {}: {}",
                status, error_text
            )));
        }

        // Check response size
        let content_length = response.content_length().unwrap_or(0) as usize;
        if content_length > self.max_response_size {
            return Err(ZoeyError::model(format!(
                "Response too large: {} bytes (max {} bytes)",
                content_length, self.max_response_size
            )));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| {
            ZoeyError::model(format!(
                "Failed to parse LocalAI response: {}. Response may be malformed.",
                e
            ))
        })?;

        // Safe JSON parsing with proper error handling
        let choices = json
            .get("choices")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ZoeyError::model(format!(
                    "Invalid LocalAI response: missing or invalid 'choices' array. Response: {}",
                    serde_json::to_string(&json).unwrap_or_else(|_| "invalid JSON".to_string())
                ))
            })?;

        if choices.is_empty() {
            return Err(ZoeyError::model(
                "Invalid LocalAI response: 'choices' array is empty",
            ));
        }

        let first_choice = choices.get(0).ok_or_else(|| {
            ZoeyError::model("Invalid LocalAI response: cannot access first choice")
        })?;

        let text = first_choice.get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ZoeyError::model(format!(
                    "Invalid LocalAI response: 'text' field missing or not a string in first choice. Choice: {:?}",
                    first_choice
                ))
            })?;

        // Validate response size
        if text.len() > self.max_response_size {
            return Err(ZoeyError::model(format!(
                "Response text too large: {} bytes (max {} bytes)",
                text.len(),
                self.max_response_size
            )));
        }

        Ok(text.to_string())
    }

    /// Generate using Text generation web UI
    async fn generate_text_gen_webui(&self, params: GenerateTextParams) -> Result<String> {
        // Validate prompt
        if params.prompt.is_empty() {
            return Err(ZoeyError::validation("Prompt cannot be empty"));
        }

        // Validate parameters
        let max_tokens = params.max_tokens.unwrap_or(512);
        if max_tokens == 0 || max_tokens > 32768 {
            return Err(ZoeyError::validation(format!(
                "max_tokens must be between 1 and 32768, got {}",
                max_tokens
            )));
        }

        let temperature = params.temperature.unwrap_or(0.7);
        if temperature < 0.0 || temperature > 2.0 {
            return Err(ZoeyError::validation(format!(
                "Temperature must be between 0.0 and 2.0, got {}",
                temperature
            )));
        }

        let request = serde_json::json!({
            "prompt": params.prompt,
            "max_new_tokens": max_tokens,
            "temperature": temperature,
        });

        let url = format!("{}/api/v1/generate", self.base_url);
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                ZoeyError::model(format!(
                    "Text generation web UI API request failed: {}. Check if server is running at {}",
                    e, self.base_url
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("Failed to read error response: {}", e));
            return Err(ZoeyError::model(format!(
                "Text generation web UI API returned error status {}: {}",
                status, error_text
            )));
        }

        // Check response size
        let content_length = response.content_length().unwrap_or(0) as usize;
        if content_length > self.max_response_size {
            return Err(ZoeyError::model(format!(
                "Response too large: {} bytes (max {} bytes)",
                content_length, self.max_response_size
            )));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| {
            ZoeyError::model(format!(
                "Failed to parse Text generation web UI response: {}. Response may be malformed.",
                e
            ))
        })?;

        // Safe JSON parsing with proper error handling
        let results = json.get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ZoeyError::model(format!(
                    "Invalid Text generation web UI response: missing or invalid 'results' array. Response: {}",
                    serde_json::to_string(&json).unwrap_or_else(|_| "invalid JSON".to_string())
                ))
            })?;

        if results.is_empty() {
            return Err(ZoeyError::model(
                "Invalid Text generation web UI response: 'results' array is empty",
            ));
        }

        let first_result = results.get(0).ok_or_else(|| {
            ZoeyError::model("Invalid Text generation web UI response: cannot access first result")
        })?;

        let text = first_result.get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ZoeyError::model(format!(
                    "Invalid Text generation web UI response: 'text' field missing or not a string in first result. Result: {:?}",
                    first_result
                ))
            })?;

        // Validate response size
        if text.len() > self.max_response_size {
            return Err(ZoeyError::model(format!(
                "Response text too large: {} bytes (max {} bytes)",
                text.len(),
                self.max_response_size
            )));
        }

        Ok(text.to_string())
    }
}

/// Local LLM plugin
pub struct LocalLLMPlugin {
    backend: LocalBackend,
    base_url: Option<String>,
    default_model: Option<String>,
}

impl LocalLLMPlugin {
    /// Create a new local LLM plugin with Ollama backend
    pub fn new() -> Self {
        Self {
            backend: LocalBackend::Ollama,
            base_url: None,
            default_model: None,
        }
    }

    /// Create with specific backend
    pub fn with_backend(backend: LocalBackend) -> Self {
        Self {
            backend,
            base_url: None,
            default_model: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(backend: LocalBackend, base_url: String, default_model: String) -> Self {
        Self {
            backend,
            base_url: Some(base_url),
            default_model: Some(default_model),
        }
    }
}

impl Default for LocalLLMPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for LocalLLMPlugin {
    fn name(&self) -> &str {
        "local-llm"
    }

    fn description(&self) -> &str {
        "Local LLM integration for privacy-first, government-compliant AI inference"
    }

    fn priority(&self) -> i32 {
        200 // Higher priority than cloud providers for government use
    }

    async fn init(
        &self,
        config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        tracing::info!(
            "Local LLM plugin initialized with backend: {:?}",
            self.backend
        );
        tracing::info!("Using local inference - no data sent to external APIs");
        tracing::info!("HIPAA compliant - all processing on-premises");

        if let Some(url) = config.get("base_url") {
            tracing::info!("Local LLM endpoint: {}", url);
        }

        Ok(())
    }

    fn models(&self) -> HashMap<String, ModelHandler> {
        let backend = self.backend;
        let configured_base_url = self.base_url.clone();
        let configured_model = self.default_model.clone();

        let handler: ModelHandler = Arc::new(move |params: ModelHandlerParams| {
            let backend = backend;
            let configured_base_url = configured_base_url.clone();
            let configured_model = configured_model.clone();

            Box::pin(async move {
                let mut gen_params = params.params;
                let start_time = std::time::Instant::now();

                // Use model from params if specified, otherwise use configured default
                // This allows character XML to override plugin defaults
                if gen_params.model.is_none() {
                    gen_params.model = configured_model;
                }

                let base_url = configured_base_url;
                let client = LocalLLMClient::new(backend, base_url, None).map_err(|e| {
                    ZoeyError::plugin(format!("Failed to create local LLM client: {}", e))
                })?;
                let text = client.generate(gen_params.clone()).await?;

                if let Some(runtime) = params
                    .runtime
                    .downcast_ref::<zoey_core::runtime::AgentRuntime>()
                {
                    let (cost_tracker, security_monitor) = {
                        let obs_lock = runtime.observability.read().unwrap();
                        (
                            obs_lock.as_ref().and_then(|obs| obs.cost_tracker.clone()),
                            obs_lock
                                .as_ref()
                                .and_then(|obs| obs.security_monitor.clone()),
                        )
                    };

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
                    if let Some(ref monitor) = security_monitor {
                        let _ = monitor
                            .check_pii_violation(runtime.agent_id, None, &text, "completion")
                            .await;
                    }

                    if let Some(cost_tracker) = cost_tracker {
                        let latency_ms = start_time.elapsed().as_millis() as u64;
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
                        let provider_name = match backend {
                            LocalBackend::Ollama => "ollama",
                            LocalBackend::LlamaCpp => "llama.cpp",
                            LocalBackend::LocalAI => "localai",
                            LocalBackend::TextGenWebUI => "textgenwebui",
                        };
                        let model_name = gen_params
                            .model
                            .clone()
                            .unwrap_or_else(|| "local".to_string());
                        fn estimate_tokens(s: &str) -> usize {
                            let chars = s.chars().count();
                            ((chars as f64) / 4.0).ceil() as usize
                        }
                        let prompt_tokens = estimate_tokens(&gen_params.prompt);
                        let completion_tokens = estimate_tokens(&text);
                        let _ = cost_tracker
                            .record_llm_call(
                                provider_name,
                                &model_name,
                                prompt_tokens,
                                completion_tokens,
                                latency_ms,
                                runtime.agent_id,
                                context,
                            )
                            .await
                            .map_err(|e| {
                                tracing::warn!("Failed to record LLM cost: {}", e);
                                e
                            });
                    }
                }

                Ok(text)
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>
        });

        let mut models = HashMap::new();
        // Register for all text models with high priority
        models.insert("TEXT_SMALL".to_string(), handler.clone());
        models.insert("TEXT_MEDIUM".to_string(), handler.clone());
        models.insert("TEXT_LARGE".to_string(), handler);

        models
    }
}

#[async_trait]
impl Provider for LocalLLMPlugin {
    fn name(&self) -> &str {
        "local_llm"
    }
    fn capabilities(&self) -> Option<Vec<String>> {
        Some(vec!["CHAT".to_string()])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_llm_plugin() {
        let plugin = LocalLLMPlugin::new();
        assert_eq!(zoey_core::Plugin::name(&plugin), "local-llm");
        assert_eq!(plugin.priority(), 200);
    }

    #[test]
    fn test_local_llm_with_backend() {
        let plugin = LocalLLMPlugin::with_backend(LocalBackend::LlamaCpp);
        let models = plugin.models();

        assert!(models.contains_key("TEXT_SMALL"));
        assert!(models.contains_key("TEXT_MEDIUM"));
        assert!(models.contains_key("TEXT_LARGE"));
    }

    #[test]
    fn test_local_llm_backends() {
        for backend in [
            LocalBackend::Ollama,
            LocalBackend::LlamaCpp,
            LocalBackend::LocalAI,
            LocalBackend::TextGenWebUI,
        ] {
            let client = LocalLLMClient::new(backend, None, None).expect("Should create client");
            assert!(!client.base_url.is_empty());
            assert!(!client.default_model.is_empty());
        }
    }

    #[test]
    fn test_url_validation() {
        // Valid URLs
        assert!(LocalLLMClient::validate_url("http://localhost:11434").is_ok());
        assert!(LocalLLMClient::validate_url("https://example.com").is_ok());

        // Invalid URLs
        assert!(LocalLLMClient::validate_url("").is_err());
        assert!(LocalLLMClient::validate_url("not-a-url").is_err());
        assert!(LocalLLMClient::validate_url("ftp://example.com").is_err());
    }

    #[test]
    fn test_model_name_validation() {
        // Valid model names
        assert!(LocalLLMClient::validate_model_name("llama2").is_ok());
        assert!(LocalLLMClient::validate_model_name("gpt-4").is_ok());

        // Invalid model names
        assert!(LocalLLMClient::validate_model_name("").is_err());
        assert!(LocalLLMClient::validate_model_name(&"a".repeat(257)).is_err());
        assert!(LocalLLMClient::validate_model_name("model\nname").is_err());
    }
}
