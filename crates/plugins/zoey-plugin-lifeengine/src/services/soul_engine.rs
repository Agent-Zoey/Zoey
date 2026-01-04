//! Soul Engine Service
//!
//! The central orchestrator for the Life Engine. Manages:
//! - Working memory lifecycle
//! - Mental process state machine
//! - Emotional state updates
//! - Drive system
//! - Cognitive step execution

use crate::types::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use zoey_core::{types::*, Result, ZoeyError};

/// Soul state for a specific entity/conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulState {
    /// Entity ID this state belongs to
    pub entity_id: Uuid,
    
    /// Room/conversation ID
    pub room_id: Uuid,
    
    /// Working memory
    pub working_memory: WorkingMemory,
    
    /// Emotional state
    pub emotional_state: EmotionalState,
    
    /// Mental process orchestrator
    pub process_orchestrator: ProcessOrchestrator,
    
    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,
    
    /// Session start time
    pub session_start: DateTime<Utc>,
    
    /// Turn count in this session
    pub turn_count: usize,
    
    /// Custom state data
    pub custom_data: HashMap<String, serde_json::Value>,
}

impl SoulState {
    /// Create a new soul state
    pub fn new(entity_id: Uuid, room_id: Uuid, config: &SoulConfig) -> Self {
        let mut emotional_state = EmotionalState::with_baseline(
            config.emotional_baseline.valence,
            config.emotional_baseline.arousal,
            config.emotional_baseline.dominance,
        );
        
        // Set mood volatility based on neuroticism
        emotional_state.mood.volatility = config.personality.neuroticism;
        
        let mut process_orchestrator = ProcessOrchestrator::new();
        
        // Register standard processes from library
        process_orchestrator.register(crate::types::mental_process::library::introduction_process());
        process_orchestrator.register(crate::types::mental_process::library::active_listening_process());
        process_orchestrator.register(crate::types::mental_process::library::problem_solving_process());
        process_orchestrator.register(crate::types::mental_process::library::emotional_support_process());
        
        Self {
            entity_id,
            room_id,
            working_memory: WorkingMemory::with_capacity(50),
            emotional_state,
            process_orchestrator,
            last_activity: Utc::now(),
            session_start: Utc::now(),
            turn_count: 0,
            custom_data: HashMap::new(),
        }
    }
    
    /// Check if this session has been idle too long
    pub fn is_stale(&self, max_idle_secs: u64) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.last_activity);
        elapsed.num_seconds() as u64 > max_idle_secs
    }
    
    /// Reset for a new session
    pub fn reset_session(&mut self) {
        self.working_memory = WorkingMemory::with_capacity(50);
        self.session_start = Utc::now();
        self.turn_count = 0;
        self.last_activity = Utc::now();
        self.process_orchestrator.turn_count = 0;
        self.process_orchestrator.active_process = None;
    }
}

/// Configuration for the Soul Engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulEngineConfig {
    /// Default soul configuration
    pub default_soul: SoulConfig,
    
    /// Session idle timeout in seconds
    pub session_idle_timeout: u64,
    
    /// Maximum sessions to keep in memory
    pub max_sessions: usize,
    
    /// Whether to persist state between sessions
    pub persist_state: bool,
    
    /// Emotional decay interval in seconds
    pub emotional_decay_interval: u64,
    
    /// Drive update interval in seconds
    pub drive_update_interval: u64,
}

impl Default for SoulEngineConfig {
    fn default() -> Self {
        Self {
            default_soul: SoulConfig::default(),
            session_idle_timeout: 3600, // 1 hour
            max_sessions: 1000,
            persist_state: true,
            emotional_decay_interval: 60,
            drive_update_interval: 300,
        }
    }
}

/// The Soul Engine service
pub struct SoulEngineService {
    /// Runtime reference
    runtime: Option<Arc<dyn std::any::Any + Send + Sync>>,
    
    /// Configuration
    config: SoulEngineConfig,
    
    /// Active soul states keyed by (entity_id, room_id)
    states: Arc<RwLock<HashMap<(Uuid, Uuid), SoulState>>>,
    
    /// Soul configurations keyed by entity_id (allows per-entity customization)
    soul_configs: Arc<RwLock<HashMap<Uuid, SoulConfig>>>,
    
    /// Running flag
    running: bool,
    
    /// Last maintenance run
    last_maintenance: DateTime<Utc>,
}

impl SoulEngineService {
    /// Create a new soul engine
    pub fn new(config: SoulEngineConfig) -> Self {
        Self {
            runtime: None,
            config,
            states: Arc::new(RwLock::new(HashMap::new())),
            soul_configs: Arc::new(RwLock::new(HashMap::new())),
            running: false,
            last_maintenance: Utc::now(),
        }
    }
    
    /// Get or create a soul state for an entity/room
    pub fn get_or_create_state(&self, entity_id: Uuid, room_id: Uuid) -> SoulState {
        let key = (entity_id, room_id);
        
        // Check if state exists
        {
            let states = self.states.read().unwrap();
            if let Some(state) = states.get(&key) {
                if !state.is_stale(self.config.session_idle_timeout) {
                    return state.clone();
                }
            }
        }
        
        // Create new state
        let soul_config = {
            let configs = self.soul_configs.read().unwrap();
            configs.get(&entity_id).cloned().unwrap_or_else(|| self.config.default_soul.clone())
        };
        
        let state = SoulState::new(entity_id, room_id, &soul_config);
        
        // Store and return
        {
            let mut states = self.states.write().unwrap();
            states.insert(key, state.clone());
        }
        
        state
    }
    
    /// Update a soul state
    pub fn update_state(&self, state: SoulState) {
        let key = (state.entity_id, state.room_id);
        let mut states = self.states.write().unwrap();
        states.insert(key, state);
    }
    
    /// Process an incoming message and update soul state
    pub async fn process_message(
        &self,
        message: &Memory,
        current_state: &mut SoulState,
    ) -> Result<ProcessingResult> {
        let start_time = std::time::Instant::now();
        
        // Update activity timestamp
        current_state.last_activity = Utc::now();
        current_state.turn_count += 1;
        
        // 1. Add perception to working memory
        let perception = ThoughtFragment::new(
            &message.content.text,
            ThoughtType::Perception,
            ThoughtSource::External {
                entity_id: Some(message.entity_id),
                channel: message.content.source.clone().unwrap_or_else(|| "unknown".to_string()),
            },
        ).with_salience(0.9);
        
        current_state.working_memory = current_state.working_memory.push(perception);
        
        // 2. Run comprehension step
        let comprehension = ComprehensionStep::new();
        let runtime_any: Arc<dyn std::any::Any + Send + Sync> = self.runtime.clone().unwrap_or_else(|| Arc::new(()));
        let comp_result = comprehension.execute(current_state.working_memory.clone(), runtime_any.clone()).await;
        current_state.working_memory = comp_result.memory;
        
        // 3. Run emotional processing
        let emotional_step = EmotionalProcessingStep::new();
        let emotion_result = emotional_step.execute(current_state.working_memory.clone(), runtime_any.clone()).await;
        current_state.working_memory = emotion_result.memory;
        
        // Update emotional state based on detected emotions
        if emotion_result.success {
            let detected_emotion = if emotion_result.output.valence > 0.3 {
                DiscreteEmotion::Joy
            } else if emotion_result.output.valence < -0.3 {
                DiscreteEmotion::Sadness
            } else {
                DiscreteEmotion::Neutral
            };
            
            current_state.emotional_state.process_event(
                "message_received",
                detected_emotion,
                emotion_result.output.intensity,
            );
        }
        
        // 4. Evaluate mental processes
        current_state.process_orchestrator.evaluate(
            &current_state.working_memory,
            &current_state.emotional_state,
            &message.content.text,
        );
        
        // 5. Update drives based on interaction
        self.update_drives_from_message(current_state, message);
        
        // 6. Add reflection thought
        let reflection = ThoughtFragment::new(
            format!(
                "Current state: emotion={}, mode={}",
                current_state.emotional_state.describe(),
                current_state.process_orchestrator.active_process
                    .as_ref()
                    .map(|p| p.process.name.as_str())
                    .unwrap_or("default")
            ),
            ThoughtType::Reflection,
            ThoughtSource::Internal { process: "soul_engine".to_string() },
        ).with_salience(0.5).with_ttl(120);
        
        current_state.working_memory = current_state.working_memory.push(reflection);
        
        // Persist state
        self.update_state(current_state.clone());
        
        let elapsed = start_time.elapsed().as_millis() as u64;
        
        Ok(ProcessingResult {
            comprehension: comp_result.output,
            emotional_output: emotion_result.output,
            active_process: current_state.process_orchestrator.active_process
                .as_ref()
                .map(|p| p.process.name.clone()),
            behavioral_modifiers: current_state.process_orchestrator.current_modifiers(),
            processing_time_ms: elapsed,
        })
    }
    
    /// Update drives based on message content
    fn update_drives_from_message(&self, state: &mut SoulState, message: &Memory) {
        let text = message.content.text.to_lowercase();
        
        // Get soul config for drive definitions
        let soul_config = {
            let configs = self.soul_configs.read().unwrap();
            configs.get(&state.entity_id).cloned().unwrap_or_else(|| self.config.default_soul.clone())
        };
        
        // Update drives based on satisfiers/frustrators in message
        for drive_config in &soul_config.drives {
            // Check satisfiers
            for satisfier in &drive_config.satisfiers {
                if text.contains(&satisfier.to_lowercase()) {
                    if let Some(drive) = state.custom_data.get_mut(&format!("drive_{}", drive_config.name)) {
                        if let Some(intensity) = drive.as_f64() {
                            state.custom_data.insert(
                                format!("drive_{}", drive_config.name),
                                serde_json::json!((intensity - 0.1).max(0.0)),
                            );
                        }
                    } else {
                        state.custom_data.insert(
                            format!("drive_{}", drive_config.name),
                            serde_json::json!((drive_config.intensity - 0.1).max(0.0)),
                        );
                    }
                }
            }
            
            // Check frustrators
            for frustrator in &drive_config.frustrators {
                if text.contains(&frustrator.to_lowercase()) {
                    if let Some(drive) = state.custom_data.get_mut(&format!("drive_{}", drive_config.name)) {
                        if let Some(intensity) = drive.as_f64() {
                            state.custom_data.insert(
                                format!("drive_{}", drive_config.name),
                                serde_json::json!((intensity + 0.1).min(1.0)),
                            );
                        }
                    } else {
                        state.custom_data.insert(
                            format!("drive_{}", drive_config.name),
                            serde_json::json!((drive_config.intensity + 0.1).min(1.0)),
                        );
                    }
                }
            }
        }
    }
    
    /// Generate context for LLM prompts
    pub fn generate_context(&self, state: &SoulState) -> String {
        let soul_config = {
            let configs = self.soul_configs.read().unwrap();
            configs.get(&state.entity_id).cloned().unwrap_or_else(|| self.config.default_soul.clone())
        };
        
        let mut parts = Vec::new();
        
        // Soul identity context
        parts.push(soul_config.to_context());
        
        // Current emotional state
        parts.push(format!("\n# Current State\n{}", state.emotional_state.to_context()));
        
        // Mental process context
        let process_context = state.process_orchestrator.to_context();
        if !process_context.is_empty() {
            parts.push(format!("\n# Behavioral Mode\n{}", process_context));
        }
        
        // Working memory context
        let memory_context = state.working_memory.to_context_string();
        if !memory_context.is_empty() {
            parts.push(format!("\n# Working Memory\n{}", memory_context));
        }
        
        parts.join("\n")
    }
    
    /// Register a custom soul configuration for an entity
    pub fn register_soul_config(&self, entity_id: Uuid, config: SoulConfig) {
        let mut configs = self.soul_configs.write().unwrap();
        configs.insert(entity_id, config);
    }
    
    /// Run maintenance (cleanup stale sessions, decay emotions, etc.)
    pub async fn run_maintenance(&mut self) {
        let now = Utc::now();
        
        // Clean up stale sessions
        {
            let mut states = self.states.write().unwrap();
            states.retain(|_, state| !state.is_stale(self.config.session_idle_timeout));
            
            // Ensure we don't exceed max sessions
            while states.len() > self.config.max_sessions {
                // Remove oldest session
                if let Some(oldest_key) = states.iter()
                    .min_by_key(|(_, state)| state.last_activity)
                    .map(|(key, _)| *key)
                {
                    states.remove(&oldest_key);
                } else {
                    break;
                }
            }
        }
        
        // Decay emotional states
        if now.signed_duration_since(self.last_maintenance).num_seconds() as u64 
            >= self.config.emotional_decay_interval 
        {
            let mut states = self.states.write().unwrap();
            for state in states.values_mut() {
                state.emotional_state.decay();
            }
        }
        
        self.last_maintenance = now;
    }
    
    /// Get statistics about the engine
    pub fn stats(&self) -> EngineStats {
        let states = self.states.read().unwrap();
        EngineStats {
            active_sessions: states.len(),
            total_turns: states.values().map(|s| s.turn_count).sum(),
            avg_working_memory_size: if states.is_empty() {
                0.0
            } else {
                states.values().map(|s| s.working_memory.len()).sum::<usize>() as f64 / states.len() as f64
            },
        }
    }
}

/// Result of processing a message
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    /// Comprehension analysis
    pub comprehension: ComprehensionOutput,
    
    /// Emotional analysis
    pub emotional_output: EmotionalOutput,
    
    /// Active mental process
    pub active_process: Option<String>,
    
    /// Behavioral modifiers
    pub behavioral_modifiers: BehavioralModifiers,
    
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Engine statistics
#[derive(Debug, Clone)]
pub struct EngineStats {
    pub active_sessions: usize,
    pub total_turns: usize,
    pub avg_working_memory_size: f64,
}

#[async_trait]
impl Service for SoulEngineService {
    fn service_type(&self) -> &str {
        "soul_engine"
    }
    
    async fn initialize(&mut self, runtime: Arc<dyn std::any::Any + Send + Sync>) -> Result<()> {
        self.runtime = Some(runtime);
        tracing::info!("Soul Engine initialized with {} max sessions", self.config.max_sessions);
        Ok(())
    }
    
    async fn start(&mut self) -> Result<()> {
        self.running = true;
        tracing::info!("Soul Engine started");
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<()> {
        self.running = false;
        tracing::info!("Soul Engine stopped");
        Ok(())
    }
    
    fn is_running(&self) -> bool {
        self.running
    }
}

impl Default for SoulEngineService {
    fn default() -> Self {
        Self::new(SoulEngineConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_soul_state_creation() {
        let config = SoulConfig::default();
        let state = SoulState::new(Uuid::new_v4(), Uuid::new_v4(), &config);
        
        assert_eq!(state.turn_count, 0);
        assert!(state.working_memory.is_empty());
    }
    
    #[test]
    fn test_get_or_create_state() {
        let engine = SoulEngineService::default();
        let entity_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        
        let state1 = engine.get_or_create_state(entity_id, room_id);
        let state2 = engine.get_or_create_state(entity_id, room_id);
        
        // Should return the same session
        assert_eq!(state1.session_start, state2.session_start);
    }
    
    #[tokio::test]
    async fn test_process_message() {
        let mut engine = SoulEngineService::default();
        let entity_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        
        let mut state = engine.get_or_create_state(entity_id, room_id);
        
        let message = Memory {
            id: Uuid::new_v4(),
            entity_id,
            agent_id: Uuid::new_v4(),
            room_id,
            content: Content {
                text: "Hello, how are you today?".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };
        
        let result = engine.process_message(&message, &mut state).await;
        assert!(result.is_ok());
        assert_eq!(state.turn_count, 1);
        assert!(!state.working_memory.is_empty());
    }
}

