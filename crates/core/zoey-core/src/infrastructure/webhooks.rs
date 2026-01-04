//! Webhook integration system
//!
//! Provides outbound webhook support for:
//! - Event notifications
//! - Retry with exponential backoff
//! - Signature verification
//! - Delivery tracking

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Webhook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    /// Message received
    MessageReceived,
    /// Message sent
    MessageSent,
    /// Agent started
    AgentStarted,
    /// Agent stopped
    AgentStopped,
    /// Error occurred
    Error,
    /// Action executed
    ActionExecuted,
    /// Memory created
    MemoryCreated,
    /// Custom event
    Custom,
}

/// Webhook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Unique identifier
    pub id: String,
    /// Target URL
    pub url: String,
    /// Events to subscribe to
    pub events: Vec<WebhookEventType>,
    /// Secret for signature verification
    #[serde(skip_serializing)]
    pub secret: Option<String>,
    /// Custom headers
    pub headers: HashMap<String, String>,
    /// Retry policy
    pub retry_policy: RetryPolicy,
    /// Whether the webhook is enabled
    pub enabled: bool,
    /// Description
    pub description: Option<String>,
}

impl WebhookConfig {
    /// Create a new webhook configuration
    pub fn new(id: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            events: vec![],
            secret: None,
            headers: HashMap::new(),
            retry_policy: RetryPolicy::default(),
            enabled: true,
            description: None,
        }
    }

    /// Subscribe to specific events
    pub fn with_events(mut self, events: Vec<WebhookEventType>) -> Self {
        self.events = events;
        self
    }

    /// Set secret for HMAC signature
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        self.secret = Some(secret.into());
        self
    }

    /// Add a custom header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

/// Retry policy for webhook delivery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay between retries
    pub initial_delay_ms: u64,
    /// Maximum delay between retries
    pub max_delay_ms: u64,
    /// Backoff multiplier
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
        }
    }
}

/// Webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    /// Event type
    pub event: WebhookEventType,
    /// Event ID
    pub event_id: String,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Agent ID
    pub agent_id: Option<String>,
    /// Event data
    pub data: serde_json::Value,
}

impl WebhookPayload {
    /// Create a new webhook payload
    pub fn new(event: WebhookEventType, data: serde_json::Value) -> Self {
        Self {
            event,
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            agent_id: None,
            data,
        }
    }

    /// Set agent ID
    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }
}

/// Webhook delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    /// Delivery pending
    Pending,
    /// Delivery in progress
    InProgress,
    /// Delivery successful
    Delivered,
    /// Delivery failed after all retries
    Failed,
}

/// Webhook delivery record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    /// Delivery ID
    pub id: String,
    /// Webhook ID
    pub webhook_id: String,
    /// Payload
    pub payload: WebhookPayload,
    /// Status
    pub status: DeliveryStatus,
    /// Number of attempts
    pub attempts: u32,
    /// Last attempt timestamp
    pub last_attempt: Option<String>,
    /// Last error message
    pub last_error: Option<String>,
    /// Response status code
    pub response_code: Option<u16>,
    /// Created at
    pub created_at: String,
}

/// Webhook manager for handling webhook registrations and deliveries
pub struct WebhookManager {
    /// Registered webhooks
    webhooks: Arc<RwLock<HashMap<String, WebhookConfig>>>,
    /// Delivery queue sender
    delivery_tx: mpsc::Sender<WebhookDelivery>,
    /// Delivery history
    history: Arc<RwLock<Vec<WebhookDelivery>>>,
    /// HTTP client
    client: reqwest::Client,
    /// Maximum history size
    max_history: usize,
}

impl WebhookManager {
    /// Create a new webhook manager
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<WebhookDelivery>) {
        let (tx, rx) = mpsc::channel(buffer_size);

        let manager = Self {
            webhooks: Arc::new(RwLock::new(HashMap::new())),
            delivery_tx: tx,
            history: Arc::new(RwLock::new(Vec::new())),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            max_history: 1000,
        };

        (manager, rx)
    }

    /// Start the delivery worker
    pub fn start_worker(self: Arc<Self>, mut rx: mpsc::Receiver<WebhookDelivery>) {
        tokio::spawn(async move {
            while let Some(mut delivery) = rx.recv().await {
                self.process_delivery(&mut delivery).await;
            }
        });
    }

    /// Register a webhook
    pub fn register(&self, config: WebhookConfig) {
        info!("Registering webhook: {} -> {}", config.id, config.url);
        self.webhooks
            .write()
            .unwrap()
            .insert(config.id.clone(), config);
    }

    /// Unregister a webhook
    pub fn unregister(&self, webhook_id: &str) {
        info!("Unregistering webhook: {}", webhook_id);
        self.webhooks.write().unwrap().remove(webhook_id);
    }

    /// Get all registered webhooks
    pub fn list_webhooks(&self) -> Vec<WebhookConfig> {
        self.webhooks.read().unwrap().values().cloned().collect()
    }

    /// Trigger an event (sends to all subscribed webhooks)
    pub async fn trigger(&self, payload: WebhookPayload) -> Result<Vec<String>> {
        let webhooks = self.webhooks.read().unwrap();
        let mut delivery_ids = Vec::new();

        for webhook in webhooks.values() {
            if !webhook.enabled {
                continue;
            }

            if webhook.events.is_empty() || webhook.events.contains(&payload.event) {
                let delivery = WebhookDelivery {
                    id: uuid::Uuid::new_v4().to_string(),
                    webhook_id: webhook.id.clone(),
                    payload: payload.clone(),
                    status: DeliveryStatus::Pending,
                    attempts: 0,
                    last_attempt: None,
                    last_error: None,
                    response_code: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                };

                delivery_ids.push(delivery.id.clone());

                if let Err(e) = self.delivery_tx.send(delivery).await {
                    error!("Failed to queue webhook delivery: {}", e);
                }
            }
        }

        Ok(delivery_ids)
    }

    /// Process a delivery with retry logic
    async fn process_delivery(&self, delivery: &mut WebhookDelivery) {
        let webhook = {
            let webhooks = self.webhooks.read().unwrap();
            match webhooks.get(&delivery.webhook_id) {
                Some(w) => w.clone(),
                None => {
                    warn!(
                        "Webhook {} not found, skipping delivery",
                        delivery.webhook_id
                    );
                    return;
                }
            }
        };

        delivery.status = DeliveryStatus::InProgress;
        let retry_policy = &webhook.retry_policy;
        let mut delay = Duration::from_millis(retry_policy.initial_delay_ms);

        for attempt in 0..=retry_policy.max_retries {
            delivery.attempts = attempt + 1;
            delivery.last_attempt = Some(chrono::Utc::now().to_rfc3339());

            debug!(
                "Attempting webhook delivery {} (attempt {})",
                delivery.id, delivery.attempts
            );

            match self.send_webhook(&webhook, &delivery.payload).await {
                Ok(status_code) => {
                    delivery.status = DeliveryStatus::Delivered;
                    delivery.response_code = Some(status_code);
                    delivery.last_error = None;

                    info!(
                        "Webhook delivered successfully: {} -> {} (status: {})",
                        delivery.id, webhook.url, status_code
                    );
                    break;
                }
                Err(e) => {
                    delivery.last_error = Some(e.to_string());

                    if attempt < retry_policy.max_retries {
                        warn!(
                            "Webhook delivery failed (attempt {}): {}. Retrying in {:?}",
                            delivery.attempts, e, delay
                        );
                        tokio::time::sleep(delay).await;

                        // Exponential backoff
                        delay = Duration::from_millis(
                            ((delay.as_millis() as f64) * retry_policy.multiplier)
                                .min(retry_policy.max_delay_ms as f64)
                                as u64,
                        );
                    } else {
                        delivery.status = DeliveryStatus::Failed;
                        error!(
                            "Webhook delivery failed after {} attempts: {}",
                            delivery.attempts, e
                        );
                    }
                }
            }
        }

        // Record in history
        self.record_history(delivery.clone());
    }

    /// Send webhook request
    async fn send_webhook(&self, webhook: &WebhookConfig, payload: &WebhookPayload) -> Result<u16> {
        let body = serde_json::to_string(payload)
            .map_err(|e| ZoeyError::other(format!("Failed to serialize payload: {}", e)))?;

        let mut request = self
            .client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "LauraAI-Webhook/1.0");

        // Add custom headers
        for (key, value) in &webhook.headers {
            request = request.header(key, value);
        }

        // Add signature if secret is configured
        if let Some(secret) = &webhook.secret {
            let signature = self.compute_signature(secret, &body);
            request = request.header("X-Webhook-Signature", signature);
        }

        let response = request
            .body(body)
            .send()
            .await
            .map_err(|e| ZoeyError::other(format!("HTTP request failed: {}", e)))?;

        let status = response.status().as_u16();

        if status >= 200 && status < 300 {
            Ok(status)
        } else {
            Err(ZoeyError::other(format!(
                "Webhook returned status {}",
                status
            )))
        }
    }

    /// Compute HMAC-SHA256 signature
    fn compute_signature(&self, secret: &str, body: &str) -> String {
        use sha2::{Digest, Sha256};

        // Simple HMAC-like signature (for production, use proper HMAC)
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hasher.update(body.as_bytes());
        let result = hasher.finalize();

        format!("sha256={}", hex::encode(result))
    }

    /// Record delivery in history
    fn record_history(&self, delivery: WebhookDelivery) {
        let mut history = self.history.write().unwrap();

        if history.len() >= self.max_history {
            history.remove(0);
        }

        history.push(delivery);
    }

    /// Get delivery history
    pub fn get_history(&self, limit: usize) -> Vec<WebhookDelivery> {
        let history = self.history.read().unwrap();
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get delivery by ID
    pub fn get_delivery(&self, id: &str) -> Option<WebhookDelivery> {
        self.history
            .read()
            .unwrap()
            .iter()
            .find(|d| d.id == id)
            .cloned()
    }
}

/// Helper to trigger common events
pub struct WebhookEvents;

impl WebhookEvents {
    /// Create message received event
    pub fn message_received(
        message_id: &str,
        content: &str,
        sender: &str,
        room_id: &str,
    ) -> WebhookPayload {
        WebhookPayload::new(
            WebhookEventType::MessageReceived,
            serde_json::json!({
                "message_id": message_id,
                "content": content,
                "sender": sender,
                "room_id": room_id,
            }),
        )
    }

    /// Create message sent event
    pub fn message_sent(message_id: &str, content: &str, room_id: &str) -> WebhookPayload {
        WebhookPayload::new(
            WebhookEventType::MessageSent,
            serde_json::json!({
                "message_id": message_id,
                "content": content,
                "room_id": room_id,
            }),
        )
    }

    /// Create agent started event
    pub fn agent_started(agent_id: &str, agent_name: &str) -> WebhookPayload {
        WebhookPayload::new(
            WebhookEventType::AgentStarted,
            serde_json::json!({
                "agent_name": agent_name,
            }),
        )
        .with_agent_id(agent_id)
    }

    /// Create agent stopped event
    pub fn agent_stopped(agent_id: &str, reason: &str) -> WebhookPayload {
        WebhookPayload::new(
            WebhookEventType::AgentStopped,
            serde_json::json!({
                "reason": reason,
            }),
        )
        .with_agent_id(agent_id)
    }

    /// Create error event
    pub fn error(
        error_type: &str,
        message: &str,
        details: Option<serde_json::Value>,
    ) -> WebhookPayload {
        WebhookPayload::new(
            WebhookEventType::Error,
            serde_json::json!({
                "error_type": error_type,
                "message": message,
                "details": details,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_config_builder() {
        let config = WebhookConfig::new("test", "https://example.com/webhook")
            .with_events(vec![WebhookEventType::MessageReceived])
            .with_secret("my_secret")
            .with_header("Authorization", "Bearer token");

        assert_eq!(config.id, "test");
        assert_eq!(config.url, "https://example.com/webhook");
        assert!(config.events.contains(&WebhookEventType::MessageReceived));
        assert_eq!(config.secret, Some("my_secret".to_string()));
    }

    #[test]
    fn test_webhook_payload_creation() {
        let payload = WebhookPayload::new(
            WebhookEventType::MessageReceived,
            serde_json::json!({"test": "data"}),
        )
        .with_agent_id("agent-123");

        assert_eq!(payload.event, WebhookEventType::MessageReceived);
        assert_eq!(payload.agent_id, Some("agent-123".to_string()));
    }

    #[tokio::test]
    async fn test_webhook_manager_registration() {
        let (manager, _rx) = WebhookManager::new(10);

        let config = WebhookConfig::new("test", "https://example.com/webhook");
        manager.register(config);

        let webhooks = manager.list_webhooks();
        assert_eq!(webhooks.len(), 1);
        assert_eq!(webhooks[0].id, "test");
    }
}
