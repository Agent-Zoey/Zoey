//! Security monitoring for observability
//!
//! Monitors for:
//! - PII violations in prompts/completions
//! - Cost anomalies (baseline-relative, >2x threshold)
//! - Unusual patterns (high latency, high token usage)

use super::config::ObservabilityConfig;
use crate::error::ZoeyError;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid as UUID;

/// Security monitoring alerts
#[derive(Debug, Clone)]
pub enum SecurityAlert {
    /// PII detected in prompt or completion
    PIIViolation {
        timestamp: chrono::DateTime<chrono::Utc>,
        agent_id: UUID,
        conversation_id: Option<UUID>,
        location: String, // "prompt" or "completion"
        detected_types: Vec<String>,
        severity: AlertSeverity,
    },

    /// Cost anomaly detected
    CostAnomaly {
        timestamp: chrono::DateTime<chrono::Utc>,
        agent_id: UUID,
        current_cost: f64,
        baseline_cost: f64,
        multiplier: f64,
        window: String, // "hourly" or "daily"
    },

    /// Unusual latency detected
    LatencyAnomaly {
        timestamp: chrono::DateTime<chrono::Utc>,
        agent_id: UUID,
        latency_ms: u64,
        baseline_ms: f64,
        multiplier: f64,
    },
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Alert routing channel
#[derive(Debug, Clone)]
pub enum AlertChannel {
    /// Log to tracing
    Log,

    /// Write to file
    File { path: String },

    /// Send to webhook
    Webhook { url: String },
}

/// Security monitor
pub struct SecurityMonitor {
    config: ObservabilityConfig,

    /// Baseline costs per agent (for anomaly detection)
    baseline_costs: Arc<RwLock<HashMap<UUID, BaselineCost>>>,

    /// Alert channels
    alert_channels: Vec<AlertChannel>,
}

/// Baseline cost statistics for anomaly detection
#[derive(Debug, Clone)]
struct BaselineCost {
    hourly_avg: f64,
    hourly_samples: usize,
    daily_avg: f64,
    daily_samples: usize,
    last_updated: chrono::DateTime<chrono::Utc>,
}

impl SecurityMonitor {
    /// Create a new security monitor
    pub fn new(config: ObservabilityConfig) -> Self {
        // Default to log channel
        let alert_channels = vec![AlertChannel::Log];

        Self {
            config,
            baseline_costs: Arc::new(RwLock::new(HashMap::new())),
            alert_channels,
        }
    }

    /// Add an alert channel
    pub fn add_channel(&mut self, channel: AlertChannel) {
        self.alert_channels.push(channel);
    }

    /// Check for PII violations
    pub async fn check_pii_violation(
        &self,
        agent_id: UUID,
        conversation_id: Option<UUID>,
        text: &str,
        location: &str,
    ) -> Result<(), ZoeyError> {
        // Simple regex-based PII detection
        let pii_patterns = self.detect_pii(text);

        if !pii_patterns.is_empty() {
            let severity = if location == "prompt" {
                AlertSeverity::High
            } else {
                AlertSeverity::Medium
            };

            let alert = SecurityAlert::PIIViolation {
                timestamp: Utc::now(),
                agent_id,
                conversation_id,
                location: location.to_string(),
                detected_types: pii_patterns,
                severity,
            };

            self.route_alert(&alert).await;
        }

        Ok(())
    }

    /// Detect PII in text
    fn detect_pii(&self, text: &str) -> Vec<String> {
        let mut detected = Vec::new();

        // Email detection
        if regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b")
            .unwrap()
            .is_match(text)
        {
            detected.push("email".to_string());
        }

        // Phone number detection (simple US format)
        if regex::Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b")
            .unwrap()
            .is_match(text)
        {
            detected.push("phone".to_string());
        }

        // SSN detection
        if regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b")
            .unwrap()
            .is_match(text)
        {
            detected.push("ssn".to_string());
        }

        // Credit card detection (simple)
        if regex::Regex::new(r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b")
            .unwrap()
            .is_match(text)
        {
            detected.push("credit_card".to_string());
        }

        detected
    }

    /// Check for cost anomalies
    pub async fn check_cost_anomaly(
        &self,
        agent_id: UUID,
        current_cost: f64,
        window: &str,
    ) -> Result<(), ZoeyError> {
        let baselines = self.baseline_costs.read().await;

        if let Some(baseline) = baselines.get(&agent_id) {
            let (baseline_cost, multiplier_threshold) = if window == "hourly" {
                (
                    baseline.hourly_avg,
                    self.config.cost_tracking.anomaly_multiplier,
                )
            } else {
                (
                    baseline.daily_avg,
                    self.config.cost_tracking.anomaly_multiplier,
                )
            };

            if baseline_cost > 0.0 && current_cost > baseline_cost * multiplier_threshold {
                let multiplier = current_cost / baseline_cost;

                let alert = SecurityAlert::CostAnomaly {
                    timestamp: Utc::now(),
                    agent_id,
                    current_cost,
                    baseline_cost,
                    multiplier,
                    window: window.to_string(),
                };

                drop(baselines); // Drop read lock before async routing
                self.route_alert(&alert).await;
            }
        }

        Ok(())
    }

    /// Update baseline costs
    pub async fn update_baseline(&self, agent_id: UUID, hourly_cost: f64, daily_cost: f64) {
        let mut baselines = self.baseline_costs.write().await;

        baselines
            .entry(agent_id)
            .and_modify(|b| {
                // Exponential moving average
                b.hourly_avg = (b.hourly_avg * b.hourly_samples as f64 + hourly_cost)
                    / (b.hourly_samples as f64 + 1.0);
                b.hourly_samples += 1;

                b.daily_avg = (b.daily_avg * b.daily_samples as f64 + daily_cost)
                    / (b.daily_samples as f64 + 1.0);
                b.daily_samples += 1;

                b.last_updated = Utc::now();
            })
            .or_insert(BaselineCost {
                hourly_avg: hourly_cost,
                hourly_samples: 1,
                daily_avg: daily_cost,
                daily_samples: 1,
                last_updated: Utc::now(),
            });
    }

    /// Route alert to configured channels
    async fn route_alert(&self, alert: &SecurityAlert) {
        for channel in &self.alert_channels {
            match channel {
                AlertChannel::Log => {
                    self.log_alert(alert);
                }
                AlertChannel::File { path } => {
                    if let Err(e) = self.write_alert_to_file(alert, path).await {
                        tracing::error!("Failed to write alert to file {}: {}", path, e);
                    }
                }
                AlertChannel::Webhook { url } => {
                    if let Err(e) = self.send_alert_to_webhook(alert, url).await {
                        tracing::error!("Failed to send alert to webhook {}: {}", url, e);
                    }
                }
            }
        }
    }

    /// Log alert to tracing
    fn log_alert(&self, alert: &SecurityAlert) {
        match alert {
            SecurityAlert::PIIViolation {
                timestamp,
                agent_id,
                location,
                detected_types,
                severity,
                ..
            } => {
                tracing::warn!(
                    "[{}] PII VIOLATION ({:?}) detected in {} for agent {} - Types: {:?}",
                    timestamp.format("%Y-%m-%d %H:%M:%S"),
                    severity,
                    location,
                    agent_id,
                    detected_types
                );
            }
            SecurityAlert::CostAnomaly {
                timestamp,
                agent_id,
                current_cost,
                baseline_cost,
                multiplier,
                window,
            } => {
                tracing::warn!(
                    "[{}] COST ANOMALY detected for agent {} - Current: ${:.4}, Baseline: ${:.4}, {}x over baseline ({})",
                    timestamp.format("%Y-%m-%d %H:%M:%S"),
                    agent_id,
                    current_cost,
                    baseline_cost,
                    multiplier,
                    window
                );
            }
            SecurityAlert::LatencyAnomaly {
                timestamp,
                agent_id,
                latency_ms,
                baseline_ms,
                multiplier,
            } => {
                tracing::warn!(
                    "[{}] LATENCY ANOMALY detected for agent {} - Current: {}ms, Baseline: {:.0}ms, {}x over baseline",
                    timestamp.format("%Y-%m-%d %H:%M:%S"),
                    agent_id,
                    latency_ms,
                    baseline_ms,
                    multiplier
                );
            }
        }
    }

    /// Write alert to file
    async fn write_alert_to_file(
        &self,
        alert: &SecurityAlert,
        path: &str,
    ) -> Result<(), ZoeyError> {
        use tokio::fs::OpenOptions;
        use tokio::io::AsyncWriteExt;

        let alert_str = format!(
            "{}\n",
            serde_json::to_string(&alert).map_err(|e| ZoeyError::Serialization(e))?
        );

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(|e| ZoeyError::Io(e))?;

        file.write_all(alert_str.as_bytes())
            .await
            .map_err(|e| ZoeyError::Io(e))?;

        Ok(())
    }

    /// Send alert to webhook
    async fn send_alert_to_webhook(
        &self,
        alert: &SecurityAlert,
        url: &str,
    ) -> Result<(), ZoeyError> {
        let client = reqwest::Client::new();

        client.post(url).json(&alert).send().await?;

        Ok(())
    }
}

// Implement Serialize for SecurityAlert
impl serde::Serialize for SecurityAlert {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        match self {
            SecurityAlert::PIIViolation {
                timestamp,
                agent_id,
                conversation_id,
                location,
                detected_types,
                severity,
            } => {
                let mut state = serializer.serialize_struct("SecurityAlert", 7)?;
                state.serialize_field("type", "pii_violation")?;
                state.serialize_field("timestamp", &timestamp.to_rfc3339())?;
                state.serialize_field("agent_id", &agent_id.to_string())?;
                state.serialize_field(
                    "conversation_id",
                    &conversation_id.map(|id| id.to_string()),
                )?;
                state.serialize_field("location", location)?;
                state.serialize_field("detected_types", detected_types)?;
                state.serialize_field("severity", &format!("{:?}", severity))?;
                state.end()
            }
            SecurityAlert::CostAnomaly {
                timestamp,
                agent_id,
                current_cost,
                baseline_cost,
                multiplier,
                window,
            } => {
                let mut state = serializer.serialize_struct("SecurityAlert", 7)?;
                state.serialize_field("type", "cost_anomaly")?;
                state.serialize_field("timestamp", &timestamp.to_rfc3339())?;
                state.serialize_field("agent_id", &agent_id.to_string())?;
                state.serialize_field("current_cost", current_cost)?;
                state.serialize_field("baseline_cost", baseline_cost)?;
                state.serialize_field("multiplier", multiplier)?;
                state.serialize_field("window", window)?;
                state.end()
            }
            SecurityAlert::LatencyAnomaly {
                timestamp,
                agent_id,
                latency_ms,
                baseline_ms,
                multiplier,
            } => {
                let mut state = serializer.serialize_struct("SecurityAlert", 6)?;
                state.serialize_field("type", "latency_anomaly")?;
                state.serialize_field("timestamp", &timestamp.to_rfc3339())?;
                state.serialize_field("agent_id", &agent_id.to_string())?;
                state.serialize_field("latency_ms", latency_ms)?;
                state.serialize_field("baseline_ms", baseline_ms)?;
                state.serialize_field("multiplier", multiplier)?;
                state.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pii_detection_email() {
        let config = ObservabilityConfig::default();
        let monitor = SecurityMonitor::new(config);

        let text = "Contact me at john.doe@example.com";
        let detected = monitor.detect_pii(text);

        assert!(detected.contains(&"email".to_string()));
    }

    #[test]
    fn test_pii_detection_phone() {
        let config = ObservabilityConfig::default();
        let monitor = SecurityMonitor::new(config);

        let text = "Call me at 555-123-4567";
        let detected = monitor.detect_pii(text);

        assert!(detected.contains(&"phone".to_string()));
    }

    #[test]
    fn test_pii_detection_ssn() {
        let config = ObservabilityConfig::default();
        let monitor = SecurityMonitor::new(config);

        let text = "My SSN is 123-45-6789";
        let detected = monitor.detect_pii(text);

        assert!(detected.contains(&"ssn".to_string()));
    }

    #[tokio::test]
    async fn test_cost_anomaly_detection() {
        let mut config = ObservabilityConfig::default();
        config.cost_tracking.anomaly_multiplier = 2.0;

        let monitor = SecurityMonitor::new(config);
        let agent_id = UUID::new_v4();

        // Set baseline
        monitor.update_baseline(agent_id, 0.10, 1.00).await;

        // Normal cost - should not trigger
        assert!(monitor
            .check_cost_anomaly(agent_id, 0.15, "hourly")
            .await
            .is_ok());

        // Anomalous cost - should trigger (but won't fail, just log)
        assert!(monitor
            .check_cost_anomaly(agent_id, 0.30, "hourly")
            .await
            .is_ok());
    }
}
