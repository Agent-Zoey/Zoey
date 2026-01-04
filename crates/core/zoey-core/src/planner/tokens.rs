//! Token tracking and counting

use crate::types::*;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Session-level token usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTokens {
    /// Session ID
    pub session_id: Uuid,
    /// Total input tokens used in session
    pub total_input_tokens: usize,
    /// Total output tokens used in session
    pub total_output_tokens: usize,
    /// Total tokens used
    pub total_tokens: usize,
    /// Total cost in USD
    pub total_cost: f64,
    /// Number of requests in session
    pub request_count: usize,
    /// Session start time
    pub started_at: i64,
    /// Last update time
    pub updated_at: i64,
}

impl SessionTokens {
    /// Create new session token tracker
    pub fn new(session_id: Uuid) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            session_id,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_tokens: 0,
            total_cost: 0.0,
            request_count: 0,
            started_at: now,
            updated_at: now,
        }
    }

    /// Record token usage
    pub fn record(&mut self, usage: &TokenUsage, cost: f64) {
        self.total_input_tokens += usage.prompt_tokens;
        self.total_output_tokens += usage.completion_tokens;
        self.total_tokens += usage.total_tokens;
        self.total_cost += cost;
        self.request_count += 1;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Get average tokens per request
    pub fn avg_tokens_per_request(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.total_tokens as f64 / self.request_count as f64
        }
    }

    /// Get average cost per request
    pub fn avg_cost_per_request(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.total_cost / self.request_count as f64
        }
    }
}

/// Global token tracker
pub struct TokenTracker {
    /// Sessions by session ID
    sessions: Arc<RwLock<HashMap<Uuid, SessionTokens>>>,
    /// Total input tokens across all sessions
    total_input_tokens: Arc<RwLock<usize>>,
    /// Total output tokens across all sessions
    total_output_tokens: Arc<RwLock<usize>>,
    /// Total cost across all sessions
    total_cost: Arc<RwLock<f64>>,
}

impl TokenTracker {
    /// Create a new token tracker
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            total_input_tokens: Arc::new(RwLock::new(0)),
            total_output_tokens: Arc::new(RwLock::new(0)),
            total_cost: Arc::new(RwLock::new(0.0)),
        }
    }

    /// Record token usage for a session
    pub fn record_usage(&self, session_id: Uuid, usage: &TokenUsage, cost: f64) -> Result<()> {
        // Update session
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions
            .entry(session_id)
            .or_insert_with(|| SessionTokens::new(session_id));
        session.record(usage, cost);

        // Update totals
        *self.total_input_tokens.write().unwrap() += usage.prompt_tokens;
        *self.total_output_tokens.write().unwrap() += usage.completion_tokens;
        *self.total_cost.write().unwrap() += cost;

        Ok(())
    }

    /// Get session stats
    pub fn get_session(&self, session_id: &Uuid) -> Option<SessionTokens> {
        self.sessions.read().unwrap().get(session_id).cloned()
    }

    /// Get total cost for a session
    pub fn get_session_cost(&self, session_id: &Uuid) -> f64 {
        self.sessions
            .read()
            .unwrap()
            .get(session_id)
            .map(|s| s.total_cost)
            .unwrap_or(0.0)
    }

    /// Get total tokens for a session
    pub fn get_session_tokens(&self, session_id: &Uuid) -> usize {
        self.sessions
            .read()
            .unwrap()
            .get(session_id)
            .map(|s| s.total_tokens)
            .unwrap_or(0)
    }

    /// Get global totals
    pub fn get_totals(&self) -> TokenTrackerTotals {
        TokenTrackerTotals {
            total_input_tokens: *self.total_input_tokens.read().unwrap(),
            total_output_tokens: *self.total_output_tokens.read().unwrap(),
            total_tokens: *self.total_input_tokens.read().unwrap()
                + *self.total_output_tokens.read().unwrap(),
            total_cost: *self.total_cost.read().unwrap(),
            session_count: self.sessions.read().unwrap().len(),
        }
    }

    /// Get all sessions
    pub fn get_all_sessions(&self) -> Vec<SessionTokens> {
        self.sessions.read().unwrap().values().cloned().collect()
    }

    /// Clear old sessions (older than specified days)
    pub fn cleanup_old_sessions(&self, days: i64) -> usize {
        let cutoff = chrono::Utc::now().timestamp() - (days.max(0) * 86400);
        let mut sessions = self.sessions.write().unwrap();
        let before_count = sessions.len();

        sessions.retain(|_, session| session.updated_at > cutoff);

        before_count - sessions.len()
    }
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Global token tracker totals
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTrackerTotals {
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub total_tokens: usize,
    pub total_cost: f64,
    pub session_count: usize,
}

/// Token counter for estimating token usage
pub struct TokenCounter;

impl TokenCounter {
    /// Estimate tokens in text (rough approximation: 4 chars per token)
    pub fn estimate_tokens(text: &str) -> usize {
        // More sophisticated approximation:
        // - Average English word is ~4.7 characters
        // - Average token is ~0.75 words
        // So roughly: chars / 4 or words * 0.75

        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();

        // Use both methods and average
        let char_estimate = (char_count as f32 / 4.0).ceil() as usize;
        let word_estimate = (word_count as f32 * 0.75).ceil() as usize;

        // Take the average
        ((char_estimate + word_estimate) / 2).max(1)
    }

    /// Estimate tokens for JSON structure
    pub fn estimate_json_tokens(json: &serde_json::Value) -> usize {
        let json_str = serde_json::to_string(json).unwrap_or_default();
        Self::estimate_tokens(&json_str)
    }

    /// Estimate tokens for a state object
    pub fn estimate_state_tokens(state: &State) -> usize {
        let mut total = 0;

        // Estimate from text fields
        for (key, value) in &state.data {
            total += Self::estimate_tokens(key);
            total += Self::estimate_json_tokens(value);
        }

        total
    }

    /// Estimate tokens for a memory object
    pub fn estimate_memory_tokens(memory: &Memory) -> usize {
        let mut total = 0;

        total += Self::estimate_tokens(&memory.content.text);

        // Note: Content structure may vary, add fields as needed
        // if let Some(action) = &memory.content.action {
        //     total += Self::estimate_tokens(action);
        // }

        if let Some(source) = &memory.content.source {
            total += Self::estimate_tokens(source);
        }

        total
    }

    /// Estimate tokens for conversation context
    pub fn estimate_conversation_tokens(messages: &[Memory]) -> usize {
        messages
            .iter()
            .map(|m| Self::estimate_memory_tokens(m))
            .sum()
    }
}

/// Token budget for planning
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBudget {
    /// Maximum input tokens allowed
    pub max_input_tokens: usize,
    /// Maximum output tokens allowed
    pub max_output_tokens: usize,
    /// Estimated input tokens
    pub estimated_input: usize,
    /// Estimated output tokens
    pub estimated_output: usize,
    /// Buffer percentage (0.0 - 1.0)
    pub buffer_percentage: f32,
    /// Whether budget is exceeded
    pub is_exceeded: bool,
}

impl TokenBudget {
    /// Create a new token budget
    pub fn new(
        max_input: usize,
        max_output: usize,
        estimated_input: usize,
        estimated_output: usize,
    ) -> Self {
        Self::with_buffer(
            max_input,
            max_output,
            estimated_input,
            estimated_output,
            0.2,
        )
    }

    /// Create with custom buffer
    pub fn with_buffer(
        max_input: usize,
        max_output: usize,
        estimated_input: usize,
        estimated_output: usize,
        buffer_percentage: f32,
    ) -> Self {
        let buffered_input = (estimated_input as f32 * (1.0 + buffer_percentage)) as usize;
        let buffered_output = (estimated_output as f32 * (1.0 + buffer_percentage)) as usize;

        let is_exceeded = buffered_input > max_input || buffered_output > max_output;

        Self {
            max_input_tokens: max_input,
            max_output_tokens: max_output,
            estimated_input: buffered_input,
            estimated_output: buffered_output,
            buffer_percentage,
            is_exceeded,
        }
    }

    /// Get remaining input tokens
    pub fn remaining_input(&self) -> isize {
        self.max_input_tokens as isize - self.estimated_input as isize
    }

    /// Get remaining output tokens
    pub fn remaining_output(&self) -> isize {
        self.max_output_tokens as isize - self.estimated_output as isize
    }

    /// Get utilization percentage (0.0 - 1.0+)
    pub fn utilization(&self) -> f32 {
        let input_util = self.estimated_input as f32 / self.max_input_tokens as f32;
        let output_util = self.estimated_output as f32 / self.max_output_tokens as f32;
        input_util.max(output_util)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimation() {
        let text = "Hello, this is a test message with some words.";
        let tokens = TokenCounter::estimate_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < 50); // Should be around 10-12 tokens
    }

    #[test]
    fn test_session_tokens() {
        let session_id = Uuid::new_v4();
        let mut session = SessionTokens::new(session_id);

        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        session.record(&usage, 0.01);

        assert_eq!(session.total_input_tokens, 100);
        assert_eq!(session.total_output_tokens, 50);
        assert_eq!(session.total_tokens, 150);
        assert_eq!(session.total_cost, 0.01);
        assert_eq!(session.request_count, 1);
    }

    #[test]
    fn test_token_tracker() {
        let tracker = TokenTracker::new();
        let session_id = Uuid::new_v4();

        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        tracker.record_usage(session_id, &usage, 0.01).unwrap();

        let session = tracker.get_session(&session_id).unwrap();
        assert_eq!(session.total_tokens, 150);

        let totals = tracker.get_totals();
        assert_eq!(totals.total_tokens, 150);
        assert_eq!(totals.total_cost, 0.01);
    }

    #[test]
    fn test_token_budget() {
        let budget = TokenBudget::new(1000, 500, 800, 400);

        assert!(!budget.is_exceeded);
        assert_eq!(budget.remaining_input(), 40); // 1000 - (800 * 1.2) = 1000 - 960 = 40
        assert!(budget.utilization() < 1.0);
    }

    #[test]
    fn test_token_budget_exceeded() {
        let budget = TokenBudget::new(1000, 500, 900, 400);

        assert!(budget.is_exceeded); // 900 * 1.2 > 1000
    }

    #[test]
    fn test_cleanup_old_sessions() {
        let tracker = TokenTracker::new();

        // Add some sessions
        for _ in 0..5 {
            let session_id = Uuid::new_v4();
            let usage = TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            };
            tracker.record_usage(session_id, &usage, 0.01).unwrap();
        }

        // Cleanup sessions older than far future (should remove none)
        let removed = tracker.cleanup_old_sessions(3650);
        assert_eq!(removed, 0);

        // Cleanup sessions older than 0 days (should remove all)
        let removed = tracker.cleanup_old_sessions(0);
        assert_eq!(removed, 5);
    }
}
