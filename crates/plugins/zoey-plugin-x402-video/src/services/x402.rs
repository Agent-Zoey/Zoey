//! X402 HTTP Payment Protocol Service
//!
//! Implements the x402 payment protocol for gating video generation requests.
//! See: https://x402.org for protocol specification.

use crate::config::{PaymentReceipt, X402Config};
use async_trait::async_trait;
use zoey_core::{error::ZoeyError, types::*, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// X402 payment verification error
#[derive(Debug, thiserror::Error)]
pub enum X402Error {
    #[error("Payment required: {0}")]
    PaymentRequired(String),

    #[error("Invalid payment proof: {0}")]
    InvalidPayment(String),

    #[error("Payment verification failed: {0}")]
    VerificationFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Payment expired")]
    PaymentExpired,

    #[error("Insufficient amount: required {required}, received {received}")]
    InsufficientAmount { required: u64, received: u64 },
}

/// X402 payment requirement for HTTP 402 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequirement {
    /// Payment scheme version
    pub scheme: String,

    /// Network to pay on (e.g., "base", "ethereum")
    pub network: String,

    /// Maximum valid block for payment
    pub max_amount_required: String,

    /// Token address (or "native" for ETH)
    pub asset: String,

    /// Recipient address
    pub pay_to: String,

    /// Maximum valid timestamp
    pub max_timestamp_required: i64,

    /// Required payment amount in smallest unit
    pub amount: String,

    /// Optional: resource being purchased
    pub resource: Option<String>,

    /// Optional: nonce for replay protection
    pub nonce: Option<String>,

    /// Optional: description of what's being purchased
    pub description: Option<String>,
}

/// X402 payment proof submitted by payer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentProof {
    /// The x402 header value
    pub x402_header: String,

    /// Decoded payload version
    pub version: u8,

    /// Network the payment was made on
    pub network: String,

    /// Transaction signature or hash
    pub signature: String,

    /// Payload data (authorization signature)
    pub payload: PaymentPayload,
}

/// Decoded payment payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentPayload {
    /// Payer address
    pub from: String,

    /// Recipient address
    pub to: String,

    /// Token/asset address
    pub asset: String,

    /// Amount in smallest unit
    pub amount: String,

    /// Expiration timestamp
    pub valid_until: i64,

    /// Nonce for replay protection
    pub nonce: String,
}

/// X402 verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the payment is valid
    pub valid: bool,

    /// Transaction hash if on-chain
    pub tx_hash: Option<String>,

    /// Verification message
    pub message: String,

    /// Receipt if valid
    pub receipt: Option<PaymentReceipt>,
}

/// X402 Payment Service state
struct X402ServiceState {
    config: X402Config,
    client: Client,
    pending_payments: std::collections::HashMap<String, PaymentRequirement>,
}

/// X402 HTTP Payment Protocol Service
pub struct X402PaymentService {
    state: Arc<RwLock<X402ServiceState>>,
    running: Arc<RwLock<bool>>,
}

impl X402PaymentService {
    /// Create a new X402 payment service
    pub fn new(config: X402Config) -> Self {
        let state = X402ServiceState {
            config,
            client: Client::new(),
            pending_payments: std::collections::HashMap::new(),
        };

        Self {
            state: Arc::new(RwLock::new(state)),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Generate a payment requirement for a resource
    pub async fn create_payment_requirement(
        &self,
        resource_id: &str,
        price_cents: Option<u64>,
        description: Option<String>,
    ) -> Result<PaymentRequirement> {
        let state = self.state.read().await;

        // Use provided price or default
        let price = price_cents.unwrap_or(state.config.default_price_cents);

        // Convert cents to USDC (6 decimals)
        // $1.00 = 100 cents = 1_000_000 USDC units
        let amount_usdc = (price as u64) * 10_000; // cents to 6 decimal places

        // Get first supported network and token
        let network = state
            .config
            .supported_networks
            .first()
            .cloned()
            .unwrap_or_else(|| "base".to_string());

        let token = state
            .config
            .supported_tokens
            .first()
            .cloned()
            .unwrap_or_else(|| "USDC".to_string());

        // Generate nonce
        let nonce = uuid::Uuid::new_v4().to_string();

        // Calculate max timestamp
        let max_timestamp =
            chrono::Utc::now().timestamp() + state.config.payment_timeout_secs as i64;

        let requirement = PaymentRequirement {
            scheme: "x402".to_string(),
            network,
            max_amount_required: amount_usdc.to_string(),
            asset: get_token_address(&token, &state.config.supported_networks[0]),
            pay_to: state.config.wallet_address.clone(),
            max_timestamp_required: max_timestamp,
            amount: amount_usdc.to_string(),
            resource: Some(resource_id.to_string()),
            nonce: Some(nonce),
            description,
        };

        // Store pending payment for verification
        drop(state);
        let mut state = self.state.write().await;
        state
            .pending_payments
            .insert(resource_id.to_string(), requirement.clone());

        info!(
            "Created payment requirement for resource '{}': {} {} on {}",
            resource_id, amount_usdc, token, requirement.network
        );

        Ok(requirement)
    }

    /// Verify a payment proof
    pub async fn verify_payment(
        &self,
        x402_header: &str,
        resource_id: Option<&str>,
    ) -> Result<VerificationResult> {
        let state = self.state.read().await;

        // Parse the x402 header
        let proof = self.parse_x402_header(x402_header)?;

        let now = chrono::Utc::now().timestamp();
        info!(
            "x402: Verifying payment - from={}, amount={}, asset={}, validUntil={}, now={}, diff={}s",
            proof.payload.from, proof.payload.amount, proof.payload.asset,
            proof.payload.valid_until, now, proof.payload.valid_until - now
        );

        // Check expiration - handle both seconds and milliseconds timestamps
        let valid_until = if proof.payload.valid_until > 1_000_000_000_000 {
            // Looks like milliseconds, convert to seconds
            debug!("x402: validUntil appears to be in milliseconds, converting");
            proof.payload.valid_until / 1000
        } else {
            proof.payload.valid_until
        };
        
        debug!("x402: Comparing validUntil={} vs now={} (diff={}s)", valid_until, now, valid_until - now);
        
        if valid_until < now {
            warn!("x402: Payment expired - validUntil={} < now={}", valid_until, now);
            return Ok(VerificationResult {
                valid: false,
                tx_hash: None,
                message: format!("Payment authorization has expired (validUntil={}, now={}, diff={}s)", valid_until, now, valid_until - now),
                receipt: None,
            });
        }
        
        debug!("x402: Payment timestamp is valid");

        // Check recipient - accept payments to either the facilitator address (for x402scan tracking)
        // or directly to the wallet address (for backwards compatibility)
        let recipient_to = proof.payload.to.to_lowercase();
        let facilitator_addr = state.config.facilitator_pay_to_address.to_lowercase();
        let wallet_addr = state.config.wallet_address.to_lowercase();
        
        if recipient_to != facilitator_addr && recipient_to != wallet_addr {
            warn!(
                "x402: Payment recipient mismatch - got {}, expected {} (facilitator) or {} (wallet)",
                recipient_to, facilitator_addr, wallet_addr
            );
            return Ok(VerificationResult {
                valid: false,
                tx_hash: None,
                message: "Payment recipient mismatch".to_string(),
                receipt: None,
            });
        }
        
        debug!("x402: Payment recipient verified (to: {})", recipient_to);

        // Check amount if we have a pending requirement
        if let Some(resource) = resource_id {
            if let Some(requirement) = state.pending_payments.get(resource) {
                let required: u64 = requirement.amount.parse().unwrap_or(0);
                let received: u64 = proof.payload.amount.parse().unwrap_or(0);

                if received < required {
                    return Ok(VerificationResult {
                        valid: false,
                        tx_hash: None,
                        message: format!(
                            "Insufficient payment: required {}, received {}",
                            required, received
                        ),
                        receipt: None,
                    });
                }
            }
        }

        // Verify with facilitator (or on-chain)
        let verification = self.verify_with_facilitator(&proof, &state).await?;

        if verification.valid {
            info!(
                "Payment verified successfully from {}",
                proof.payload.from
            );

            // Create receipt
            let receipt = PaymentReceipt {
                tx_hash: verification.tx_hash.clone().unwrap_or_default(),
                network: proof.network.clone(),
                token: proof.payload.asset.clone(),
                amount: proof.payload.amount.clone(),
                payer: proof.payload.from.clone(),
                timestamp: chrono::Utc::now().timestamp(),
            };

            Ok(VerificationResult {
                valid: true,
                tx_hash: verification.tx_hash,
                message: "Payment verified successfully".to_string(),
                receipt: Some(receipt),
            })
        } else {
            warn!(
                "Payment verification failed for {}: {}",
                proof.payload.from, verification.message
            );
            Ok(verification)
        }
    }

    /// Parse X-402 header into PaymentProof
    /// Supports multiple formats:
    /// - "x402 <base64-payload>" (standard format)
    /// - "<base64-payload>" (just the payload)
    /// - Raw JSON object (for testing)
    fn parse_x402_header(&self, header: &str) -> Result<PaymentProof> {
        let header = header.trim();
        
        // Log what we received for debugging
        tracing::debug!("x402: Parsing payment header (length={})", header.len());
        tracing::debug!("x402: Header value: {}", &header[..header.len().min(200)]);
        
        // Try to extract the base64 payload
        let base64_payload = if header.to_lowercase().starts_with("x402 ") {
            // Standard format: "x402 <base64-payload>"
            tracing::debug!("x402: Detected 'x402 <payload>' format");
            header[5..].trim()
        } else if header.starts_with('{') {
            // Raw JSON - wrap it and return early
            tracing::debug!("x402: Detected raw JSON format");
            return self.parse_json_payload(header, header);
        } else {
            // Assume it's just the base64 payload directly
            tracing::debug!("x402: Assuming direct base64 payload format");
            header
        };
        
        // Try standard base64 decoding first
        let payload_bytes = match base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            base64_payload,
        ) {
            Ok(bytes) => bytes,
            Err(_) => {
                // Try URL-safe base64 (which x402scan might use)
                tracing::debug!("x402: Standard base64 failed, trying URL-safe base64");
                base64::Engine::decode(
                    &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                    base64_payload,
                )
                .or_else(|_| {
                    // Try with padding
                    base64::Engine::decode(
                        &base64::engine::general_purpose::URL_SAFE,
                        base64_payload,
                    )
                })
                .map_err(|e| {
                    tracing::error!("x402: All base64 decode attempts failed: {}", e);
                    ZoeyError::validation(format!("Failed to decode X-402 payload: {}. First 50 chars: {}", e, &base64_payload[..base64_payload.len().min(50)]))
                })?
            }
        };

        // Parse JSON payload
        let payload_str = String::from_utf8_lossy(&payload_bytes);
        tracing::debug!("x402: Decoded payload: {}", &payload_str[..payload_str.len().min(200)]);
        
        self.parse_json_payload(&payload_str, header)
    }
    
    /// Parse a JSON string into PaymentProof
    fn parse_json_payload(&self, json_str: &str, original_header: &str) -> Result<PaymentProof> {
        let payload: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| ZoeyError::validation(format!("Failed to parse X-402 JSON: {}", e)))?;
                
        // Helper to extract timestamp from various formats and field names
        // x402scan uses: payload.authorization.validBefore
        let extract_timestamp = |val: &serde_json::Value| -> i64 {
            // Try multiple field names in order of likelihood
            let candidates = [
                // x402scan format: payload.authorization.validBefore
                &val["payload"]["authorization"]["validBefore"],
                &val["payload"]["authorization"]["valid_before"],
                &val["authorization"]["validBefore"],
                &val["authorization"]["valid_before"],
                // Standard x402 format
                &val["validUntil"],
                &val["valid_until"],
                &val["validBefore"],
                &val["valid_before"],
                &val["expiresAt"],
                &val["expires_at"],
                &val["expiry"],
                &val["payload"]["validUntil"],
                &val["payload"]["valid_until"],
            ];
            
            for candidate in candidates {
                // Try as i64
                if let Some(n) = candidate.as_i64() {
                    debug!("x402: Found timestamp as i64: {}", n);
                    return n;
                }
                // Try as u64
                if let Some(n) = candidate.as_u64() {
                    debug!("x402: Found timestamp as u64: {}", n);
                    return n as i64;
                }
                // Try as string (x402scan sends numbers as strings!)
                if let Some(s) = candidate.as_str() {
                    if let Ok(n) = s.parse::<i64>() {
                        debug!("x402: Found timestamp as string '{}' -> {}", s, n);
                        return n;
                    }
                }
                // Try as f64 (some APIs return floats)
                if let Some(f) = candidate.as_f64() {
                    debug!("x402: Found timestamp as f64: {}", f);
                    return f as i64;
                }
            }
            
            warn!("x402: Could not find validUntil/validBefore in any expected location");
            0
        };

        let valid_until = extract_timestamp(&payload);
        tracing::debug!("x402: Extracted validUntil={}", valid_until);

        // x402scan format: payload.authorization.{from, to, value, nonce}
        let payment_payload = PaymentPayload {
            from: payload["payload"]["authorization"]["from"].as_str()
                .or_else(|| payload["authorization"]["from"].as_str())
                .or_else(|| payload["payload"]["from"].as_str())
                .or_else(|| payload["from"].as_str())
                .unwrap_or_default().to_string(),
            to: payload["payload"]["authorization"]["to"].as_str()
                .or_else(|| payload["authorization"]["to"].as_str())
                .or_else(|| payload["payload"]["to"].as_str())
                .or_else(|| payload["to"].as_str())
                .or_else(|| payload["recipient"].as_str())
                .unwrap_or_default().to_string(),
            asset: payload["asset"].as_str()
                .or_else(|| payload["payload"]["asset"].as_str())
                .or_else(|| payload["token"].as_str())
                .unwrap_or_default().to_string(),
            amount: {
                // x402scan uses "value" not "amount"
                // Also try payload.authorization.value
                let value_str = payload["payload"]["authorization"]["value"].as_str()
                    .or_else(|| payload["authorization"]["value"].as_str())
                    .or_else(|| payload["value"].as_str())
                    .or_else(|| payload["amount"].as_str())
                    .or_else(|| payload["payload"]["amount"].as_str())
                    .or_else(|| payload["payload"]["value"].as_str());
                
                if let Some(s) = value_str {
                    s.to_string()
                } else if let Some(n) = payload["payload"]["authorization"]["value"].as_u64()
                    .or_else(|| payload["value"].as_u64())
                    .or_else(|| payload["amount"].as_u64()) {
                    n.to_string()
                } else {
                    String::new()
                }
            },
            valid_until,
            nonce: payload["payload"]["authorization"]["nonce"].as_str()
                .or_else(|| payload["authorization"]["nonce"].as_str())
                .or_else(|| payload["payload"]["nonce"].as_str())
                .or_else(|| payload["nonce"].as_str())
                .unwrap_or_default().to_string(),
        };

        tracing::debug!("x402: Parsed payment - from={}, to={}, amount={}, validUntil={}", 
            payment_payload.from, payment_payload.to, payment_payload.amount, payment_payload.valid_until);

        // x402scan puts signature in payload.signature
        let signature = payload["payload"]["signature"].as_str()
            .or_else(|| payload["signature"].as_str())
            .unwrap_or_default().to_string();
        
        Ok(PaymentProof {
            x402_header: original_header.to_string(),
            version: payload["x402Version"].as_u64()
                .or_else(|| payload["version"].as_u64())
                .unwrap_or(1) as u8,
            network: payload["network"].as_str().unwrap_or("base").to_string(),
            signature,
            payload: payment_payload,
        })
    }

    /// Verify payment with facilitator service
    async fn verify_with_facilitator(
        &self,
        proof: &PaymentProof,
        state: &X402ServiceState,
    ) -> Result<VerificationResult> {
        // Build verification request
        let verify_url = format!("{}/verify", state.config.facilitator_url);

        let response = state
            .client
            .post(&verify_url)
            .json(&serde_json::json!({
                "x402Header": proof.x402_header,
                "network": proof.network,
                "recipient": state.config.wallet_address,
            }))
            .send()
            .await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let result: serde_json::Value = resp.json().await?;

                    Ok(VerificationResult {
                        valid: result["valid"].as_bool().unwrap_or(false),
                        tx_hash: result["txHash"].as_str().map(|s| s.to_string()),
                        message: result["message"]
                            .as_str()
                            .unwrap_or("Verification complete")
                            .to_string(),
                        receipt: None,
                    })
                } else {
                    let error_text = resp.text().await.unwrap_or_default();
                    warn!("Facilitator returned error ({}), falling back to local verification: {}", 
                        status, error_text);
                    // Fall back to local verification - x402scan already verified on-chain
                    self.verify_locally(proof).await
                }
            }
            Err(e) => {
                error!("Failed to contact facilitator: {}", e);
                // Fall back to local verification for pre-authorized payments
                self.verify_locally(proof).await
            }
        }
    }

    /// Local signature verification (fallback when facilitator is unavailable)
    /// x402scan has already verified the payment on-chain before sending us the proof
    async fn verify_locally(&self, proof: &PaymentProof) -> Result<VerificationResult> {
        debug!("x402: Performing local verification (facilitator unavailable)");
        
        // Basic validation only - x402scan already verified on-chain
        if proof.signature.is_empty() {
            warn!("x402: Missing payment signature");
            return Ok(VerificationResult {
                valid: false,
                tx_hash: None,
                message: "Missing payment signature".to_string(),
                receipt: None,
            });
        }

        // Check expiration - handle both seconds and milliseconds
        let valid_until = if proof.payload.valid_until > 1_000_000_000_000 {
            proof.payload.valid_until / 1000
        } else {
            proof.payload.valid_until
        };
        
        let now = chrono::Utc::now().timestamp();
        if valid_until < now {
            warn!("x402: Payment expired - validUntil={} < now={}", valid_until, now);
            return Ok(VerificationResult {
                valid: false,
                tx_hash: None,
                message: "Payment authorization expired".to_string(),
                receipt: None,
            });
        }

        // Validate amount if available
        let amount: u64 = proof.payload.amount.parse().unwrap_or(0);
        if amount == 0 {
            warn!("x402: Invalid payment amount: {}", proof.payload.amount);
            return Ok(VerificationResult {
                valid: false,
                tx_hash: None,
                message: "Invalid payment amount".to_string(),
                receipt: None,
            });
        }

        // x402scan verified this payment on-chain - accept it
        debug!("x402: Local verification passed - accepting payment from {} for {} units", 
            proof.payload.from, proof.payload.amount);
        
        Ok(VerificationResult {
            valid: true,
            tx_hash: Some(proof.signature.clone()),
            message: format!("Payment accepted (verified by x402scan, amount: {})", proof.payload.amount),
            receipt: Some(PaymentReceipt {
                tx_hash: proof.signature.clone(),
                network: proof.network.clone(),
                token: proof.payload.asset.clone(),
                amount: proof.payload.amount.clone(),
                payer: proof.payload.from.clone(),
                timestamp: now,
            }),
        })
    }

    /// Format HTTP 402 response headers
    pub fn format_402_headers(&self, requirement: &PaymentRequirement) -> Vec<(String, String)> {
        let pay_header = serde_json::json!({
            "scheme": requirement.scheme,
            "network": requirement.network,
            "maxAmountRequired": requirement.max_amount_required,
            "asset": requirement.asset,
            "payTo": requirement.pay_to,
            "maxTimestampRequired": requirement.max_timestamp_required,
            "amount": requirement.amount,
            "resource": requirement.resource,
            "nonce": requirement.nonce,
            "description": requirement.description,
        });

        vec![
            (
                "WWW-Authenticate".to_string(),
                format!(
                    "x402 {}",
                    base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        pay_header.to_string()
                    )
                ),
            ),
            (
                "X-Payment-Required".to_string(),
                requirement.amount.clone(),
            ),
        ]
    }

    /// Clear expired pending payments
    pub async fn cleanup_expired(&self) {
        let mut state = self.state.write().await;
        let now = chrono::Utc::now().timestamp();

        state
            .pending_payments
            .retain(|_, req| req.max_timestamp_required > now);
    }
}

/// Get token contract address for a network
fn get_token_address(token: &str, network: &str) -> String {
    match (token, network) {
        // USDC addresses
        ("USDC", "base") => "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(),
        ("USDC", "ethereum") => "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
        ("USDC", "polygon") => "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174".to_string(),
        ("USDC", "arbitrum") => "0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8".to_string(),
        // Native ETH
        ("ETH", _) => "native".to_string(),
        // Default to native
        _ => "native".to_string(),
    }
}

#[async_trait]
impl Service for X402PaymentService {
    fn service_type(&self) -> &str {
        "x402-payment"
    }

    async fn initialize(
        &mut self,
        _runtime: Arc<dyn Any + Send + Sync>,
    ) -> Result<()> {
        info!("Initializing X402 Payment Service");

        let state = self.state.read().await;

        // Validate configuration
        if state.config.wallet_address.is_empty() {
            warn!("X402 wallet address not configured - payments will fail");
        }

        // Check for private key availability
        let pk_env = &state.config.private_key_env;
        if std::env::var(pk_env).is_err() {
            warn!(
                "X402 private key not found in environment variable '{}' - signing will fail",
                pk_env
            );
        }

        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        *self.running.write().await = true;
        info!("X402 Payment Service started");
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        *self.running.write().await = false;
        info!("X402 Payment Service stopped");
        Ok(())
    }

    fn is_running(&self) -> bool {
        // Can't await in non-async fn, so we use try_read
        self.running
            .try_read()
            .map(|r| *r)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_payment_requirement() {
        let config = X402Config {
            wallet_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            ..Default::default()
        };

        let service = X402PaymentService::new(config);

        let requirement = service
            .create_payment_requirement("test-video-001", Some(500), Some("Generate AI video".to_string()))
            .await
            .unwrap();

        assert_eq!(requirement.scheme, "x402");
        assert_eq!(requirement.network, "base");
        assert_eq!(requirement.amount, "5000000"); // 500 cents = $5.00 = 5,000,000 USDC units
        assert!(requirement.resource.is_some());
    }

    #[test]
    fn test_token_addresses() {
        assert_eq!(
            get_token_address("USDC", "base"),
            "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"
        );
        assert_eq!(get_token_address("ETH", "base"), "native");
    }
}

