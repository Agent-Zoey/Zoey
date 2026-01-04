//! Authentication for Agent API
//!
//! Provides secure token-based authentication and authorization

use super::types::{ApiPermission, ApiToken};
use crate::{ZoeyError, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Authentication manager for agent API
pub struct ApiAuthManager {
    /// Token hash -> ApiToken mapping
    tokens: Arc<RwLock<HashMap<String, ApiToken>>>,
}

impl ApiAuthManager {
    /// Create new authentication manager
    pub fn new(tokens: Vec<ApiToken>) -> Self {
        let token_map: HashMap<String, ApiToken> =
            tokens.into_iter().map(|t| (t.token.clone(), t)).collect();

        Self {
            tokens: Arc::new(RwLock::new(token_map)),
        }
    }

    /// Create with no tokens (authentication disabled)
    pub fn disabled() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Hash a token for secure storage
    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Validate an authentication token and return permissions
    pub async fn validate_token(&self, token: &str) -> Result<Vec<ApiPermission>> {
        let token_hash = Self::hash_token(token);

        let tokens = self.tokens.read().await;

        // If no tokens configured, allow access (authentication disabled)
        if tokens.is_empty() {
            debug!("Authentication disabled, allowing access");
            return Ok(vec![
                ApiPermission::Read,
                ApiPermission::Write,
                ApiPermission::Execute,
            ]);
        }

        let api_token = tokens.get(&token_hash).ok_or_else(|| {
            warn!("Authentication failed: invalid token");
            ZoeyError::Config("Invalid authentication token".to_string())
        })?;

        // Check if token is expired
        if let Some(expires_at) = api_token.expires_at {
            let now = chrono::Utc::now().timestamp();
            if now > expires_at {
                warn!("Authentication failed: token expired");
                return Err(ZoeyError::Config(
                    "Authentication token has expired".to_string(),
                ));
            }
        }

        debug!(
            "Token validated successfully: {} (permissions: {:?})",
            api_token.name, api_token.permissions
        );

        Ok(api_token.permissions.clone())
    }

    /// Check if token has specific permission
    pub async fn has_permission(&self, token: &str, permission: ApiPermission) -> Result<bool> {
        let permissions = self.validate_token(token).await?;

        // Admin has all permissions
        if permissions.contains(&ApiPermission::Admin) {
            return Ok(true);
        }

        Ok(permissions.contains(&permission))
    }

    /// Add a new token
    pub async fn add_token(&self, token: ApiToken) {
        let mut tokens = self.tokens.write().await;
        tokens.insert(token.token.clone(), token);
    }

    /// Remove a token
    pub async fn remove_token(&self, token_hash: &str) -> bool {
        let mut tokens = self.tokens.write().await;
        tokens.remove(token_hash).is_some()
    }

    /// List all tokens (without sensitive data)
    pub async fn list_tokens(&self) -> Vec<String> {
        let tokens = self.tokens.read().await;
        tokens.values().map(|t| t.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_authentication() {
        let token = ApiToken {
            token: ApiAuthManager::hash_token("test-token"),
            name: "Test Token".to_string(),
            permissions: vec![ApiPermission::Read, ApiPermission::Write],
            expires_at: None,
            agent_id: Some(Uuid::new_v4()),
        };

        let auth = ApiAuthManager::new(vec![token]);

        // Valid token
        let perms = auth.validate_token("test-token").await.unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&ApiPermission::Read));

        // Invalid token
        assert!(auth.validate_token("invalid").await.is_err());
    }

    #[tokio::test]
    async fn test_disabled_auth() {
        let auth = ApiAuthManager::disabled();
        let perms = auth.validate_token("any-token").await.unwrap();
        assert!(perms.contains(&ApiPermission::Read));
    }

    #[tokio::test]
    async fn test_has_permission() {
        let admin_token = ApiToken {
            token: ApiAuthManager::hash_token("admin-token"),
            name: "Admin Token".to_string(),
            permissions: vec![ApiPermission::Admin],
            expires_at: None,
            agent_id: None,
        };

        let auth = ApiAuthManager::new(vec![admin_token]);

        // Admin has all permissions
        assert!(auth
            .has_permission("admin-token", ApiPermission::Execute)
            .await
            .unwrap());
    }

    #[test]
    fn test_token_hashing() {
        let hash1 = ApiAuthManager::hash_token("test");
        let hash2 = ApiAuthManager::hash_token("test");
        let hash3 = ApiAuthManager::hash_token("different");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
