//! Output Planner Functor - Plans output before sending reply

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// Output plan for reply stage
#[derive(Debug, Clone)]
pub struct OutputPlan {
    /// Planned response text
    pub planned_text: String,

    /// Tone to use
    pub tone: Tone,

    /// Whether output needs sanitization
    pub needs_sanitization: bool,

    /// Compliance check result
    pub compliance_passed: bool,

    /// PII detected and removed
    pub pii_removed: Vec<String>,

    /// Redacted content
    pub redactions: Vec<String>,
}

/// Tone for the response
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Professional tone
    Professional,
    /// Friendly tone
    Friendly,
    /// Formal tone
    Formal,
    /// Empathetic tone
    Empathetic,
}

/// Output Planner Functor
/// Plans the output strategy in the reply stage
pub struct OutputPlannerFunctor;

#[async_trait]
impl Provider for OutputPlannerFunctor {
    fn name(&self) -> &str {
        "output_planner"
    }

    fn description(&self) -> Option<String> {
        Some("Plans the output strategy before sending reply".to_string())
    }

    fn position(&self) -> i32 {
        100 // Execute late, after context providers
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<ProviderResult> {
        // Plan the output strategy
        let plan = self.plan_output(message, state).await?;

        let mut result = ProviderResult::default();

        result.text = Some(format!(
            "Output Plan:\n\
            - Tone: {:?}\n\
            - Needs Sanitization: {}\n\
            - Compliance: {}\n\
            - PII Removed: {:?}\n\
            - Redactions: {}",
            plan.tone,
            plan.needs_sanitization,
            if plan.compliance_passed {
                "PASSED"
            } else {
                "FAILED"
            },
            plan.pii_removed,
            plan.redactions.len()
        ));

        result.data = Some({
            let mut data = std::collections::HashMap::new();
            data.insert(
                "output_plan".to_string(),
                serde_json::json!({
                    "tone": format!("{:?}", plan.tone),
                    "needs_sanitization": plan.needs_sanitization,
                    "compliance_passed": plan.compliance_passed,
                    "pii_removed": plan.pii_removed,
                    "redaction_count": plan.redactions.len(),
                }),
            );
            data
        });

        Ok(result)
    }
}

impl OutputPlannerFunctor {
    /// Plan the output strategy
    async fn plan_output(&self, message: &Memory, _state: &State) -> Result<OutputPlan> {
        let text = &message.content.text;

        // Determine appropriate tone
        let tone = self.determine_tone(text);

        // Check if output needs sanitization
        let needs_sanitization = self.needs_sanitization(text);

        // Scan for PII and remove
        let (pii_removed, redactions) = self.scan_and_redact(text);

        // Check compliance
        let compliance_passed = pii_removed.is_empty() && redactions.is_empty();

        Ok(OutputPlan {
            planned_text: text.clone(),
            tone,
            needs_sanitization,
            compliance_passed,
            pii_removed,
            redactions,
        })
    }

    /// Determine appropriate tone for response
    fn determine_tone(&self, text: &str) -> Tone {
        let lower = text.to_lowercase();

        if lower.contains("urgent") || lower.contains("serious") {
            Tone::Formal
        } else if lower.contains("sad") || lower.contains("worried") {
            Tone::Empathetic
        } else if lower.contains("thanks") || lower.contains("hi") {
            Tone::Friendly
        } else {
            Tone::Professional
        }
    }

    /// Check if output needs sanitization
    fn needs_sanitization(&self, text: &str) -> bool {
        // Check for control characters or dangerous content
        text.chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    }

    /// Scan for PII and create redactions
    fn scan_and_redact(&self, text: &str) -> (Vec<String>, Vec<String>) {
        let mut pii_removed = Vec::new();
        let mut redactions = Vec::new();

        // Scan for email addresses
        if text.contains('@') && text.contains('.') {
            pii_removed.push("EMAIL".to_string());
            redactions.push("email address".to_string());
        }

        // Scan for phone numbers (simple pattern)
        if text.contains("phone") || text.contains("call") {
            if text.chars().filter(|c| c.is_numeric()).count() >= 10 {
                pii_removed.push("PHONE".to_string());
                redactions.push("phone number".to_string());
            }
        }

        // Scan for SSN pattern
        if text.contains("ssn") || text.contains("social security") {
            pii_removed.push("SSN".to_string());
            redactions.push("social security number".to_string());
        }

        // Scan for credit card
        if text.contains("credit card") || text.contains("card number") {
            pii_removed.push("CREDIT_CARD".to_string());
            redactions.push("credit card number".to_string());
        }

        // Scan for API keys or private keys
        if text.contains("api_key") || text.contains("private_key") || text.contains("secret") {
            pii_removed.push("API_KEY".to_string());
            redactions.push("API key or secret".to_string());
        }

        (pii_removed, redactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reaction_planner() {
        let planner = OutputPlannerFunctor;

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Hello, how are you?".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };

        let state = State::new();
        let result = planner.get(Arc::new(()), &message, &state).await.unwrap();

        assert!(result.text.is_some());
        assert!(result.data.is_some());
    }

    #[test]
    fn test_tone_determination() {
        let planner = OutputPlannerFunctor;

        assert_eq!(planner.determine_tone("This is urgent!"), Tone::Formal);
        assert_eq!(planner.determine_tone("I'm feeling sad"), Tone::Empathetic);
        assert_eq!(planner.determine_tone("Hi there!"), Tone::Friendly);
        assert_eq!(
            planner.determine_tone("Regular message"),
            Tone::Professional
        );
    }

    #[test]
    fn test_pii_detection() {
        let planner = OutputPlannerFunctor;

        let (pii, redactions) = planner.scan_and_redact("My email is test@example.com");
        assert!(
            pii.contains(&"EMAIL".to_string()) || redactions.contains(&"email address".to_string())
        );

        let (pii, redactions) =
            planner.scan_and_redact("My SSN is 123-45-6789 for social security");
        assert!(
            pii.contains(&"SSN".to_string())
                || redactions.contains(&"social security number".to_string())
        );

        let (pii, redactions) = planner.scan_and_redact("Here's my api_key: sk-1234");
        assert!(
            pii.contains(&"API_KEY".to_string())
                || redactions.contains(&"API key or secret".to_string())
        );
    }
}
