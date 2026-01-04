//! Security features: encryption, validation, rate limiting

use crate::{ZoeyError, Result};
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Derive a 256-bit encryption key from a password using Argon2
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let argon2 = Argon2::default();
    let salt_string = SaltString::encode_b64(salt)
        .map_err(|e| ZoeyError::other(format!("Failed to encode salt: {}", e)))?;

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt_string)
        .map_err(|e| ZoeyError::other(format!("Failed to derive key: {}", e)))?;

    // Extract the hash bytes
    let hash_output = password_hash
        .hash
        .ok_or_else(|| ZoeyError::other("No hash produced"))?;
    let hash_bytes = hash_output.as_bytes();

    // Take first 32 bytes for AES-256
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash_bytes[..32]);
    Ok(key)
}

/// Encrypt a secret value using AES-256-GCM
///
/// This function provides authenticated encryption with the following properties:
/// - Uses AES-256-GCM for encryption (authenticated encryption)
/// - Derives encryption key from password using Argon2
/// - Generates a random salt for key derivation
/// - Generates a random nonce for each encryption
/// - Returns base64-encoded format: salt(16 bytes) || nonce(12 bytes) || ciphertext
pub fn encrypt_secret(value: &str, key: &str) -> Result<String> {
    // Generate random salt for key derivation (16 bytes)
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    // Derive encryption key from password using Argon2
    let derived_key = derive_key(key, &salt)?;

    // Create cipher instance
    let cipher = Aes256Gcm::new(&derived_key.into());

    // Generate random nonce (96 bits / 12 bytes for GCM)
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt the plaintext
    let ciphertext = cipher
        .encrypt(nonce, value.as_bytes())
        .map_err(|e| ZoeyError::other(format!("Encryption failed: {}", e)))?;

    // Combine salt + nonce + ciphertext
    let mut result = Vec::with_capacity(salt.len() + nonce_bytes.len() + ciphertext.len());
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    // Encode as base64
    Ok(BASE64.encode(&result))
}

/// Decrypt a secret value using AES-256-GCM
///
/// Decrypts data encrypted with `encrypt_secret`.
/// Expected format: base64(salt(16) || nonce(12) || ciphertext)
pub fn decrypt_secret(encrypted: &str, key: &str) -> Result<String> {
    // Decode from base64
    let decoded = BASE64
        .decode(encrypted)
        .map_err(|e| ZoeyError::other(format!("Failed to decode base64: {}", e)))?;

    // Verify minimum length (salt + nonce + at least some ciphertext)
    if decoded.len() < 28 + 16 {
        // 16 salt + 12 nonce + 16 min auth tag
        return Err(ZoeyError::other("Invalid encrypted data: too short"));
    }

    // Extract components
    let salt = &decoded[0..16];
    let nonce_bytes = &decoded[16..28];
    let ciphertext = &decoded[28..];

    // Derive the same key using the stored salt
    let derived_key = derive_key(key, salt)?;

    // Create cipher instance
    let cipher = Aes256Gcm::new(&derived_key.into());
    let nonce = Nonce::from_slice(nonce_bytes);

    // Decrypt the ciphertext
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| ZoeyError::other(format!("Decryption failed: {}", e)))?;

    // Convert to UTF-8 string
    String::from_utf8(plaintext)
        .map_err(|e| ZoeyError::other(format!("Invalid UTF-8 in decrypted data: {}", e)))
}

/// Validate input string
pub fn validate_input(input: &str, max_length: usize) -> Result<()> {
    // Check length
    if input.len() > max_length {
        return Err(ZoeyError::validation(format!(
            "Input too long: {} > {}",
            input.len(),
            max_length
        )));
    }

    // Check for null bytes
    if input.contains('\0') {
        return Err(ZoeyError::validation("Input contains null bytes"));
    }

    // Check for control characters (except newlines and tabs)
    for ch in input.chars() {
        if ch.is_control() && ch != '\n' && ch != '\t' && ch != '\r' {
            return Err(ZoeyError::validation(
                "Input contains invalid control characters",
            ));
        }
    }

    Ok(())
}

/// Sanitize input by removing/escaping dangerous characters
pub fn sanitize_input(input: &str) -> String {
    input
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t' || *ch == '\r')
        .collect()
}

/// Maximum key length to prevent memory exhaustion attacks
const MAX_RATE_LIMIT_KEY_LENGTH: usize = 256;

/// Maximum number of tracked keys to prevent memory exhaustion
const MAX_TRACKED_KEYS: usize = 100_000;

/// Rate limiter for API calls
pub struct RateLimiter {
    limits: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    window: Duration,
    max_requests: usize,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(window: Duration, max_requests: usize) -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            window,
            max_requests,
        }
    }

    /// Acquire write lock with poisoning recovery
    fn get_limits_write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, Vec<Instant>>> {
        self.limits.write().unwrap_or_else(|poisoned| {
            tracing::error!("RateLimiter lock was poisoned, recovering");
            poisoned.into_inner()
        })
    }

    /// Check if a request is allowed for a key
    pub fn check(&self, key: &str) -> bool {
        // Validate key length to prevent memory exhaustion
        if key.len() > MAX_RATE_LIMIT_KEY_LENGTH {
            tracing::warn!("Rate limit key too long, rejecting request");
            return false;
        }

        let mut limits = self.get_limits_write();
        let now = Instant::now();

        // Prevent memory exhaustion by limiting tracked keys
        if limits.len() >= MAX_TRACKED_KEYS && !limits.contains_key(key) {
            // Evict oldest entries before adding new one
            let keys_to_remove: Vec<String> = limits
                .iter()
                .filter(|(_, timestamps)| {
                    timestamps
                        .last()
                        .map(|&t| now.duration_since(t) >= self.window)
                        .unwrap_or(true)
                })
                .map(|(k, _)| k.clone())
                .take(1000) // Remove up to 1000 stale entries
                .collect();

            for k in keys_to_remove {
                limits.remove(&k);
            }

            // If still at capacity, reject new keys
            if limits.len() >= MAX_TRACKED_KEYS {
                tracing::warn!("Rate limiter at capacity, rejecting new key");
                return false;
            }
        }

        // Get or create entry for this key
        let timestamps = limits.entry(key.to_string()).or_insert_with(Vec::new);

        // Remove old timestamps outside the window
        timestamps.retain(|&t| now.duration_since(t) < self.window);

        // Check if under limit
        if timestamps.len() < self.max_requests {
            timestamps.push(now);
            true
        } else {
            false
        }
    }

    /// Reset rate limit for a key
    pub fn reset(&self, key: &str) {
        let mut limits = self.get_limits_write();
        limits.remove(key);
    }

    /// Get remaining requests for a key
    pub fn remaining(&self, key: &str) -> usize {
        let mut limits = self.get_limits_write();
        let now = Instant::now();

        if let Some(timestamps) = limits.get_mut(key) {
            // Remove old timestamps
            timestamps.retain(|&t| now.duration_since(t) < self.window);
            self.max_requests.saturating_sub(timestamps.len())
        } else {
            self.max_requests
        }
    }
}

/// Hash a password securely using Argon2id
///
/// This is the recommended approach for password hashing as it provides:
/// - Memory-hard computation (resistant to GPU/ASIC attacks)
/// - Configurable time/memory tradeoffs
/// - Built-in salt handling
///
/// Note: The salt parameter is used as additional context, but Argon2 generates
/// its own cryptographic salt internally.
pub fn hash_password(password: &str, salt: &str) -> String {
    use argon2::password_hash::SaltString;
    use argon2::{Argon2, PasswordHasher};

    // Generate a cryptographic salt (the passed salt is used as pepper/context)
    let salt_string = SaltString::generate(&mut OsRng);

    // Create Argon2id hasher with default secure parameters
    let argon2 = Argon2::default();

    // Combine password with the context salt for additional entropy
    let combined = format!("{}:{}", password, salt);

    // Hash the password
    match argon2.hash_password(combined.as_bytes(), &salt_string) {
        Ok(hash) => hash.to_string(),
        Err(e) => {
            // Fallback to SHA-256 only if Argon2 fails (should never happen)
            tracing::error!("Argon2 hashing failed, using fallback: {}", e);
            let mut hasher = Sha256::new();
            hasher.update(password.as_bytes());
            hasher.update(salt.as_bytes());
            format!("SHA256:{:x}", hasher.finalize())
        }
    }
}

/// Verify a password against an Argon2 hash
///
/// Supports both Argon2 hashes (preferred) and legacy SHA-256 hashes (for migration).
pub fn verify_password(password: &str, salt: &str, hash: &str) -> bool {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};

    // Check for legacy SHA-256 hash format
    if hash.starts_with("SHA256:") {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(salt.as_bytes());
        let computed = format!("SHA256:{:x}", hasher.finalize());
        return computed == hash;
    }

    // Parse the Argon2 hash
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("Failed to parse password hash: {}", e);
            return false;
        }
    };

    // Combine password with context salt (same as in hash_password)
    let combined = format!("{}:{}", password, salt);

    // Verify using Argon2
    Argon2::default()
        .verify_password(combined.as_bytes(), &parsed_hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let plaintext = "Hello, World! This is a secret message.";
        let key = "my-secret-password";

        // Encrypt
        let encrypted = encrypt_secret(plaintext, key).expect("Encryption should succeed");
        assert_ne!(encrypted, plaintext);

        // Decrypt
        let decrypted = decrypt_secret(&encrypted, key).expect("Decryption should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_different_outputs() {
        let plaintext = "Same message";
        let key = "same-key";

        // Multiple encryptions of the same data should produce different outputs
        // due to random salt and nonce
        let encrypted1 = encrypt_secret(plaintext, key).unwrap();
        let encrypted2 = encrypt_secret(plaintext, key).unwrap();
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same plaintext
        assert_eq!(decrypt_secret(&encrypted1, key).unwrap(), plaintext);
        assert_eq!(decrypt_secret(&encrypted2, key).unwrap(), plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let plaintext = "Secret data";
        let key1 = "correct-password";
        let key2 = "wrong-password";

        let encrypted = encrypt_secret(plaintext, key1).unwrap();

        // Decryption with wrong key should fail
        assert!(decrypt_secret(&encrypted, key2).is_err());
    }

    #[test]
    fn test_decrypt_invalid_data() {
        let key = "some-key";

        // Too short data
        assert!(decrypt_secret("dGVzdA==", key).is_err());

        // Invalid base64
        assert!(decrypt_secret("not-valid-base64!!!", key).is_err());

        // Corrupted ciphertext
        let plaintext = "test";
        let encrypted = encrypt_secret(plaintext, key).unwrap();
        let mut corrupted = encrypted.clone();
        corrupted.push('X'); // Corrupt the data
        assert!(decrypt_secret(&corrupted, key).is_err());
    }

    #[test]
    fn test_encrypt_empty_string() {
        let encrypted = encrypt_secret("", "key").unwrap();
        let decrypted = decrypt_secret(&encrypted, "key").unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_encrypt_unicode() {
        let plaintext = "Hello 世界 🌍 مرحبا";
        let key = "unicode-key";

        let encrypted = encrypt_secret(plaintext, key).unwrap();
        let decrypted = decrypt_secret(&encrypted, key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_validate_input() {
        assert!(validate_input("Hello, World!", 100).is_ok());
        assert!(validate_input("A".repeat(1000).as_str(), 100).is_err());
        assert!(validate_input("Hello\0World", 100).is_err());
    }

    #[test]
    fn test_sanitize_input() {
        let input = "Hello\x01World\x02";
        let sanitized = sanitize_input(input);
        assert_eq!(sanitized, "HelloWorld");
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(Duration::from_secs(60), 5);

        // First 5 requests should succeed
        for _ in 0..5 {
            assert!(limiter.check("user1"));
        }

        // 6th request should fail
        assert!(!limiter.check("user1"));

        // Different key should work
        assert!(limiter.check("user2"));

        // Reset should allow more requests
        limiter.reset("user1");
        assert!(limiter.check("user1"));
    }

    #[test]
    fn test_remaining() {
        let limiter = RateLimiter::new(Duration::from_secs(60), 10);

        assert_eq!(limiter.remaining("user1"), 10);
        limiter.check("user1");
        assert_eq!(limiter.remaining("user1"), 9);
    }

    #[test]
    fn test_hash_password() {
        let hash1 = hash_password("password123", "salt");
        let hash2 = hash_password("password123", "salt");
        // Argon2 uses random salt internally, so two hashes will differ
        assert_ne!(hash1, hash2);

        // But verification should pass for the original password
        assert!(verify_password("password123", "salt", &hash1));
        assert!(verify_password("password123", "salt", &hash2));

        // Different password should not verify
        assert!(!verify_password("different", "salt", &hash1));
    }

    #[test]
    fn test_verify_password() {
        let hash = hash_password("secret", "mysalt");
        assert!(verify_password("secret", "mysalt", &hash));
        assert!(!verify_password("wrong", "mysalt", &hash));
    }
}
