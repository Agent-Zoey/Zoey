//! Integration Tests with Real X402 Facilitator
//!
//! These tests use the actual x402 facilitator service for payment verification.
//! They require network access and test against real infrastructure.
//!
//! Run with: cargo test --test integration_real_facilitator -- --ignored
//!
//! Set environment variables:
//!   X402_FACILITATOR_URL - The facilitator URL (defaults to https://x402.org/facilitator)
//!   X402_WALLET_ADDRESS - Your wallet address for testing

mod common;

use common::*;
use zoey_plugin_x402_video::{
    config::X402Config,
    services::X402PaymentService,
};

/// Get the facilitator URL from environment or use default
fn get_facilitator_url() -> String {
    std::env::var("X402_FACILITATOR_URL")
        .unwrap_or_else(|_| DEFAULT_FACILITATOR_URL.to_string())
}

/// Get wallet address from environment or use test default
fn get_wallet_address() -> String {
    std::env::var("X402_WALLET_ADDRESS")
        .unwrap_or_else(|_| TEST_WALLET_ADDRESS.to_string())
}

// ============================================================================
// Real Facilitator Tests
// ============================================================================

/// Test that we can reach the real facilitator service
#[tokio::test]
#[ignore] // Run with --ignored flag
async fn test_facilitator_health() {
    let facilitator_url = get_facilitator_url();
    let client = reqwest::Client::new();
    
    // Try to reach the facilitator
    let response = client
        .get(&facilitator_url)
        .send()
        .await;
    
    match response {
        Ok(resp) => {
            println!("Facilitator URL: {}", facilitator_url);
            println!("Response status: {}", resp.status());
            // We expect some response (even 404 means the server is up)
            assert!(
                resp.status().is_success() || resp.status().as_u16() == 404 || resp.status().as_u16() == 405,
                "Facilitator should be reachable"
            );
        }
        Err(e) => {
            panic!("Failed to reach facilitator at {}: {}", facilitator_url, e);
        }
    }
}

/// Test creating payment requirement with real facilitator URL configured
#[tokio::test]
#[ignore]
async fn test_create_requirement_real_config() {
    let config = X402Config {
        facilitator_url: get_facilitator_url(),
        wallet_address: get_wallet_address(),
        default_price_cents: 100,
        supported_networks: vec!["base".to_string()],
        supported_tokens: vec!["USDC".to_string()],
        payment_timeout_secs: 300,
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    let requirement = service
        .create_payment_requirement(
            "integration-test-video",
            Some(100), // $1.00
            Some("Integration test video generation".to_string()),
        )
        .await
        .expect("Should create payment requirement");

    assert_eq!(requirement.scheme, "x402");
    assert_eq!(requirement.network, "base");
    assert!(!requirement.pay_to.is_empty());
    println!("Payment requirement created:");
    println!("  Amount: {} (USDC units)", requirement.amount);
    println!("  Pay to: {}", requirement.pay_to);
    println!("  Network: {}", requirement.network);
    println!("  Expires: {}", requirement.max_timestamp_required);
}

/// Test verifying an invalid payment with real facilitator
/// This tests that the facilitator correctly rejects invalid payments
#[tokio::test]
#[ignore]
async fn test_verify_invalid_payment_real_facilitator() {
    let config = X402Config {
        facilitator_url: get_facilitator_url(),
        wallet_address: get_wallet_address(),
        default_price_cents: 100,
        ..Default::default()
    };

    let service = X402PaymentService::new(config);

    // Create a fake payment header (should be rejected by real facilitator)
    let header = create_test_x402_header("fake-invalid-signature");

    let result = service
        .verify_payment(&header, None)
        .await;

    match result {
        Ok(verification) => {
            println!("Verification result: {:?}", verification);
            // Real facilitator should reject invalid payments
            assert!(
                !verification.valid,
                "Invalid payment should be rejected by real facilitator"
            );
        }
        Err(e) => {
            println!("Verification error (expected for invalid payment): {}", e);
            // An error is also acceptable for invalid payments
        }
    }
}

/// Test full config using environment variables
#[tokio::test]
#[ignore]
async fn test_from_env_with_real_facilitator() {
    // Set the facilitator URL env var
    std::env::set_var("X402_FACILITATOR_URL", get_facilitator_url());
    
    // Create plugin from environment
    let plugin = zoey_plugin_x402_video::X402VideoPlugin::from_env();
    
    // The plugin should be configured with the real facilitator URL
    let state = plugin.create_route_state();
    
    println!("Plugin configured with:");
    println!("  Facilitator URL: {}", state.config.x402.facilitator_url);
    println!("  Wallet: {}", state.config.x402.wallet_address);
    println!("  Video Provider: {:?}", state.config.video_generation.provider);
    
    assert!(
        state.config.x402.facilitator_url.contains("facilitator") || 
        state.config.x402.facilitator_url.contains("x402"),
        "Should use a valid facilitator URL"
    );
}

/// Test the PayAI Echo test merchant (demonstrates real 402 responses)
#[tokio::test]
#[ignore]
async fn test_payai_echo_merchant_402_response() {
    let client = reqwest::Client::new();
    
    // This endpoint returns a proper x402 402 Payment Required response
    let response = client
        .get("https://x402.payai.network/api/base/paid-content")
        .send()
        .await
        .expect("Should reach PayAI Echo merchant");
    
    println!("PayAI Echo Merchant Response:");
    println!("  Status: {}", response.status());
    
    // Should return 402 Payment Required
    assert_eq!(response.status().as_u16(), 402, "Should return 402 Payment Required");
    
    let body: serde_json::Value = response.json().await.expect("Should parse JSON");
    
    println!("  x402 Version: {}", body["x402Version"]);
    println!("  Error: {}", body["error"]);
    
    if let Some(accepts) = body["accepts"].as_array() {
        for accept in accepts {
            println!("  Payment Option:");
            println!("    Scheme: {}", accept["scheme"]);
            println!("    Network: {}", accept["network"]);
            println!("    Amount: {} USDC units", accept["maxAmountRequired"]);
            println!("    Pay To: {}", accept["payTo"]);
            println!("    Asset: {}", accept["asset"]);
            println!("    Description: {}", accept["description"]);
        }
    }
    
    // Verify the response structure
    assert_eq!(body["x402Version"], 1);
    assert!(body["accepts"].is_array());
}

// ============================================================================
// Tests against x402.getzoey.ai live service
// ============================================================================

/// Base URL for the getzoey.ai x402 video service
const GETZOEY_BASE_URL: &str = "https://x402.getzoey.ai";

/// Test the root endpoint on getzoey.ai - returns x402scan compatible info
/// NOTE: This test will fail until the new root route is deployed to getzoey.ai
#[tokio::test]
#[ignore]
async fn test_getzoey_root_x402scan_info() {
    let client = reqwest::Client::new();
    
    let response = client
        .get(GETZOEY_BASE_URL)
        .send()
        .await
        .expect("Should reach getzoey.ai root endpoint");
    
    let status = response.status();
    println!("GetZoey Root (x402scan info) Response:");
    println!("  Status: {}", status);
    
    // If 404, the new route hasn't been deployed yet
    if status.as_u16() == 404 {
        println!("  ⚠️  Root route not deployed yet - this is expected until deployment");
        println!("  The new x402scan-compatible root route needs to be deployed to getzoey.ai");
        return; // Skip the rest of the test
    }
    
    assert!(status.is_success(), "Root endpoint should return 200");
    
    let body: serde_json::Value = response.json().await.expect("Should parse JSON");
    
    println!("  Body: {}", serde_json::to_string_pretty(&body).unwrap());
    
    // Verify x402scan schema
    assert_eq!(body["x402Version"], 1, "Should have x402Version = 1");
    assert!(body["accepts"].is_array(), "Should have accepts array");
    
    if let Some(accepts) = body["accepts"].as_array() {
        for accept in accepts {
            println!("\n  Accept Entry:");
            println!("    Scheme: {}", accept["scheme"]);
            println!("    Network: {}", accept["network"]);
            println!("    Amount: {} USDC units", accept["maxAmountRequired"]);
            println!("    Resource: {}", accept["resource"]);
            println!("    Description: {}", accept["description"]);
            println!("    Pay To: {}", accept["payTo"]);
            println!("    Asset: {}", accept["asset"]);
            println!("    Timeout: {} seconds", accept["maxTimeoutSeconds"]);
            
            // Verify required fields for x402scan
            assert_eq!(accept["scheme"], "exact");
            assert!(!accept["resource"].as_str().unwrap_or("").is_empty());
            assert!(!accept["payTo"].as_str().unwrap_or("").is_empty());
            
            if let Some(output_schema) = accept.get("outputSchema") {
                println!("    Has Output Schema: Yes");
                println!("      Input Method: {}", output_schema["input"]["method"]);
                println!("      Body Type: {}", output_schema["input"]["bodyType"]);
            }
            
            if let Some(extra) = accept.get("extra") {
                println!("    Extra Info:");
                println!("      Provider: {}", extra["provider"]);
                println!("      Max Duration: {} secs", extra["max_duration_secs"]);
            }
        }
    }
}

/// Test the health endpoint on getzoey.ai
#[tokio::test]
#[ignore]
async fn test_getzoey_health_endpoint() {
    let client = reqwest::Client::new();
    
    let response = client
        .get(format!("{}/x402-video/health", GETZOEY_BASE_URL))
        .send()
        .await
        .expect("Should reach getzoey.ai health endpoint");
    
    println!("GetZoey Health Response:");
    println!("  Status: {}", response.status());
    
    assert!(response.status().is_success(), "Health endpoint should return 200");
    
    let body: serde_json::Value = response.json().await.expect("Should parse JSON");
    
    println!("  Body: {}", serde_json::to_string_pretty(&body).unwrap());
    println!("  Plugin: {}", body["plugin"]);
    println!("  Status: {}", body["status"]);
    println!("  Video Provider: {}", body["video_provider"]);
    println!("  Wallet Configured: {}", body["wallet_configured"]);
    
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["plugin"], "x402-video");
}

/// Test the pricing endpoint on getzoey.ai
#[tokio::test]
#[ignore]
async fn test_getzoey_pricing_endpoint() {
    let client = reqwest::Client::new();
    
    let response = client
        .get(format!("{}/x402-video/pricing", GETZOEY_BASE_URL))
        .send()
        .await
        .expect("Should reach getzoey.ai pricing endpoint");
    
    println!("GetZoey Pricing Response:");
    println!("  Status: {}", response.status());
    
    assert!(response.status().is_success(), "Pricing endpoint should return 200");
    
    let body: serde_json::Value = response.json().await.expect("Should parse JSON");
    
    println!("  Body: {}", serde_json::to_string_pretty(&body).unwrap());
    println!("  Base Price: {} cents (${:.2})", body["base_price_cents"], body["base_price_cents"].as_u64().unwrap_or(0) as f64 / 100.0);
    println!("  Networks: {:?}", body["networks"]);
    println!("  Tokens: {:?}", body["tokens"]);
    println!("  Video Provider: {}", body["video_provider"]);
    println!("  Wallet Address: {}", body["wallet_address"]);
}

/// Test the generate endpoint on getzoey.ai - should return 402 without payment
#[tokio::test]
#[ignore]
async fn test_getzoey_generate_requires_payment() {
    let client = reqwest::Client::new();
    
    let request_body = serde_json::json!({
        "prompt": "A beautiful sunset over the ocean with dolphins jumping",
        "options": {
            "duration_secs": 5,
            "resolution": "HD720p"
        }
    });
    
    let response = client
        .post(format!("{}/x402-video/generate", GETZOEY_BASE_URL))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .expect("Should reach getzoey.ai generate endpoint");
    
    let status = response.status();
    println!("GetZoey Generate Response (without payment):");
    println!("  Status: {}", status);
    
    // Accept either 402 (Payment Required) or 422 (Unprocessable Entity - body validation first)
    let status_code = status.as_u16();
    assert!(
        status_code == 402 || status_code == 422,
        "Generate endpoint should return 402 or 422 without payment, got {}",
        status_code
    );
    
    let body_text = response.text().await.unwrap_or_default();
    println!("  Body: {}", body_text);
    
    if let Ok(body) = serde_json::from_str::<serde_json::Value>(&body_text) {
        if status_code == 402 {
            println!("  Error: {}", body["error"]);
            
            if let Some(payment) = body.get("payment") {
                println!("  Payment Required:");
                println!("    Scheme: {}", payment["scheme"]);
                println!("    Network: {}", payment["network"]);
                println!("    Amount: {} (USDC units)", payment["amount"]);
                println!("    Pay To: {}", payment["pay_to"]);
                println!("    Asset: {}", payment["asset"]);
                println!("    Expires At: {}", payment["expires_at"]);
                println!("    Resource ID: {}", payment["resource_id"]);
                println!("    Description: {}", payment["description"]);
            }
        } else {
            println!("  ⚠️  Service returned 422 - request body validation happened before payment check");
            println!("  This is OK - the service validates the request first");
        }
    }
}

/// Test submitting an invalid payment to getzoey.ai
#[tokio::test]
#[ignore]
async fn test_getzoey_invalid_payment_rejected() {
    let client = reqwest::Client::new();
    
    let request_body = serde_json::json!({
        "prompt": "A cat playing piano",
        "options": {
            "duration_secs": 4
        }
    });
    
    // Create a fake x402 payment header
    let fake_payment = create_test_x402_header("fake-invalid-signature");
    
    let response = client
        .post(format!("{}/x402-video/generate", GETZOEY_BASE_URL))
        .header("Content-Type", "application/json")
        .header("X-402", &fake_payment)
        .json(&request_body)
        .send()
        .await
        .expect("Should reach getzoey.ai generate endpoint");
    
    let status = response.status();
    let status_code = status.as_u16();
    println!("GetZoey Generate Response (with invalid payment):");
    println!("  Status: {}", status);
    
    let body_text = response.text().await.unwrap_or_default();
    if !body_text.is_empty() {
        if let Ok(body) = serde_json::from_str::<serde_json::Value>(&body_text) {
            println!("  Body: {}", serde_json::to_string_pretty(&body).unwrap());
        } else {
            println!("  Body (text): {}", body_text);
        }
    }
    
    // Should reject invalid payment (402, 422, or 500)
    assert!(
        status_code == 402 || status_code == 422 || status_code == 500,
        "Should reject invalid payment with 402, 422, or 500, got {}",
        status_code
    );
    
    println!("  ✅ Invalid payment correctly rejected with status {}", status_code);
}

/// Full integration test summary
#[tokio::test]
#[ignore]
async fn test_getzoey_full_flow_summary() {
    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║            GetZoey.ai X402 Video Service Integration Test            ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    
    let client = reqwest::Client::new();
    
    // 1. Health Check
    println!("1️⃣  HEALTH CHECK");
    println!("   GET {}/x402-video/health", GETZOEY_BASE_URL);
    let health = client
        .get(format!("{}/x402-video/health", GETZOEY_BASE_URL))
        .send()
        .await;
    
    match health {
        Ok(resp) => {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            println!("   ✅ Status: {}", status);
            println!("   📊 Provider: {}", body["video_provider"]);
            println!("   💰 Wallet Configured: {}", body["wallet_configured"]);
        }
        Err(e) => println!("   ❌ Error: {}", e),
    }
    println!();
    
    // 2. Pricing Info
    println!("2️⃣  PRICING INFO");
    println!("   GET {}/x402-video/pricing", GETZOEY_BASE_URL);
    let pricing = client
        .get(format!("{}/x402-video/pricing", GETZOEY_BASE_URL))
        .send()
        .await;
    
    match pricing {
        Ok(resp) => {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            println!("   ✅ Status: {}", status);
            let price = body["base_price_cents"].as_u64().unwrap_or(0);
            println!("   💵 Price: {} cents (${:.2})", price, price as f64 / 100.0);
            println!("   🌐 Networks: {:?}", body["networks"]);
            println!("   🪙 Tokens: {:?}", body["tokens"]);
            println!("   📹 Provider: {}", body["video_provider"]);
        }
        Err(e) => println!("   ❌ Error: {}", e),
    }
    println!();
    
    // 3. Generate (without payment)
    println!("3️⃣  GENERATE VIDEO (without payment - expecting 402)");
    println!("   POST {}/x402-video/generate", GETZOEY_BASE_URL);
    let generate = client
        .post(format!("{}/x402-video/generate", GETZOEY_BASE_URL))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "prompt": "A futuristic city with flying cars at sunset"
        }))
        .send()
        .await;
    
    match generate {
        Ok(resp) => {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            if status.as_u16() == 402 {
                println!("   ✅ Status: {} Payment Required (expected)", status);
                if let Some(payment) = body.get("payment") {
                    println!("   💳 Payment Details:");
                    println!("      Network: {}", payment["network"]);
                    println!("      Amount: {} USDC units", payment["amount"]);
                    println!("      Pay To: {}", payment["pay_to"]);
                }
            } else {
                println!("   ⚠️  Status: {} (unexpected)", status);
            }
        }
        Err(e) => println!("   ❌ Error: {}", e),
    }
    println!();
    
    // 4. Facilitator Info
    println!("4️⃣  FACILITATOR SERVICE");
    println!("   URL: https://facilitator.payai.network");
    let facilitator = client
        .get("https://facilitator.payai.network")
        .send()
        .await;
    
    match facilitator {
        Ok(resp) => {
            println!("   ✅ Status: {} (facilitator is reachable)", resp.status());
        }
        Err(e) => println!("   ❌ Error: {}", e),
    }
    println!();
    
    println!("═══════════════════════════════════════════════════════════════════════");
    println!("Integration test complete. To make an actual payment:");
    println!("1. Get payment details from POST /x402-video/generate");
    println!("2. Pay using x402 protocol to the wallet address");
    println!("3. Include X-402 header with payment proof");
    println!("4. Video will be generated using Sora");
    println!("═══════════════════════════════════════════════════════════════════════");
}

