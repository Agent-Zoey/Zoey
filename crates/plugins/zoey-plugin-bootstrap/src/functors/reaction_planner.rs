//! Reaction Planner Functor - Plans how agent should react before execution

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::sync::Arc;

/// Reaction plan created by the planner
#[derive(Debug, Clone)]
pub struct ReactionPlan {
    /// Whether to respond at all
    pub should_respond: bool,

    /// Reasoning for the decision
    pub reasoning: String,

    /// Planned actions to take
    pub planned_actions: Vec<String>,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,

    /// Risk assessment
    pub risk_level: RiskLevel,

    /// Compliance checks passed
    pub compliance_passed: bool,
}

/// Risk level for planned reaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Low risk - safe to proceed
    Low,
    /// Medium risk - proceed with caution
    Medium,
    /// High risk - requires review
    High,
    /// Critical risk - block action
    Critical,
}

/// Reaction Planner Functor
/// Plans the reaction strategy before execution in the bootstrap phase
pub struct ReactionPlannerFunctor;

#[async_trait]
impl Provider for ReactionPlannerFunctor {
    fn name(&self) -> &str {
        "reaction_planner"
    }

    fn description(&self) -> Option<String> {
        Some("Plans the agent's reaction strategy before execution".to_string())
    }

    fn position(&self) -> i32 {
        -100 // Execute early, before other providers
    }

    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<ProviderResult> {
        // Analyze the message and context to plan reaction
        let plan = self.plan_reaction(message, state).await?;

        let mut result = ProviderResult::default();

        result.text = Some(format!(
            "Reaction Plan:\n\
            - Should Respond: {}\n\
            - Reasoning: {}\n\
            - Planned Actions: {:?}\n\
            - Confidence: {:.2}\n\
            - Risk Level: {:?}\n\
            - Compliance: {}",
            plan.should_respond,
            plan.reasoning,
            plan.planned_actions,
            plan.confidence,
            plan.risk_level,
            if plan.compliance_passed {
                "PASSED"
            } else {
                "FAILED"
            }
        ));

        result.data = Some({
            let mut data = std::collections::HashMap::new();
            data.insert(
                "reaction_plan".to_string(),
                serde_json::json!({
                    "should_respond": plan.should_respond,
                    "reasoning": plan.reasoning,
                    "planned_actions": plan.planned_actions,
                    "confidence": plan.confidence,
                    "risk_level": format!("{:?}", plan.risk_level),
                    "compliance_passed": plan.compliance_passed,
                }),
            );
            data
        });

        Ok(result)
    }
}

impl ReactionPlannerFunctor {
    /// Plan the reaction strategy
    async fn plan_reaction(&self, message: &Memory, state: &State) -> Result<ReactionPlan> {
        let text = &message.content.text.to_lowercase();

        // Analyze message intent and sentiment
        let intent = self.detect_intent(text);
        let sentiment = self.analyze_sentiment(text);

        // Analyze message for risk factors
        let risk_level = self.assess_risk(text);

        // Check compliance
        let compliance_passed = self.check_compliance(text);

        // Check if message is directed at agent
        let is_directed = self.is_message_directed(text, state);

        // Determine if should respond
        let should_respond = compliance_passed
            && risk_level != RiskLevel::Critical
            && (is_directed || intent.requires_response);

        // Plan actions based on message intent and context
        let planned_actions = if should_respond {
            self.select_actions(&intent, sentiment, text)
        } else {
            vec!["IGNORE".to_string()]
        };

        // Calculate confidence based on multiple factors
        let confidence =
            self.calculate_confidence(compliance_passed, &risk_level, &intent, is_directed);

        let reasoning = format!(
            "Intent: {:?} | Sentiment: {:?} | Risk: {:?} | Directed: {} | Compliance: {} | Actions: {:?}",
            intent.category, sentiment, risk_level, is_directed, 
            if compliance_passed { "PASS" } else { "FAIL" }, 
            planned_actions
        );

        Ok(ReactionPlan {
            should_respond,
            reasoning,
            planned_actions,
            confidence,
            risk_level,
            compliance_passed,
        })
    }

    /// Detect message intent
    fn detect_intent(&self, text: &str) -> MessageIntent {
        // Questions
        if text.contains("?")
            || text.starts_with("what")
            || text.starts_with("how")
            || text.starts_with("why")
            || text.starts_with("when")
            || text.starts_with("where")
            || text.starts_with("who")
            || text.starts_with("can you")
            || text.starts_with("could you")
        {
            return MessageIntent {
                category: IntentCategory::Question,
                requires_response: true,
                confidence: 0.9,
            };
        }

        // Commands/Requests
        if text.starts_with("please")
            || text.starts_with("help")
            || text.contains("can you help")
            || text.contains("need help")
            || text.contains("show me")
            || text.contains("tell me")
        {
            return MessageIntent {
                category: IntentCategory::Request,
                requires_response: true,
                confidence: 0.85,
            };
        }

        // Greetings
        if text.contains("hello")
            || text.contains("hi ")
            || text.contains("hey")
            || text.contains("good morning")
            || text.contains("good evening")
            || text == "hi"
            || text == "hey"
        {
            return MessageIntent {
                category: IntentCategory::Greeting,
                requires_response: true,
                confidence: 0.95,
            };
        }

        // Farewells
        if text.contains("goodbye")
            || text.contains("bye")
            || text.contains("see you")
            || text.contains("thanks for")
            || text.contains("thank you for")
        {
            return MessageIntent {
                category: IntentCategory::Farewell,
                requires_response: true,
                confidence: 0.9,
            };
        }

        // Statements that might need response
        if text.len() > 20 && !text.contains("just fyi") && !text.contains("note:") {
            return MessageIntent {
                category: IntentCategory::Statement,
                requires_response: true,
                confidence: 0.6,
            };
        }

        // Default: informational, may not need response
        MessageIntent {
            category: IntentCategory::Informational,
            requires_response: false,
            confidence: 0.5,
        }
    }

    /// Analyze sentiment of message
    fn analyze_sentiment(&self, text: &str) -> Sentiment {
        let positive_words = [
            "good",
            "great",
            "awesome",
            "excellent",
            "thank",
            "thanks",
            "love",
            "nice",
            "wonderful",
            "amazing",
            "perfect",
        ];
        let negative_words = [
            "bad", "terrible", "awful", "hate", "horrible", "wrong", "problem", "issue", "error",
            "fail", "broken",
        ];
        let urgent_words = [
            "urgent",
            "emergency",
            "critical",
            "asap",
            "immediately",
            "now",
        ];

        let positive_count = positive_words
            .iter()
            .filter(|&word| text.contains(word))
            .count();
        let negative_count = negative_words
            .iter()
            .filter(|&word| text.contains(word))
            .count();
        let urgent_count = urgent_words
            .iter()
            .filter(|&word| text.contains(word))
            .count();

        if urgent_count > 0 {
            Sentiment::Urgent
        } else if positive_count > negative_count {
            Sentiment::Positive
        } else if negative_count > positive_count {
            Sentiment::Negative
        } else {
            Sentiment::Neutral
        }
    }

    /// Check if message is directed at the agent
    fn is_message_directed(&self, text: &str, state: &State) -> bool {
        // Check for direct mentions or agent name
        if let Some(agent_name) = state.data.get("agent_name") {
            if let Some(name) = agent_name.as_str() {
                if text.contains(&name.to_lowercase()) {
                    return true;
                }
            }
        }

        // Check for common addressing patterns
        text.starts_with("@")
            || text.contains("hey agent")
            || text.contains("you ")
            || text.contains("can you")
            || text.contains("could you")
            || text.contains("would you")
    }

    /// Select appropriate actions based on intent and context
    fn select_actions(
        &self,
        intent: &MessageIntent,
        sentiment: Sentiment,
        text: &str,
    ) -> Vec<String> {
        match intent.category {
            IntentCategory::Question | IntentCategory::Request => {
                vec!["REPLY".to_string()]
            }
            IntentCategory::Greeting => {
                vec!["REPLY".to_string()]
            }
            IntentCategory::Farewell => {
                vec!["REPLY".to_string()]
            }
            IntentCategory::Statement => {
                // Respond to statements that seem to need acknowledgment
                if sentiment == Sentiment::Urgent || text.contains("important") {
                    vec!["REPLY".to_string()]
                } else if intent.confidence > 0.7 {
                    vec!["REPLY".to_string()]
                } else {
                    vec!["NONE".to_string()]
                }
            }
            IntentCategory::Informational => {
                vec!["NONE".to_string()]
            }
        }
    }

    /// Calculate confidence score
    fn calculate_confidence(
        &self,
        compliance: bool,
        risk: &RiskLevel,
        intent: &MessageIntent,
        directed: bool,
    ) -> f32 {
        let mut confidence = 0.5;

        // Compliance factor
        if compliance {
            confidence += 0.2;
        } else {
            confidence -= 0.3;
        }

        // Risk factor
        match risk {
            RiskLevel::Low => confidence += 0.2,
            RiskLevel::Medium => confidence += 0.0,
            RiskLevel::High => confidence -= 0.2,
            RiskLevel::Critical => confidence -= 0.5,
        }

        // Intent confidence
        confidence += intent.confidence * 0.2;

        // Direct message boost
        if directed {
            confidence += 0.1;
        }

        // Clamp between 0.0 and 1.0
        confidence.max(0.0).min(1.0)
    }

    /// Assess risk level of the message
    fn assess_risk(&self, text: &str) -> RiskLevel {
        // Critical risk patterns
        if text.contains("execute") && text.contains("code")
            || text.contains("run") && text.contains("script")
            || text.contains("sudo")
            || text.contains("rm -rf")
            || text.contains("drop table")
        {
            return RiskLevel::Critical;
        }

        // High risk patterns
        if text.contains("password")
            || text.contains("secret")
            || text.contains("private key")
            || text.contains("api key")
            || text.contains("access token")
            || text.contains("credentials")
        {
            return RiskLevel::High;
        }

        // Medium risk patterns
        if text.contains("delete")
            || text.contains("remove all")
            || text.contains("modify")
            || text.contains("change settings")
        {
            return RiskLevel::Medium;
        }

        RiskLevel::Low
    }

    /// Check compliance requirements
    fn check_compliance(&self, text: &str) -> bool {
        // Ensure no PII is being leaked
        // Ensure no credentials in message
        // Check against compliance rules

        // Basic PII patterns
        let has_ssn = text.contains("ssn") && text.chars().filter(|c| c.is_numeric()).count() >= 9;
        let has_credit_card = (text.contains("credit card") || text.contains("card number"))
            && text.chars().filter(|c| c.is_numeric()).count() >= 13;
        let has_api_key = text.contains("api_key") || text.contains("api-key");
        let has_private_key = text.contains("private_key") || text.contains("private-key");

        // Phone number pattern (simplified)
        let has_phone = text.matches(char::is_numeric).count() >= 10
            && (text.contains("phone") || text.contains("call"));

        !has_ssn && !has_credit_card && !has_api_key && !has_private_key && !has_phone
    }
}

/// Message intent analysis
#[derive(Debug, Clone)]
struct MessageIntent {
    category: IntentCategory,
    requires_response: bool,
    confidence: f32,
}

/// Intent categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntentCategory {
    Question,
    Request,
    Greeting,
    Farewell,
    Statement,
    Informational,
}

/// Sentiment analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sentiment {
    Positive,
    Negative,
    Neutral,
    Urgent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reaction_planner() {
        let planner = ReactionPlannerFunctor;

        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content {
                text: "Can you help me with a question?".to_string(),
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
    fn test_risk_assessment() {
        let planner = ReactionPlannerFunctor;

        assert_eq!(planner.assess_risk("hello world"), RiskLevel::Low);
        assert_eq!(
            planner.assess_risk("what's your password?"),
            RiskLevel::High
        );
        assert_eq!(planner.assess_risk("delete all files"), RiskLevel::Medium);
        assert_eq!(
            planner.assess_risk("execute code from untrusted source"),
            RiskLevel::Critical
        );
        assert_eq!(
            planner.assess_risk("run script with sudo"),
            RiskLevel::Critical
        );
    }

    #[test]
    fn test_compliance_check() {
        let planner = ReactionPlannerFunctor;

        assert!(planner.check_compliance("hello world"));
        assert!(!planner.check_compliance("my ssn is 123-45-6789"));
        assert!(!planner.check_compliance("here's my api_key: sk-1234"));
        assert!(planner.check_compliance("can you help me with rust programming?"));
    }

    #[test]
    fn test_intent_detection() {
        let planner = ReactionPlannerFunctor;

        let question = planner.detect_intent("how are you?");
        assert_eq!(question.category, IntentCategory::Question);
        assert!(question.requires_response);

        let greeting = planner.detect_intent("hello there");
        assert_eq!(greeting.category, IntentCategory::Greeting);
        assert!(greeting.requires_response);

        let request = planner.detect_intent("please help me");
        assert_eq!(request.category, IntentCategory::Request);
        assert!(request.requires_response);

        let farewell = planner.detect_intent("goodbye and thanks");
        assert_eq!(farewell.category, IntentCategory::Farewell);
        assert!(farewell.requires_response);
    }

    #[test]
    fn test_sentiment_analysis() {
        let planner = ReactionPlannerFunctor;

        assert_eq!(
            planner.analyze_sentiment("this is great and awesome"),
            Sentiment::Positive
        );
        assert_eq!(
            planner.analyze_sentiment("this is terrible and awful"),
            Sentiment::Negative
        );
        assert_eq!(
            planner.analyze_sentiment("the sky is blue"),
            Sentiment::Neutral
        );
        assert_eq!(
            planner.analyze_sentiment("urgent emergency please help"),
            Sentiment::Urgent
        );
    }

    #[test]
    fn test_confidence_calculation() {
        let planner = ReactionPlannerFunctor;

        let high_intent = MessageIntent {
            category: IntentCategory::Question,
            requires_response: true,
            confidence: 0.9,
        };

        let low_intent = MessageIntent {
            category: IntentCategory::Informational,
            requires_response: false,
            confidence: 0.3,
        };

        // High confidence: compliant, low risk, high intent confidence, directed
        // 0.5 + 0.2 + 0.2 + 0.18 + 0.1 = 1.18 -> 1.0 (clamped)
        let conf1 = planner.calculate_confidence(true, &RiskLevel::Low, &high_intent, true);
        assert!(conf1 > 0.9);

        // Low confidence: non-compliant, high risk
        // 0.5 - 0.3 - 0.2 + 0.18 + 0.0 = 0.18
        let conf2 = planner.calculate_confidence(false, &RiskLevel::High, &high_intent, false);
        assert!(conf2 < 0.3);

        // Medium confidence: compliant, medium risk, low intent
        // 0.5 + 0.2 + 0.0 + 0.06 + 0.0 = 0.76
        let conf3 = planner.calculate_confidence(true, &RiskLevel::Medium, &low_intent, false);
        assert!(conf3 > 0.6 && conf3 < 0.9);

        // Critical risk should have very low confidence
        // 0.5 + 0.2 - 0.5 + 0.18 + 0.0 = 0.38
        let conf4 = planner.calculate_confidence(true, &RiskLevel::Critical, &high_intent, false);
        assert!(conf4 < 0.5);
    }
}
