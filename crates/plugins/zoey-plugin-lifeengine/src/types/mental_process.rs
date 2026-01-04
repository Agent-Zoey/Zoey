//! Mental Processes - State Machine for Behavioral Modes
//!
//! Inspired by OpenSouls' MentalProcesses, this module implements
//! a state machine where each process defines a behavioral mode
//! (e.g., "introduction", "deep_conversation", "problem_solving")
//! that can transition to another based on context.

use super::{EmotionalState, WorkingMemory};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A behavioral mode the agent can be in
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentalProcess {
    /// Unique identifier
    pub id: Uuid,
    
    /// Process name (e.g., "introduction", "deep_conversation")
    pub name: String,
    
    /// Human-readable description
    pub description: String,
    
    /// Priority level (higher = more important)
    pub priority: i32,
    
    /// Conditions that activate this process
    pub entry_conditions: Vec<ProcessCondition>,
    
    /// Conditions that deactivate this process
    pub exit_conditions: Vec<ProcessCondition>,
    
    /// Allowed transitions to other processes
    pub transitions: Vec<ProcessTransition>,
    
    /// Behavioral modifications when active
    pub behavioral_modifiers: BehavioralModifiers,
    
    /// Maximum duration in seconds (None = unlimited)
    pub max_duration: Option<u64>,
    
    /// Whether this is a background process
    pub background: bool,
}

impl MentalProcess {
    /// Create a new mental process
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            priority: 0,
            entry_conditions: Vec::new(),
            exit_conditions: Vec::new(),
            transitions: Vec::new(),
            behavioral_modifiers: BehavioralModifiers::default(),
            max_duration: None,
            background: false,
        }
    }
    
    /// Add an entry condition
    pub fn with_entry_condition(mut self, condition: ProcessCondition) -> Self {
        self.entry_conditions.push(condition);
        self
    }
    
    /// Add an exit condition
    pub fn with_exit_condition(mut self, condition: ProcessCondition) -> Self {
        self.exit_conditions.push(condition);
        self
    }
    
    /// Add a transition
    pub fn with_transition(mut self, transition: ProcessTransition) -> Self {
        self.transitions.push(transition);
        self
    }
    
    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
    
    /// Set as background process
    pub fn as_background(mut self) -> Self {
        self.background = true;
        self
    }
}

/// Condition for process activation/deactivation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessCondition {
    /// Emotional state condition
    Emotion {
        emotion_type: String,
        min_intensity: f32,
        comparator: Comparator,
    },
    
    /// Working memory contains certain thought types
    ThoughtPresent {
        thought_type: String,
        min_count: usize,
    },
    
    /// Conversation turn count
    TurnCount {
        count: usize,
        comparator: Comparator,
    },
    
    /// Time elapsed since process start
    TimeElapsed {
        seconds: u64,
        comparator: Comparator,
    },
    
    /// Message content matches pattern
    MessageContains {
        patterns: Vec<String>,
        any: bool, // true = any match, false = all must match
    },
    
    /// Entity type check
    EntityType {
        entity_type: String,
    },
    
    /// Custom condition (evaluated by callback)
    Custom {
        name: String,
        params: HashMap<String, serde_json::Value>,
    },
    
    /// Combined conditions
    And(Vec<ProcessCondition>),
    Or(Vec<ProcessCondition>),
    Not(Box<ProcessCondition>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Comparator {
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Equal,
}

/// Transition to another process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessTransition {
    /// Target process name
    pub target: String,
    
    /// Conditions that trigger this transition
    pub conditions: Vec<ProcessCondition>,
    
    /// Priority of this transition
    pub priority: i32,
    
    /// Probability of transition (0.0 to 1.0)
    pub probability: f32,
}

/// Behavioral modifications when a process is active
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BehavioralModifiers {
    /// Response style adjustments
    pub style: StyleModifiers,
    
    /// Goal adjustments
    pub goals: Vec<GoalModifier>,
    
    /// Memory retrieval preferences
    pub memory_bias: MemoryBias,
    
    /// Action restrictions
    pub allowed_actions: Option<Vec<String>>,
    pub blocked_actions: Vec<String>,
}

/// Style modifiers for responses
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StyleModifiers {
    /// Verbosity adjustment (-1.0 = terse, 1.0 = verbose)
    pub verbosity: f32,
    
    /// Formality adjustment (-1.0 = casual, 1.0 = formal)
    pub formality: f32,
    
    /// Empathy adjustment (0.0 = analytical, 1.0 = highly empathetic)
    pub empathy: f32,
    
    /// Directness adjustment (0.0 = indirect, 1.0 = direct)
    pub directness: f32,
    
    /// Creativity adjustment (0.0 = conservative, 1.0 = creative)
    pub creativity: f32,
    
    /// Custom style hints
    pub hints: Vec<String>,
}

/// Goal modifier during a process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalModifier {
    /// Goal name
    pub goal: String,
    /// Priority adjustment
    pub priority_delta: i32,
    /// Whether to suppress this goal
    pub suppress: bool,
}

/// Memory retrieval bias
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryBias {
    /// Prefer recent memories
    pub recency_weight: f32,
    /// Prefer emotionally significant memories
    pub emotional_weight: f32,
    /// Topic focus
    pub topic_filter: Option<Vec<String>>,
    /// Exclude topics
    pub topic_exclude: Vec<String>,
}

/// Active process instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveProcess {
    /// Process definition
    pub process: MentalProcess,
    
    /// When this process was activated
    pub started_at: DateTime<Utc>,
    
    /// Turn count when activated
    pub start_turn: usize,
    
    /// Custom state data
    pub state: HashMap<String, serde_json::Value>,
}

impl ActiveProcess {
    /// Create a new active process
    pub fn new(process: MentalProcess) -> Self {
        Self {
            process,
            started_at: Utc::now(),
            start_turn: 0,
            state: HashMap::new(),
        }
    }
    
    /// Check if this process has exceeded its max duration
    pub fn is_expired(&self) -> bool {
        if let Some(max_duration) = self.process.max_duration {
            let elapsed = Utc::now().signed_duration_since(self.started_at);
            elapsed.num_seconds() as u64 > max_duration
        } else {
            false
        }
    }
    
    /// Get seconds elapsed since activation
    pub fn elapsed_secs(&self) -> u64 {
        let elapsed = Utc::now().signed_duration_since(self.started_at);
        elapsed.num_seconds().max(0) as u64
    }
}

/// Mental process orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessOrchestrator {
    /// All registered processes
    pub processes: HashMap<String, MentalProcess>,
    
    /// Currently active foreground process
    pub active_process: Option<ActiveProcess>,
    
    /// Active background processes
    pub background_processes: Vec<ActiveProcess>,
    
    /// Default process name
    pub default_process: String,
    
    /// Current turn count
    pub turn_count: usize,
    
    /// History of process transitions
    pub transition_history: Vec<TransitionRecord>,
}

/// Record of a process transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRecord {
    pub from: Option<String>,
    pub to: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
    pub turn: usize,
}

impl Default for ProcessOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessOrchestrator {
    /// Create a new orchestrator
    pub fn new() -> Self {
        let mut orchestrator = Self {
            processes: HashMap::new(),
            active_process: None,
            background_processes: Vec::new(),
            default_process: "default".to_string(),
            turn_count: 0,
            transition_history: Vec::new(),
        };
        
        // Register default process
        orchestrator.register(MentalProcess::new(
            "default",
            "Default conversational mode"
        ));
        
        orchestrator
    }
    
    /// Register a mental process
    pub fn register(&mut self, process: MentalProcess) {
        self.processes.insert(process.name.clone(), process);
    }
    
    /// Activate a process by name
    pub fn activate(&mut self, name: &str, reason: &str) -> bool {
        if let Some(process) = self.processes.get(name).cloned() {
            let from = self.active_process.as_ref().map(|p| p.process.name.clone());
            
            if process.background {
                // Add to background processes
                if !self.background_processes.iter().any(|p| p.process.name == name) {
                    self.background_processes.push(ActiveProcess::new(process));
                }
            } else {
                // Replace foreground process
                self.active_process = Some(ActiveProcess::new(process));
            }
            
            self.transition_history.push(TransitionRecord {
                from,
                to: name.to_string(),
                reason: reason.to_string(),
                timestamp: Utc::now(),
                turn: self.turn_count,
            });
            
            true
        } else {
            false
        }
    }
    
    /// Deactivate a background process
    pub fn deactivate_background(&mut self, name: &str) {
        self.background_processes.retain(|p| p.process.name != name);
    }
    
    /// Evaluate conditions and perform transitions
    pub fn evaluate(
        &mut self,
        memory: &WorkingMemory,
        emotion: &EmotionalState,
        message_content: &str,
    ) {
        self.turn_count += 1;
        
        // Check if current process should exit
        if let Some(ref active) = self.active_process {
            if active.is_expired() {
                self.activate(&self.default_process.clone(), "process_expired");
                return;
            }
            
            // Check exit conditions
            for condition in &active.process.exit_conditions {
                if self.evaluate_condition(condition, memory, emotion, message_content) {
                    self.activate(&self.default_process.clone(), "exit_condition_met");
                    return;
                }
            }
            
            // Check transitions
            let mut best_transition: Option<(ProcessTransition, i32)> = None;
            for transition in &active.process.transitions {
                let all_conditions_met = transition.conditions.iter()
                    .all(|c| self.evaluate_condition(c, memory, emotion, message_content));
                
                if all_conditions_met {
                    if let Some((_, best_priority)) = best_transition {
                        if transition.priority > best_priority {
                            best_transition = Some((transition.clone(), transition.priority));
                        }
                    } else {
                        best_transition = Some((transition.clone(), transition.priority));
                    }
                }
            }
            
            if let Some((transition, _)) = best_transition {
                // Apply probability
                if rand::random::<f32>() < transition.probability {
                    self.activate(&transition.target, "transition_triggered");
                }
            }
        } else {
            // No active process, activate default
            self.activate(&self.default_process.clone(), "no_active_process");
        }
        
        // Check if any new process should activate based on entry conditions
        let processes: Vec<_> = self.processes.values().cloned().collect();
        for process in processes {
            if process.name == self.active_process.as_ref().map(|p| p.process.name.as_str()).unwrap_or("") {
                continue;
            }
            
            let all_entry_conditions_met = process.entry_conditions.iter()
                .all(|c| self.evaluate_condition(c, memory, emotion, message_content));
            
            if all_entry_conditions_met && !process.entry_conditions.is_empty() {
                // Check priority
                if let Some(ref active) = self.active_process {
                    if process.priority > active.process.priority {
                        self.activate(&process.name, "higher_priority_entry");
                    }
                }
            }
        }
    }
    
    /// Evaluate a single condition
    fn evaluate_condition(
        &self,
        condition: &ProcessCondition,
        memory: &WorkingMemory,
        emotion: &EmotionalState,
        message_content: &str,
    ) -> bool {
        match condition {
            ProcessCondition::Emotion { emotion_type, min_intensity, comparator } => {
                let intensity = if emotion_type == &format!("{:?}", emotion.primary_emotion).to_lowercase() {
                    emotion.intensity
                } else {
                    emotion.secondary_emotions
                        .iter()
                        .find(|(e, _)| &format!("{:?}", e).to_lowercase() == emotion_type)
                        .map(|(_, &i)| i)
                        .unwrap_or(0.0)
                };
                self.compare(intensity, *min_intensity, *comparator)
            }
            
            ProcessCondition::ThoughtPresent { thought_type, min_count } => {
                // Simple text matching for thought type
                let count = memory.active_thoughts().iter()
                    .filter(|t| format!("{:?}", t.thought_type).to_lowercase() == *thought_type)
                    .count();
                count >= *min_count
            }
            
            ProcessCondition::TurnCount { count, comparator } => {
                self.compare(self.turn_count as f32, *count as f32, *comparator)
            }
            
            ProcessCondition::TimeElapsed { seconds, comparator } => {
                if let Some(ref active) = self.active_process {
                    self.compare(active.elapsed_secs() as f32, *seconds as f32, *comparator)
                } else {
                    false
                }
            }
            
            ProcessCondition::MessageContains { patterns, any } => {
                let lower = message_content.to_lowercase();
                if *any {
                    patterns.iter().any(|p| lower.contains(&p.to_lowercase()))
                } else {
                    patterns.iter().all(|p| lower.contains(&p.to_lowercase()))
                }
            }
            
            ProcessCondition::EntityType { .. } => {
                // Would need entity context to evaluate
                false
            }
            
            ProcessCondition::Custom { .. } => {
                // Custom conditions need external evaluation
                false
            }
            
            ProcessCondition::And(conditions) => {
                conditions.iter().all(|c| self.evaluate_condition(c, memory, emotion, message_content))
            }
            
            ProcessCondition::Or(conditions) => {
                conditions.iter().any(|c| self.evaluate_condition(c, memory, emotion, message_content))
            }
            
            ProcessCondition::Not(condition) => {
                !self.evaluate_condition(condition, memory, emotion, message_content)
            }
        }
    }
    
    fn compare(&self, value: f32, threshold: f32, comparator: Comparator) -> bool {
        match comparator {
            Comparator::GreaterThan => value > threshold,
            Comparator::GreaterOrEqual => value >= threshold,
            Comparator::LessThan => value < threshold,
            Comparator::LessOrEqual => value <= threshold,
            Comparator::Equal => (value - threshold).abs() < 0.001,
        }
    }
    
    /// Get the current behavioral modifiers
    pub fn current_modifiers(&self) -> BehavioralModifiers {
        let mut modifiers = self.active_process
            .as_ref()
            .map(|p| p.process.behavioral_modifiers.clone())
            .unwrap_or_default();
        
        // Merge background process modifiers
        for bg in &self.background_processes {
            let bg_mods = &bg.process.behavioral_modifiers;
            modifiers.style.verbosity += bg_mods.style.verbosity * 0.5;
            modifiers.style.empathy += bg_mods.style.empathy * 0.5;
            modifiers.blocked_actions.extend(bg_mods.blocked_actions.clone());
        }
        
        modifiers
    }
    
    /// Get context string for LLM prompts
    pub fn to_context(&self) -> String {
        let mut parts = Vec::new();
        
        if let Some(ref active) = self.active_process {
            parts.push(format!(
                "Current mode: {} ({})",
                active.process.name,
                active.process.description
            ));
            
            let mods = &active.process.behavioral_modifiers;
            if mods.style.verbosity != 0.0 {
                let verbosity = if mods.style.verbosity > 0.0 { "more verbose" } else { "more concise" };
                parts.push(format!("Style: Be {}", verbosity));
            }
            if mods.style.empathy > 0.5 {
                parts.push("Style: Prioritize empathy and emotional connection".to_string());
            }
            if !mods.style.hints.is_empty() {
                parts.push(format!("Guidelines: {}", mods.style.hints.join(", ")));
            }
        }
        
        parts.join("\n")
    }
}

/// Pre-built process library
pub mod library {
    use super::*;
    
    /// Introduction process - for first interactions
    pub fn introduction_process() -> MentalProcess {
        MentalProcess::new("introduction", "Initial greeting and getting to know the user")
            .with_priority(10)
            .with_entry_condition(ProcessCondition::TurnCount { 
                count: 2, 
                comparator: Comparator::LessOrEqual 
            })
            .with_exit_condition(ProcessCondition::TurnCount { 
                count: 4, 
                comparator: Comparator::GreaterThan 
            })
            .with_transition(ProcessTransition {
                target: "active_listening".to_string(),
                conditions: vec![ProcessCondition::TurnCount { 
                    count: 3, 
                    comparator: Comparator::GreaterOrEqual 
                }],
                priority: 0,
                probability: 1.0,
            })
    }
    
    /// Active listening process - for deep conversations
    pub fn active_listening_process() -> MentalProcess {
        let mut process = MentalProcess::new(
            "active_listening",
            "Deeply engaging with what the user shares"
        );
        process.behavioral_modifiers.style.empathy = 0.8;
        process.behavioral_modifiers.style.directness = 0.4;
        process.behavioral_modifiers.style.hints = vec![
            "Reflect back what you understand".to_string(),
            "Ask follow-up questions".to_string(),
            "Acknowledge emotions".to_string(),
        ];
        process
    }
    
    /// Problem solving process
    pub fn problem_solving_process() -> MentalProcess {
        let mut process = MentalProcess::new(
            "problem_solving",
            "Helping user work through a problem"
        );
        process.entry_conditions = vec![
            ProcessCondition::MessageContains {
                patterns: vec!["help".to_string(), "how do I".to_string(), "problem".to_string()],
                any: true,
            }
        ];
        process.behavioral_modifiers.style.directness = 0.8;
        process.behavioral_modifiers.style.creativity = 0.6;
        process.behavioral_modifiers.style.hints = vec![
            "Break down the problem".to_string(),
            "Offer concrete suggestions".to_string(),
            "Check for understanding".to_string(),
        ];
        process
    }
    
    /// Emotional support process
    pub fn emotional_support_process() -> MentalProcess {
        let mut process = MentalProcess::new(
            "emotional_support",
            "Providing emotional comfort and validation"
        );
        process.priority = 20; // High priority when emotions are involved
        process.entry_conditions = vec![
            ProcessCondition::Or(vec![
                ProcessCondition::Emotion {
                    emotion_type: "sadness".to_string(),
                    min_intensity: 0.5,
                    comparator: Comparator::GreaterOrEqual,
                },
                ProcessCondition::Emotion {
                    emotion_type: "fear".to_string(),
                    min_intensity: 0.5,
                    comparator: Comparator::GreaterOrEqual,
                },
                ProcessCondition::Emotion {
                    emotion_type: "anger".to_string(),
                    min_intensity: 0.6,
                    comparator: Comparator::GreaterOrEqual,
                },
            ])
        ];
        process.behavioral_modifiers.style.empathy = 1.0;
        process.behavioral_modifiers.style.directness = 0.3;
        process.behavioral_modifiers.style.verbosity = 0.3;
        process.behavioral_modifiers.style.hints = vec![
            "Validate their feelings".to_string(),
            "Be warm and supportive".to_string(),
            "Don't try to fix, just be present".to_string(),
        ];
        process
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_process_creation() {
        let process = MentalProcess::new("test", "A test process")
            .with_priority(5)
            .with_entry_condition(ProcessCondition::TurnCount {
                count: 2,
                comparator: Comparator::GreaterOrEqual,
            });
        
        assert_eq!(process.name, "test");
        assert_eq!(process.priority, 5);
        assert_eq!(process.entry_conditions.len(), 1);
    }
    
    #[test]
    fn test_orchestrator_activation() {
        let mut orchestrator = ProcessOrchestrator::new();
        orchestrator.register(MentalProcess::new("test_mode", "Test mode"));
        
        assert!(orchestrator.activate("test_mode", "testing"));
        assert_eq!(
            orchestrator.active_process.as_ref().map(|p| p.process.name.as_str()),
            Some("test_mode")
        );
    }
    
    #[test]
    fn test_condition_evaluation() {
        let orchestrator = ProcessOrchestrator::new();
        let memory = WorkingMemory::new();
        let emotion = EmotionalState::new();
        
        let condition = ProcessCondition::MessageContains {
            patterns: vec!["hello".to_string()],
            any: true,
        };
        
        assert!(orchestrator.evaluate_condition(&condition, &memory, &emotion, "hello world"));
        assert!(!orchestrator.evaluate_condition(&condition, &memory, &emotion, "goodbye"));
    }
}

