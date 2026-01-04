//! API key rotation support
//!
//! Provides secure key rotation without downtime:
//! - Multiple active keys
//! - Graceful rotation
//! - Key expiration
//! - Audit logging

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{info, warn};

/// Key status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStatus {
    /// Key is active and can be used
    Active,
    /// Key is pending activation
    Pending,
    /// Key is deprecated (still works but should be rotated)
    Deprecated,
    /// Key is revoked and cannot be used
    Revoked,
    /// Key has expired
    Expired,
}

/// API key metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyMetadata {
    /// Key ID (not the actual key)
    pub key_id: String,
    /// Key status
    pub status: KeyStatus,
    /// Provider this key is for
    pub provider: String,
    /// Created timestamp
    pub created_at: i64,
    /// Expiration timestamp (optional)
    pub expires_at: Option<i64>,
    /// Last used timestamp
    pub last_used_at: Option<i64>,
    /// Usage count
    pub usage_count: u64,
    /// Description
    pub description: Option<String>,
    /// Key hash (for verification without storing plaintext)
    pub key_hash: String,
}

impl ApiKeyMetadata {
    /// Check if key is usable
    pub fn is_usable(&self) -> bool {
        matches!(self.status, KeyStatus::Active | KeyStatus::Deprecated)
    }

    /// Check if key is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            chrono::Utc::now().timestamp() > expires_at
        } else {
            false
        }
    }
}

/// API key with secret
#[derive(Clone)]
pub struct ApiKey {
    /// Metadata
    pub metadata: ApiKeyMetadata,
    /// The actual key (never logged or serialized)
    secret: String,
}

impl ApiKey {
    /// Create a new API key
    pub fn new(provider: &str, secret: String, description: Option<String>) -> Self {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_hash = hash_key(&secret);

        Self {
            metadata: ApiKeyMetadata {
                key_id,
                status: KeyStatus::Pending,
                provider: provider.to_string(),
                created_at: chrono::Utc::now().timestamp(),
                expires_at: None,
                last_used_at: None,
                usage_count: 0,
                description,
                key_hash,
            },
            secret,
        }
    }

    /// Get the secret key
    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// Set expiration
    pub fn with_expiration(mut self, expires_at: i64) -> Self {
        self.metadata.expires_at = Some(expires_at);
        self
    }

    /// Activate the key
    pub fn activate(&mut self) {
        self.metadata.status = KeyStatus::Active;
    }

    /// Mark as deprecated
    pub fn deprecate(&mut self) {
        self.metadata.status = KeyStatus::Deprecated;
    }

    /// Revoke the key
    pub fn revoke(&mut self) {
        self.metadata.status = KeyStatus::Revoked;
    }

    /// Record usage
    pub fn record_usage(&mut self) {
        self.metadata.last_used_at = Some(chrono::Utc::now().timestamp());
        self.metadata.usage_count += 1;
    }
}

/// Hash a key for storage (one-way)
fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Key rotation event for audit logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationEvent {
    /// Event ID
    pub event_id: String,
    /// Event type
    pub event_type: KeyRotationEventType,
    /// Provider
    pub provider: String,
    /// Old key ID (if applicable)
    pub old_key_id: Option<String>,
    /// New key ID (if applicable)
    pub new_key_id: Option<String>,
    /// Timestamp
    pub timestamp: i64,
    /// Actor (who performed the action)
    pub actor: Option<String>,
    /// Additional details
    pub details: Option<String>,
}

/// Key rotation event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyRotationEventType {
    /// New key added
    KeyAdded,
    /// Key activated
    KeyActivated,
    /// Key deprecated
    KeyDeprecated,
    /// Key revoked
    KeyRevoked,
    /// Key expired
    KeyExpired,
    /// Rotation started
    RotationStarted,
    /// Rotation completed
    RotationCompleted,
    /// Key tested
    KeyTested,
}

/// API key manager for handling key rotation
pub struct ApiKeyManager {
    /// Keys by provider
    keys: Arc<RwLock<HashMap<String, Vec<ApiKey>>>>,
    /// Rotation events for audit
    events: Arc<RwLock<Vec<KeyRotationEvent>>>,
    /// Grace period for deprecated keys
    deprecation_grace_period: Duration,
    /// Key testers by provider
    testers: Arc<RwLock<HashMap<String, Box<dyn KeyTester>>>>,
}

impl ApiKeyManager {
    /// Create a new key manager
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            deprecation_grace_period: Duration::from_secs(3600), // 1 hour
            testers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set deprecation grace period
    pub fn with_grace_period(mut self, duration: Duration) -> Self {
        self.deprecation_grace_period = duration;
        self
    }

    /// Register a key tester for a provider
    pub fn register_tester<T: KeyTester + 'static>(&self, provider: &str, tester: T) {
        self.testers
            .write()
            .unwrap()
            .insert(provider.to_string(), Box::new(tester));
    }

    /// Add a new key (starts as pending)
    pub fn add_key(&self, key: ApiKey) -> Result<String> {
        let key_id = key.metadata.key_id.clone();
        let provider = key.metadata.provider.clone();

        self.keys
            .write()
            .unwrap()
            .entry(provider.clone())
            .or_insert_with(Vec::new)
            .push(key);

        self.record_event(KeyRotationEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type: KeyRotationEventType::KeyAdded,
            provider,
            old_key_id: None,
            new_key_id: Some(key_id.clone()),
            timestamp: chrono::Utc::now().timestamp(),
            actor: None,
            details: None,
        });

        info!("Added new key {} for provider", key_id);
        Ok(key_id)
    }

    /// Get the current active key for a provider
    pub fn get_active_key(&self, provider: &str) -> Option<ApiKey> {
        let keys = self.keys.read().unwrap();
        keys.get(provider)?
            .iter()
            .find(|k| k.metadata.status == KeyStatus::Active && !k.metadata.is_expired())
            .cloned()
    }

    /// Get any usable key (active or deprecated)
    pub fn get_usable_key(&self, provider: &str) -> Option<ApiKey> {
        let keys = self.keys.read().unwrap();
        keys.get(provider)?
            .iter()
            .find(|k| k.metadata.is_usable() && !k.metadata.is_expired())
            .cloned()
    }

    /// Rotate keys for a provider
    pub async fn rotate_key(&self, provider: &str, new_secret: String) -> Result<String> {
        info!("Starting key rotation for provider: {}", provider);

        // Create new key
        let mut new_key = ApiKey::new(provider, new_secret, Some("Rotated key".to_string()));

        // Test new key if tester is available
        if let Some(tester) = self.testers.read().unwrap().get(provider) {
            info!("Testing new key before activation...");
            tester.test_key(new_key.secret()).await?;

            self.record_event(KeyRotationEvent {
                event_id: uuid::Uuid::new_v4().to_string(),
                event_type: KeyRotationEventType::KeyTested,
                provider: provider.to_string(),
                old_key_id: None,
                new_key_id: Some(new_key.metadata.key_id.clone()),
                timestamp: chrono::Utc::now().timestamp(),
                actor: None,
                details: Some("Key test passed".to_string()),
            });
        }

        // Get current active key ID
        let old_key_id = self
            .get_active_key(provider)
            .map(|k| k.metadata.key_id.clone());

        // Deprecate current active key
        if let Some(ref old_id) = old_key_id {
            self.deprecate_key(provider, old_id)?;
        }

        // Activate new key
        new_key.activate();
        let new_key_id = new_key.metadata.key_id.clone();

        self.keys
            .write()
            .unwrap()
            .entry(provider.to_string())
            .or_insert_with(Vec::new)
            .push(new_key);

        self.record_event(KeyRotationEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type: KeyRotationEventType::RotationCompleted,
            provider: provider.to_string(),
            old_key_id,
            new_key_id: Some(new_key_id.clone()),
            timestamp: chrono::Utc::now().timestamp(),
            actor: None,
            details: None,
        });

        info!("Key rotation completed for provider: {}", provider);
        Ok(new_key_id)
    }

    /// Deprecate a key
    fn deprecate_key(&self, provider: &str, key_id: &str) -> Result<()> {
        let mut keys = self.keys.write().unwrap();

        if let Some(provider_keys) = keys.get_mut(provider) {
            if let Some(key) = provider_keys
                .iter_mut()
                .find(|k| k.metadata.key_id == key_id)
            {
                key.deprecate();

                self.record_event(KeyRotationEvent {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    event_type: KeyRotationEventType::KeyDeprecated,
                    provider: provider.to_string(),
                    old_key_id: Some(key_id.to_string()),
                    new_key_id: None,
                    timestamp: chrono::Utc::now().timestamp(),
                    actor: None,
                    details: None,
                });

                return Ok(());
            }
        }

        Err(ZoeyError::not_found(format!("Key {} not found", key_id)))
    }

    /// Revoke a key immediately
    pub fn revoke_key(&self, provider: &str, key_id: &str) -> Result<()> {
        let mut keys = self.keys.write().unwrap();

        if let Some(provider_keys) = keys.get_mut(provider) {
            if let Some(key) = provider_keys
                .iter_mut()
                .find(|k| k.metadata.key_id == key_id)
            {
                key.revoke();

                self.record_event(KeyRotationEvent {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    event_type: KeyRotationEventType::KeyRevoked,
                    provider: provider.to_string(),
                    old_key_id: Some(key_id.to_string()),
                    new_key_id: None,
                    timestamp: chrono::Utc::now().timestamp(),
                    actor: None,
                    details: Some("Key revoked".to_string()),
                });

                warn!("Key {} revoked for provider {}", key_id, provider);
                return Ok(());
            }
        }

        Err(ZoeyError::not_found(format!("Key {} not found", key_id)))
    }

    /// Record usage of a key
    pub fn record_key_usage(&self, provider: &str, key_id: &str) {
        let mut keys = self.keys.write().unwrap();

        if let Some(provider_keys) = keys.get_mut(provider) {
            if let Some(key) = provider_keys
                .iter_mut()
                .find(|k| k.metadata.key_id == key_id)
            {
                key.record_usage();
            }
        }
    }

    /// Get all key metadata for a provider
    pub fn list_keys(&self, provider: &str) -> Vec<ApiKeyMetadata> {
        self.keys
            .read()
            .unwrap()
            .get(provider)
            .map(|keys| keys.iter().map(|k| k.metadata.clone()).collect())
            .unwrap_or_default()
    }

    /// Get rotation events
    pub fn get_events(&self, limit: usize) -> Vec<KeyRotationEvent> {
        self.events
            .read()
            .unwrap()
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Clean up expired and old revoked keys
    pub fn cleanup(&self) {
        let mut keys = self.keys.write().unwrap();

        for provider_keys in keys.values_mut() {
            provider_keys.retain(|k| {
                // Keep active, pending, and recently deprecated keys
                match k.metadata.status {
                    KeyStatus::Active | KeyStatus::Pending => true,
                    KeyStatus::Deprecated => !k.metadata.is_expired(),
                    KeyStatus::Revoked | KeyStatus::Expired => {
                        // Keep for 7 days for audit purposes
                        let seven_days_ago = chrono::Utc::now().timestamp() - 7 * 24 * 3600;
                        k.metadata.created_at > seven_days_ago
                    }
                }
            });
        }
    }

    /// Record an event
    fn record_event(&self, event: KeyRotationEvent) {
        self.events.write().unwrap().push(event);
    }
}

impl Default for ApiKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for testing keys before activation
#[async_trait::async_trait]
pub trait KeyTester: Send + Sync {
    /// Test if a key is valid
    async fn test_key(&self, key: &str) -> Result<()>;
}

/// Simple key tester that always succeeds (for testing)
pub struct NoOpKeyTester;

#[async_trait::async_trait]
impl KeyTester for NoOpKeyTester {
    async fn test_key(&self, _key: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_creation() {
        let key = ApiKey::new("openai", "sk-test-key".to_string(), None);

        assert_eq!(key.metadata.status, KeyStatus::Pending);
        assert_eq!(key.metadata.provider, "openai");
        assert_eq!(key.secret(), "sk-test-key");
    }

    #[test]
    fn test_key_lifecycle() {
        let mut key = ApiKey::new("openai", "sk-test".to_string(), None);

        // Initial state
        assert_eq!(key.metadata.status, KeyStatus::Pending);
        assert!(!key.metadata.is_usable());

        // Activate
        key.activate();
        assert_eq!(key.metadata.status, KeyStatus::Active);
        assert!(key.metadata.is_usable());

        // Deprecate
        key.deprecate();
        assert_eq!(key.metadata.status, KeyStatus::Deprecated);
        assert!(key.metadata.is_usable()); // Still usable when deprecated

        // Revoke
        key.revoke();
        assert_eq!(key.metadata.status, KeyStatus::Revoked);
        assert!(!key.metadata.is_usable());
    }

    #[test]
    fn test_key_manager() {
        let manager = ApiKeyManager::new();

        // Add a key
        let mut key = ApiKey::new("openai", "sk-test".to_string(), None);
        key.activate();
        let key_id = manager.add_key(key).unwrap();

        // Get active key
        let active = manager.get_active_key("openai");
        assert!(active.is_some());
        assert_eq!(active.unwrap().metadata.key_id, key_id);
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let manager = ApiKeyManager::new();
        manager.register_tester("test", NoOpKeyTester);

        // Add initial key
        let mut key = ApiKey::new("test", "old-key".to_string(), None);
        key.activate();
        let old_id = manager.add_key(key).unwrap();

        // Rotate
        let new_id = manager
            .rotate_key("test", "new-key".to_string())
            .await
            .unwrap();

        // New key should be active
        let active = manager.get_active_key("test").unwrap();
        assert_eq!(active.metadata.key_id, new_id);
        assert_eq!(active.secret(), "new-key");

        // Old key should be deprecated
        let keys = manager.list_keys("test");
        let old_key = keys.iter().find(|k| k.key_id == old_id).unwrap();
        assert_eq!(old_key.status, KeyStatus::Deprecated);
    }
}
