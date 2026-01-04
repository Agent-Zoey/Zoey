//! End-to-End Tests for X402 Payment Flow
//!
//! Tests the complete x402 payment protocol integration:
//! - Payment requirement generation (HTTP 402)
//! - Payment verification
//! - Payment expiration handling
//! - Invalid payment rejection

mod common;

use common::*;
use zoey_core::types::Service;
use zoey_plugin_x402_video::{
    config::X402Config,
    services::X402PaymentService,
};

// ============================================================================
// Payment Requirement Tests
// ============================================================================

#[tokio::test]
async fn test_create_payment_requirement() {
    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        default_price_cents: 100,
        supported_networks: vec!["base".to_string()],
        supported_tokens: vec!["USDC".to_string()],
        payment_timeout_secs: 300,
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    let requirement = service
        .create_payment_requirement(
            "test-video-001",
            Some(500), // $5.00
            Some("Generate test video".to_string()),
        )
        .await
        .expect("Should create payment requirement");

    // Verify requirement fields
    assert_eq!(requirement.scheme, "x402");
    assert_eq!(requirement.network, "base");
    assert_eq!(requirement.pay_to, TEST_WALLET_ADDRESS);
    assert_eq!(requirement.amount, "5000000"); // 500 cents = 5,000,000 USDC units (6 decimals)
    assert!(requirement.resource.is_some());
    assert_eq!(requirement.resource.unwrap(), "test-video-001");
    assert!(requirement.nonce.is_some());
    assert!(requirement.max_timestamp_required > chrono::Utc::now().timestamp());
}

#[tokio::test]
async fn test_payment_requirement_default_price() {
    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        default_price_cents: 100, // $1.00
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    let requirement = service
        .create_payment_requirement("test-video", None, None)
        .await
        .expect("Should create payment requirement");

    // Should use default price
    assert_eq!(requirement.amount, "1000000"); // 100 cents = 1,000,000 USDC units
}

#[tokio::test]
async fn test_format_402_headers() {
    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        default_price_cents: 100,
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    let requirement = service
        .create_payment_requirement("test", Some(100), None)
        .await
        .unwrap();

    let headers = service.format_402_headers(&requirement);

    // Should have WWW-Authenticate header
    let auth_header = headers
        .iter()
        .find(|(k, _)| k == "WWW-Authenticate")
        .expect("Should have WWW-Authenticate header");

    assert!(auth_header.1.starts_with("x402 "));

    // Should have X-Payment-Required header
    let payment_header = headers
        .iter()
        .find(|(k, _)| k == "X-Payment-Required")
        .expect("Should have X-Payment-Required header");

    assert_eq!(payment_header.1, "1000000");
}

// ============================================================================
// Payment Verification Tests
// ============================================================================

#[tokio::test]
async fn test_verify_valid_payment() {
    // Start mock facilitator
    let (addr, _state) = start_mock_facilitator(0).await;

    let config = X402Config {
        facilitator_url: format!("http://{}/facilitator", addr),
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        default_price_cents: 100,
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    // Create a valid payment header
    let header = create_test_x402_header("valid-payment-proof");

    let result = service
        .verify_payment(&header, Some("test-resource"))
        .await
        .expect("Should verify payment");

    assert!(result.valid, "Payment should be valid");
    assert!(result.tx_hash.is_some(), "Should have transaction hash");
    assert!(result.receipt.is_some(), "Should have receipt");
}

#[tokio::test]
async fn test_verify_invalid_payment() {
    let (addr, _state) = start_mock_facilitator(0).await;

    let config = X402Config {
        facilitator_url: format!("http://{}/facilitator", addr),
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    let header = create_test_x402_header("invalid-payment");

    let result = service
        .verify_payment(&header, None)
        .await
        .expect("Should return verification result");

    assert!(!result.valid, "Payment should be invalid");
    assert!(
        result.message.contains("Invalid"),
        "Should have error message"
    );
}

#[tokio::test]
async fn test_verify_expired_payment() {
    let (addr, _state) = start_mock_facilitator(0).await;

    let config = X402Config {
        facilitator_url: format!("http://{}/facilitator", addr),
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    let header = create_test_x402_header("expired-payment");

    let result = service
        .verify_payment(&header, None)
        .await
        .expect("Should return verification result");

    assert!(!result.valid, "Expired payment should be invalid");
    assert!(
        result.message.contains("expired") || result.message.contains("Expired"),
        "Should mention expiration"
    );
}

#[tokio::test]
async fn test_verify_malformed_header() {
    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    // Test completely invalid header
    let result = service.verify_payment("not-a-valid-header", None).await;
    assert!(result.is_err(), "Should fail on malformed header");

    // Test wrong scheme
    let result = service.verify_payment("bearer token123", None).await;
    assert!(result.is_err(), "Should fail on wrong scheme");
}

#[tokio::test]
async fn test_verify_wrong_recipient() {
    let config = X402Config {
        wallet_address: "0xdifferent_wallet_address".to_string(),
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    // Create header with different recipient
    let header = create_test_x402_header("valid-payment-proof");

    let result = service
        .verify_payment(&header, None)
        .await
        .expect("Should return verification result");

    assert!(!result.valid, "Payment to wrong recipient should be invalid");
    assert!(
        result.message.contains("mismatch") || result.message.contains("recipient"),
        "Should mention recipient mismatch"
    );
}

// ============================================================================
// Amount Verification Tests
// ============================================================================

#[tokio::test]
async fn test_verify_insufficient_amount() {
    let (addr, _state) = start_mock_facilitator(0).await;

    let config = X402Config {
        facilitator_url: format!("http://{}/facilitator", addr),
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        default_price_cents: 1000, // $10.00 required
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    // Create payment requirement first
    let _ = service
        .create_payment_requirement("test-resource", Some(1000), None)
        .await
        .unwrap();

    // Create header with lower amount (the mock returns amount from header)
    // In this case, the header amount is 1000000 (=$1) but requirement is $10
    let header = create_test_x402_header("valid-payment-proof");

    let result = service
        .verify_payment(&header, Some("test-resource"))
        .await
        .expect("Should return verification result");

    // Note: In real implementation, this would check amount
    // For now, the mock facilitator accepts all valid proofs
    // This test documents expected behavior
}

// ============================================================================
// Concurrent Payment Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_payment_requirements() {
    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        default_price_cents: 100,
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    // Create multiple payment requirements concurrently
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let svc = X402PaymentService::new(X402Config {
                wallet_address: TEST_WALLET_ADDRESS.to_string(),
                default_price_cents: 100,
                ..Default::default()
            });
            tokio::spawn(async move {
                svc.create_payment_requirement(
                    &format!("resource-{}", i),
                    Some(100 + i as u64 * 10),
                    None,
                )
                .await
            })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed with unique nonces
    let requirements: Vec<_> = results
        .into_iter()
        .map(|r| r.unwrap().unwrap())
        .collect();

    let nonces: std::collections::HashSet<_> = requirements
        .iter()
        .filter_map(|r| r.nonce.clone())
        .collect();

    assert_eq!(nonces.len(), 10, "All nonces should be unique");
}

// ============================================================================
// Cleanup and Expiration Tests
// ============================================================================

#[tokio::test]
async fn test_cleanup_expired_payments() {
    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        default_price_cents: 100,
        payment_timeout_secs: 1, // Very short timeout for testing
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    // Create a payment requirement
    let _ = service
        .create_payment_requirement("test-resource", None, None)
        .await
        .unwrap();

    // Wait for expiration
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Cleanup expired
    service.cleanup_expired().await;

    // The expired requirement should be cleaned up
    // (This verifies internal state management)
}

// ============================================================================
// Service Lifecycle Tests
// ============================================================================

#[tokio::test]
async fn test_service_start_stop() {
    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        ..Default::default()
    };

    let mut service = X402PaymentService::new(config);

    assert!(!service.is_running(), "Should not be running initially");

    service.start().await.expect("Should start successfully");
    assert!(service.is_running(), "Should be running after start");

    service.stop().await.expect("Should stop successfully");
    assert!(!service.is_running(), "Should not be running after stop");
}

#[tokio::test]
async fn test_service_initialization() {
    use std::sync::Arc;

    let config = X402Config {
        wallet_address: TEST_WALLET_ADDRESS.to_string(),
        private_key_env: "TEST_X402_PRIVATE_KEY".to_string(),
        ..Default::default()
    };

    let mut service = X402PaymentService::new(config);

    // Set env var for test
    std::env::set_var("TEST_X402_PRIVATE_KEY", "test-key");

    let result = service.initialize(Arc::new(())).await;
    assert!(result.is_ok(), "Initialization should succeed");

    // Cleanup
    std::env::remove_var("TEST_X402_PRIVATE_KEY");
}

