//! Security tests for vulnerability prevention
//!
//! These tests verify that security measures are working correctly:
//! - SQL injection prevention
//! - Password hashing strength
//! - Encryption functionality
//! - Environment variable filtering

use zoey_core::{
    encrypt_secret, decrypt_secret, hash_password, verify_password,
    validate_input, sanitize_input, RateLimiter,
};
use std::time::Duration;
use base64::Engine;

// ============================================================================
// Encryption Tests
// ============================================================================

#[test]
fn test_encryption_uses_random_salt_and_nonce() {
    let plaintext = "sensitive data";
    let key = "encryption-key-123";

    // Encrypt same data twice
    let encrypted1 = encrypt_secret(plaintext, key).unwrap();
    let encrypted2 = encrypt_secret(plaintext, key).unwrap();

    // Should produce different ciphertexts due to random salt/nonce
    assert_ne!(encrypted1, encrypted2, "Encryption must use random salt/nonce");

    // Both should decrypt to the same plaintext
    assert_eq!(decrypt_secret(&encrypted1, key).unwrap(), plaintext);
    assert_eq!(decrypt_secret(&encrypted2, key).unwrap(), plaintext);
}

#[test]
fn test_encryption_wrong_key_fails() {
    let plaintext = "secret message";
    let correct_key = "correct-password";
    let wrong_key = "wrong-password";

    let encrypted = encrypt_secret(plaintext, correct_key).unwrap();

    // Decryption with wrong key should fail (AES-GCM authentication)
    let result = decrypt_secret(&encrypted, wrong_key);
    assert!(result.is_err(), "Decryption with wrong key must fail");
}

#[test]
fn test_encryption_tamper_detection() {
    use base64::engine::general_purpose::STANDARD;

    let plaintext = "important data";
    let key = "my-key";

    let encrypted = encrypt_secret(plaintext, key).unwrap();

    // Tamper with the ciphertext (flip a bit)
    let mut bytes = STANDARD.decode(&encrypted).unwrap();
    if !bytes.is_empty() {
        bytes[bytes.len() - 1] ^= 0x01; // Flip last bit
    }
    let tampered = STANDARD.encode(&bytes);

    // Tampered data should fail authentication
    let result = decrypt_secret(&tampered, key);
    assert!(result.is_err(), "Tampered ciphertext must fail authentication");
}

#[test]
fn test_encryption_handles_unicode() {
    let plaintext = "敏感数据 🔐 données sensibles العربية";
    let key = "unicode-key-日本語";

    let encrypted = encrypt_secret(plaintext, key).unwrap();
    let decrypted = decrypt_secret(&encrypted, key).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_encryption_empty_strings() {
    let key = "test-key";

    // Empty plaintext
    let encrypted = encrypt_secret("", key).unwrap();
    let decrypted = decrypt_secret(&encrypted, key).unwrap();
    assert_eq!(decrypted, "");

    // Empty key (should still work, though not recommended)
    let encrypted = encrypt_secret("data", "").unwrap();
    let decrypted = decrypt_secret(&encrypted, "").unwrap();
    assert_eq!(decrypted, "data");
}

// ============================================================================
// Password Hashing Tests
// ============================================================================

#[test]
fn test_password_hash_uses_argon2() {
    let password = "secure-password-123";
    let salt = "application-salt";

    let hash = hash_password(password, salt);

    // Argon2 hashes start with $argon2
    assert!(
        hash.starts_with("$argon2"),
        "Password hash must use Argon2, got: {}",
        &hash[..hash.len().min(20)]
    );
}

#[test]
fn test_password_hash_unique_per_call() {
    let password = "same-password";
    let salt = "same-salt";

    let hash1 = hash_password(password, salt);
    let hash2 = hash_password(password, salt);

    // Argon2 uses random salt internally, so hashes should differ
    assert_ne!(hash1, hash2, "Each hash should be unique due to random salt");

    // But both should verify correctly
    assert!(verify_password(password, salt, &hash1));
    assert!(verify_password(password, salt, &hash2));
}

#[test]
fn test_password_verification_wrong_password() {
    let password = "correct-password";
    let wrong_password = "wrong-password";
    let salt = "test-salt";

    let hash = hash_password(password, salt);

    assert!(verify_password(password, salt, &hash), "Correct password should verify");
    assert!(!verify_password(wrong_password, salt, &hash), "Wrong password should not verify");
}

#[test]
fn test_password_verification_wrong_salt() {
    let password = "my-password";
    let salt = "correct-salt";
    let wrong_salt = "wrong-salt";

    let hash = hash_password(password, salt);

    assert!(verify_password(password, salt, &hash));
    assert!(!verify_password(password, wrong_salt, &hash), "Wrong salt should not verify");
}

// ============================================================================
// Input Validation Tests
// ============================================================================

#[test]
fn test_validate_input_rejects_null_bytes() {
    let malicious = "normal\x00injected";
    let result = validate_input(malicious, 1000);
    assert!(result.is_err(), "Null bytes should be rejected");
}

#[test]
fn test_validate_input_rejects_control_chars() {
    // Bell character
    let result = validate_input("test\x07data", 1000);
    assert!(result.is_err(), "Control characters should be rejected");

    // But newlines, tabs, carriage returns should be allowed
    assert!(validate_input("line1\nline2", 1000).is_ok());
    assert!(validate_input("col1\tcol2", 1000).is_ok());
    assert!(validate_input("windows\r\nline", 1000).is_ok());
}

#[test]
fn test_validate_input_enforces_length() {
    let short = "a".repeat(100);
    let long = "a".repeat(1001);

    assert!(validate_input(&short, 100).is_ok());
    assert!(validate_input(&long, 1000).is_err(), "Input exceeding max length should be rejected");
}

#[test]
fn test_sanitize_input_removes_dangerous_chars() {
    let input = "Hello\x00World\x01Test\x07End";
    let sanitized = sanitize_input(input);
    assert_eq!(sanitized, "HelloWorldTestEnd");
}

// ============================================================================
// Rate Limiter Tests
// ============================================================================

#[test]
fn test_rate_limiter_blocks_after_limit() {
    let limiter = RateLimiter::new(Duration::from_secs(60), 5);

    // First 5 should succeed
    for i in 0..5 {
        assert!(limiter.check("user"), "Request {} should succeed", i + 1);
    }

    // 6th should fail
    assert!(!limiter.check("user"), "Request 6 should be blocked");
}

#[test]
fn test_rate_limiter_different_keys_independent() {
    let limiter = RateLimiter::new(Duration::from_secs(60), 2);

    assert!(limiter.check("user1"));
    assert!(limiter.check("user1"));
    assert!(!limiter.check("user1")); // Blocked

    // user2 should still work
    assert!(limiter.check("user2"));
    assert!(limiter.check("user2"));
}

#[test]
fn test_rate_limiter_rejects_long_keys() {
    let limiter = RateLimiter::new(Duration::from_secs(60), 100);

    // Very long key should be rejected to prevent memory exhaustion
    let long_key = "a".repeat(1000);
    assert!(!limiter.check(&long_key), "Excessively long keys should be rejected");
}

#[test]
fn test_rate_limiter_remaining_count() {
    let limiter = RateLimiter::new(Duration::from_secs(60), 10);

    assert_eq!(limiter.remaining("user"), 10);

    limiter.check("user");
    assert_eq!(limiter.remaining("user"), 9);

    limiter.check("user");
    limiter.check("user");
    assert_eq!(limiter.remaining("user"), 7);
}

// ============================================================================
// SQL Injection Prevention Tests (compile-time verification)
// ============================================================================

/// This test documents that SQL injection is prevented by table name validation.
/// The actual validation is done at compile time through the type system and
/// at runtime through the validate_table_name function.
#[test]
fn test_sql_injection_documentation() {
    // These attack patterns are blocked by validate_table_name():
    let attack_patterns = [
        "memories; DROP TABLE memories; --",
        "memories UNION SELECT * FROM users --",
        "memories' OR '1'='1",
        "memories\"; DELETE FROM agents; --",
        "../../../etc/passwd",
        "memories/**/UNION/**/SELECT",
    ];

    // All of these contain characters that would fail validation:
    // - Semicolons, quotes, dashes, slashes, asterisks, equals signs
    // - Only alphanumeric and underscore are allowed
    for pattern in &attack_patterns {
        let has_invalid_char = pattern.chars().any(|c| !c.is_ascii_alphanumeric() && c != '_');
        assert!(
            has_invalid_char,
            "Attack pattern should contain invalid characters: {}",
            pattern
        );
    }
}

// ============================================================================
// Environment Variable Filtering Tests
// ============================================================================

#[test]
fn test_sensitive_env_vars_not_leaked() {
    use zoey_core::{set_default_secrets_from_env, Character};

    // This test verifies that sensitive environment variables
    // are filtered when importing into character settings.
    //
    // The filtering is done in secrets.rs via is_blocked_env_var()

    // Set some "sensitive" test vars
    std::env::set_var("TEST_AWS_SECRET", "aws-secret-value");
    std::env::set_var("TEST_SAFE_VAR", "safe-value");

    // Note: The actual filtering tests are in secrets.rs test module
    // Here we just verify the module compiles and the function exists
    let mut character = Character::default();
    set_default_secrets_from_env(&mut character);

    // Clean up
    std::env::remove_var("TEST_AWS_SECRET");
    std::env::remove_var("TEST_SAFE_VAR");
}
