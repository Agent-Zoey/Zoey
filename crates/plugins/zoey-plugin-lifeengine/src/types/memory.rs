//! Working Memory - Immutable collection of thought fragments
//!
//! Unlike traditional conversation history, WorkingMemory represents
//! the agent's active thought process - what it's currently considering,
//! its internal monologue, and transient cognitive states.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A single thought fragment in working memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtFragment {
    /// Unique identifier
    pub id: Uuid,
    
    /// The thought content
    pub content: String,
    
    /// Type of thought (perception, reasoning, emotion, intention, etc.)
    pub thought_type: ThoughtType,
    
    /// Importance score (0.0 - 1.0)
    pub salience: f32,
    
    /// When this thought was created
    pub created_at: DateTime<Utc>,
    
    /// Time-to-live in seconds (None = permanent for this session)
    pub ttl: Option<u64>,
    
    /// Source of the thought
    pub source: ThoughtSource,
    
    /// Associations with other thoughts
    pub associations: Vec<Uuid>,
    
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ThoughtFragment {
    /// Create a new thought fragment
    pub fn new(content: impl Into<String>, thought_type: ThoughtType, source: ThoughtSource) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            thought_type,
            salience: 0.5,
            created_at: Utc::now(),
            ttl: None,
            source,
            associations: Vec::new(),
            metadata: HashMap::new(),
        }
    }
    
    /// Set salience
    pub fn with_salience(mut self, salience: f32) -> Self {
        self.salience = salience.clamp(0.0, 1.0);
        self
    }
    
    /// Set TTL
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.ttl = Some(ttl);
        self
    }
    
    /// Add an association
    pub fn with_association(mut self, thought_id: Uuid) -> Self {
        self.associations.push(thought_id);
        self
    }
    
    /// Check if this thought has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let elapsed = Utc::now().signed_duration_since(self.created_at);
            elapsed.num_seconds() as u64 > ttl
        } else {
            false
        }
    }
}

/// Types of thoughts the agent can have
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThoughtType {
    /// Raw perception from the environment
    Perception,
    /// Reasoning and logical deduction
    Reasoning,
    /// Emotional reaction
    Emotion,
    /// Intention or goal
    Intention,
    /// Memory recall
    Recall,
    /// Self-reflection
    Reflection,
    /// Planning and strategy
    Planning,
    /// Response generation
    Response,
    /// Internal monologue
    InternalMonologue,
    /// Uncertainty or confusion
    Uncertainty,
}

impl ThoughtType {
    /// Get the default TTL for this thought type in seconds
    pub fn default_ttl(&self) -> Option<u64> {
        match self {
            ThoughtType::Perception => Some(60),      // Perceptions fade quickly
            ThoughtType::Emotion => Some(300),         // Emotions linger
            ThoughtType::InternalMonologue => Some(30), // Fleeting
            ThoughtType::Uncertainty => Some(120),     // Moderate persistence
            _ => None,                                  // Other types persist
        }
    }
}

/// Source of a thought
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThoughtSource {
    /// From external input (user message, event)
    External { entity_id: Option<Uuid>, channel: String },
    /// From internal cognitive process
    Internal { process: String },
    /// From memory retrieval
    Memory { memory_id: Uuid },
    /// From another thought (chain of reasoning)
    Derived { parent_id: Uuid },
}

/// Working Memory - the agent's current cognitive workspace
/// 
/// This is an append-only, immutable structure that represents
/// the agent's active thoughts during a conversation or task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkingMemory {
    /// All thought fragments in order of creation
    thoughts: Vec<ThoughtFragment>,
    
    /// Current focus (most salient thoughts)
    focus_stack: Vec<Uuid>,
    
    /// Maximum number of thoughts to retain
    capacity: usize,
    
    /// Session identifier
    session_id: Uuid,
    
    /// When this working memory was created
    created_at: DateTime<Utc>,
}

impl WorkingMemory {
    /// Create a new working memory
    pub fn new() -> Self {
        Self {
            thoughts: Vec::new(),
            focus_stack: Vec::new(),
            capacity: 100,
            session_id: Uuid::new_v4(),
            created_at: Utc::now(),
        }
    }
    
    /// Create with a specific capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            ..Self::new()
        }
    }
    
    /// Add a thought and return a new WorkingMemory (immutable pattern)
    pub fn push(&self, thought: ThoughtFragment) -> Self {
        let mut new_wm = self.clone();
        
        // If at capacity, remove the oldest, lowest-salience thought
        if new_wm.thoughts.len() >= new_wm.capacity {
            new_wm.prune();
        }
        
        new_wm.focus_stack.push(thought.id);
        new_wm.thoughts.push(thought);
        new_wm
    }
    
    /// Get all active (non-expired) thoughts
    pub fn active_thoughts(&self) -> Vec<&ThoughtFragment> {
        self.thoughts.iter().filter(|t| !t.is_expired()).collect()
    }
    
    /// Get thoughts by type
    pub fn thoughts_by_type(&self, thought_type: ThoughtType) -> Vec<&ThoughtFragment> {
        self.active_thoughts()
            .into_iter()
            .filter(|t| t.thought_type == thought_type)
            .collect()
    }
    
    /// Get the most salient thoughts (top N by salience)
    pub fn most_salient(&self, n: usize) -> Vec<&ThoughtFragment> {
        let mut active: Vec<_> = self.active_thoughts();
        active.sort_by(|a, b| b.salience.partial_cmp(&a.salience).unwrap_or(std::cmp::Ordering::Equal));
        active.into_iter().take(n).collect()
    }
    
    /// Get a thought by ID
    pub fn get(&self, id: Uuid) -> Option<&ThoughtFragment> {
        self.thoughts.iter().find(|t| t.id == id)
    }
    
    /// Get the current focus (last N thoughts added)
    pub fn current_focus(&self, n: usize) -> Vec<&ThoughtFragment> {
        self.focus_stack
            .iter()
            .rev()
            .take(n)
            .filter_map(|id| self.get(*id))
            .collect()
    }
    
    /// Serialize working memory to a context string for LLM prompts
    pub fn to_context_string(&self) -> String {
        let mut parts = Vec::new();
        
        // Include recent thoughts grouped by type
        let recent = self.most_salient(10);
        
        if !recent.is_empty() {
            parts.push("Current thoughts:".to_string());
            for thought in recent {
                let type_label = format!("{:?}", thought.thought_type).to_lowercase();
                parts.push(format!("  [{}] {}", type_label, thought.content));
            }
        }
        
        parts.join("\n")
    }
    
    /// Remove expired and low-salience thoughts to stay under capacity
    fn prune(&mut self) {
        // Remove expired thoughts
        self.thoughts.retain(|t| !t.is_expired());
        
        // If still over capacity, remove lowest salience
        while self.thoughts.len() >= self.capacity {
            if let Some(min_idx) = self.thoughts
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.salience.partial_cmp(&b.salience).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
            {
                let removed = self.thoughts.remove(min_idx);
                self.focus_stack.retain(|id| *id != removed.id);
            } else {
                break;
            }
        }
    }
    
    /// Get session ID
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }
    
    /// Get total thought count
    pub fn len(&self) -> usize {
        self.thoughts.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.thoughts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_thought_creation() {
        let thought = ThoughtFragment::new(
            "The user seems frustrated",
            ThoughtType::Emotion,
            ThoughtSource::Internal { process: "emotion_detector".to_string() }
        ).with_salience(0.8);
        
        assert_eq!(thought.salience, 0.8);
        assert_eq!(thought.thought_type, ThoughtType::Emotion);
    }
    
    #[test]
    fn test_working_memory_push() {
        let wm = WorkingMemory::new();
        let thought = ThoughtFragment::new(
            "Hello",
            ThoughtType::Perception,
            ThoughtSource::External { entity_id: None, channel: "chat".to_string() }
        );
        
        let wm2 = wm.push(thought);
        assert_eq!(wm.len(), 0); // Original unchanged
        assert_eq!(wm2.len(), 1); // New has the thought
    }
    
    #[test]
    fn test_most_salient() {
        let wm = WorkingMemory::new()
            .push(ThoughtFragment::new("Low", ThoughtType::Perception, 
                ThoughtSource::Internal { process: "test".to_string() }).with_salience(0.2))
            .push(ThoughtFragment::new("High", ThoughtType::Perception,
                ThoughtSource::Internal { process: "test".to_string() }).with_salience(0.9))
            .push(ThoughtFragment::new("Medium", ThoughtType::Perception,
                ThoughtSource::Internal { process: "test".to_string() }).with_salience(0.5));
        
        let salient = wm.most_salient(2);
        assert_eq!(salient[0].content, "High");
        assert_eq!(salient[1].content, "Medium");
    }
}

