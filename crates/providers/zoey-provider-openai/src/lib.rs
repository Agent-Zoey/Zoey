//! OpenAI integration plugin for ZoeyOS

#![warn(missing_docs)]
#![warn(clippy::all)]

use async_openai::{
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs, CreateEmbeddingRequestArgs,
    },
    Client,
};
use async_trait::async_trait;
use zoey_core::{types::*, ZoeyError, Result};
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

struct SettingRow {
    name: String,
    value: String,
    source: String,
    change: String,
}
fn pad(s: &str, w: usize) -> String {
    let mut out = s.to_string();
    if out.len() > w {
        out.truncate(w);
    }
    let pad_len = if w > out.len() { w - out.len() } else { 0 };
    out + &" ".repeat(pad_len)
}
fn render(plugin: &str, color: &str, deco: &str, rows: Vec<SettingRow>) {
    let reset = "\x1b[0m";
    let top = format!("{}+{}+{}", color, "-".repeat(78), reset);
    let title = format!(" {plugin} settings ");
    let d1 = pad(&format!("{}{}{}", deco, deco, deco), 25);
    let d2 = pad(&format!("{}{}{}", deco, deco, deco), 25);
    let line1 = format!("{}|{}|{}", color, pad(&(d1 + &title + &d2), 78), reset);
    let line2 = format!(
        "{}|{}|{}",
        color,
        pad(&format!("{} change via KEY=VALUE {}", deco, deco), 78),
        reset
    );
    let sep = format!("{}+{}+{}", color, "=".repeat(78), reset);
    let header = format!(
        "{}|{}|{}|{}|{}|{}",
        color,
        pad("Setting", 24),
        pad("Value", 20),
        pad("Source", 10),
        pad("Change", 24),
        reset
    );
    let mid = format!("{}+{}+{}", color, "-".repeat(78), reset);
    println!("{}", top);
    println!("{}", line1);
    println!("{}", line2);
    println!("{}", sep);
    println!("{}", header);
    println!("{}", mid);
    if rows.is_empty() {
        let row = format!(
            "{}|{}|{}|{}|{}|{}",
            color,
            pad("<none>", 24),
            pad("-", 20),
            pad("-", 10),
            pad("Use runtime settings", 24),
            reset
        );
        println!("{}", row);
    } else {
        for r in rows {
            let row = format!(
                "{}|{}|{}|{}|{}|{}",
                color,
                pad(&r.name, 24),
                pad(&r.value, 20),
                pad(&r.source, 10),
                pad(&r.change, 24),
                reset
            );
            println!("{}", row);
        }
    }
    let bottom = format!("{}+{}+{}", color, "-".repeat(78), reset);
    println!("{}", bottom);
}

/// Shared OpenAI client instance for connection pooling
static CLIENT: OnceLock<Arc<Client<async_openai::config::OpenAIConfig>>> = OnceLock::new();

/// OpenAI plugin
pub struct OpenAIPlugin;

impl OpenAIPlugin {
    /// Create a new OpenAI plugin
    pub fn new() -> Self {
        Self
    }

    /// Get or initialize the shared OpenAI client
    fn get_client() -> Arc<Client<async_openai::config::OpenAIConfig>> {
        CLIENT
            .get_or_init(|| {
                tracing::debug!("Initializing shared OpenAI client");
                Arc::new(Client::new())
            })
            .clone()
    }
}

impl Default for OpenAIPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for OpenAIPlugin {
    fn name(&self) -> &str {
        "openai"
    }

    fn description(&self) -> &str {
        "OpenAI LLM integration (GPT-3.5, GPT-4, embeddings)"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        let mut rows: Vec<SettingRow> = Vec::new();
        rows.push(SettingRow {
            name: "OPENAI_API_KEY".to_string(),
            value: if std::env::var("OPENAI_API_KEY").is_ok() {
                "set".to_string()
            } else {
                "<not set>".to_string()
            },
            source: "env".to_string(),
            change: "export OPENAI_API_KEY=...".to_string(),
        });
        rows.push(SettingRow {
            name: "OPENAI_MODEL".to_string(),
            value: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4".to_string()),
            source: if std::env::var("OPENAI_MODEL").is_ok() {
                "env".to_string()
            } else {
                "default".to_string()
            },
            change: "export OPENAI_MODEL=name".to_string(),
        });
        render("openai", "\x1b[34m", "=", rows);
        Ok(())
    }

    fn models(&self) -> HashMap<String, ModelHandler> {
        let mut models = HashMap::new();

        // Text generation models
        let text_handler = create_text_handler();
        models.insert("TEXT_SMALL".to_string(), text_handler.clone());
        models.insert("TEXT_MEDIUM".to_string(), text_handler.clone());
        models.insert("TEXT_LARGE".to_string(), text_handler);

        // Embedding model
        models.insert("TEXT_EMBEDDING".to_string(), create_embedding_handler());

        models
    }
}

#[async_trait]
impl zoey_core::types::Provider for OpenAIPlugin {
    fn name(&self) -> &str {
        "openai"
    }
    fn capabilities(&self) -> Option<Vec<String>> {
        Some(vec!["CHAT".to_string(), "EMBEDDINGS".to_string()])
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

/// Create text generation handler
fn create_text_handler() -> ModelHandler {
    Arc::new(move |params: ModelHandlerParams| {
        Box::pin(async move {
            let gen_params = params.params;
            let start_time = std::time::Instant::now();

            // Build client with explicit secrets when available
            let client: Arc<Client<async_openai::config::OpenAIConfig>> = if let Some(runtime) =
                params
                    .runtime
                    .downcast_ref::<zoey_core::runtime::AgentRuntime>()
            {
                let api_key = zoey_core::secrets::get_secret(&runtime.character, "OPENAI_API_KEY");
                if let Some(key) = api_key {
                    Arc::new(Client::with_config(
                        async_openai::config::OpenAIConfig::new().with_api_key(key),
                    ))
                } else {
                    OpenAIPlugin::get_client()
                }
            } else {
                OpenAIPlugin::get_client()
            };

            // Determine model based on type
            let model = gen_params.model.unwrap_or_else(|| "gpt-4".to_string());

            let mut request_builder = CreateChatCompletionRequestArgs::default();
            request_builder.model(model.clone());
            request_builder.messages(vec![ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(gen_params.prompt.clone())
                    .build()
                    .map_err(|e| ZoeyError::model(e.to_string()))?,
            )]);

            if let Some(temp) = gen_params.temperature {
                request_builder.temperature(temp);
            }

            if let Some(max_tokens) = gen_params.max_tokens {
                request_builder.max_tokens(max_tokens as u32);
            }

            // Enable streaming and accumulate chunks for reduced provider-side latency
            request_builder.stream(true);
            let request = request_builder
                .build()
                .map_err(|e| ZoeyError::model(e.to_string()))?;

            let mut stream = client
                .chat()
                .create_stream(request)
                .await
                .map_err(|e| ZoeyError::model(e.to_string()))?;

            let mut text = String::new();
            let mut latency_ms: u64 = 0;
            let mut ttft_recorded = false;
            let mut last_headers: Option<HeaderMap> = None;
            while let Some(chunk) = stream.next().await {
                let resp = chunk.map_err(|e| ZoeyError::model(e.to_string()))?;
                if let Some(content) = resp
                    .choices
                    .first()
                    .and_then(|c| c.delta.content.as_deref())
                {
                    if !ttft_recorded {
                        latency_ms = start_time.elapsed().as_millis() as u64;
                        ttft_recorded = true;
                    }
                    text.push_str(content);
                }
                // SDK does not expose headers here; placeholder
            }
            if !ttft_recorded {
                latency_ms = start_time.elapsed().as_millis() as u64;
            }

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
                        .check_pii_violation(runtime.agent_id, None, &gen_params.prompt, "prompt")
                        .await;
                }
                if let Some(ref monitor) = security_monitor {
                    let _ = monitor
                        .check_pii_violation(runtime.agent_id, None, &text, "completion")
                        .await;
                }

                if let Some(cost_tracker) = cost_tracker {
                    fn estimate_tokens(s: &str) -> usize {
                        let chars = s.chars().count();
                        ((chars as f64) / 4.0).ceil() as usize
                    }
                    let prompt_tokens = estimate_tokens(&gen_params.prompt);
                    let completion_tokens = estimate_tokens(&text);
                    let context = zoey_core::observability::LLMCallContext {
                        agent_id: runtime.agent_id,
                        user_id: None,
                        conversation_id: None,
                        action_name: None,
                        evaluator_name: None,
                        temperature: gen_params.temperature,
                        cached_tokens: None,
                        ttft_ms: Some(latency_ms),
                        prompt_hash: Some(zoey_core::observability::compute_prompt_hash(
                            &gen_params.prompt,
                        )),
                        prompt_preview: Some(zoey_core::observability::compute_prompt_preview(
                            &gen_params.prompt,
                        )),
                    };
                    let _ = cost_tracker
                        .record_llm_call(
                            "openai",
                            &model,
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

                // Capture rate-limit (placeholder since SDK hides headers)
                if let Some(obs) = runtime.observability.read().unwrap().clone() {
                    obs.set_rate_limit(
                        "openai",
                        zoey_core::observability::types::ProviderRateLimit::default(),
                    );
                }
            }

            Ok(text)
        })
    })
}

/// Create embedding generation handler
fn create_embedding_handler() -> ModelHandler {
    Arc::new(move |params: ModelHandlerParams| {
        Box::pin(async move {
            let gen_params = params.params;

            // Use shared client for connection pooling
            let client = OpenAIPlugin::get_client();

            let request = CreateEmbeddingRequestArgs::default()
                .model("text-embedding-ada-002")
                .input(&gen_params.prompt)
                .build()
                .map_err(|e| ZoeyError::model(e.to_string()))?;

            let response = client
                .embeddings()
                .create(request)
                .await
                .map_err(|e| ZoeyError::model(e.to_string()))?;

            // Return embedding as JSON string
            let embedding = response
                .data
                .first()
                .map(|e| &e.embedding)
                .ok_or_else(|| ZoeyError::model("No embedding returned"))?;

            Ok(serde_json::to_string(embedding)?)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_plugin_creation() {
        let plugin = OpenAIPlugin::new();
        assert_eq!(zoey_core::Plugin::name(&plugin), "openai");
    }

    #[test]
    fn test_openai_plugin_models() {
        let plugin = OpenAIPlugin::new();
        let models = plugin.models();

        assert!(models.contains_key("TEXT_SMALL"));
        assert!(models.contains_key("TEXT_MEDIUM"));
        assert!(models.contains_key("TEXT_LARGE"));
        assert!(models.contains_key("TEXT_EMBEDDING"));
    }
}
