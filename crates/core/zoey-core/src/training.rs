//! Training and reinforcement learning utilities
//!
//! This module provides comprehensive training capabilities for both:
//! - **Local model training**: Fine-tuning local LLMs with conversation data
//! - **Reinforcement learning**: Feedback loops for cloud models (OpenAI, Anthropic, etc.)
//!
//! # Features
//!
//! - Thought process storage for pattern analysis
//! - Training dataset generation (JSONL, Alpaca, ShareGPT formats)
//! - Reinforcement learning from human feedback (RLHF)
//! - Quality scoring and evaluation
//! - Automated data collection and labeling
//! - Fine-tuning data export
//!
//! # Usage
//!
//! ```rust
//! use zoey_core::training::{TrainingCollector, TrainingConfig};
//! use std::sync::Arc;
//!
//! async fn collect_training_data() -> zoey_core::Result<()> {
//!     let config = TrainingConfig::default();
//!     let collector = TrainingCollector::new(config);
//!     
//!     // Store interaction for training
//!     collector.record_interaction(
//!         "User prompt",
//!         "Agent response",
//!         Some("Agent thought process".to_string()),
//!         0.9, // Quality score
//!     ).await?;
//!     
//!     // Export training dataset
//!     let dataset = collector.export_jsonl().await?;
//!     
//!     Ok(())
//! }
//! ```

use crate::types::{Content, Memory, MemoryMetadata, State, UUID};
use crate::{ZoeyError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{debug, error, info, instrument, warn};

/// Training data format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainingFormat {
    /// JSONL format (one JSON per line)
    Jsonl,
    /// Alpaca format (instruction, input, output)
    Alpaca,
    /// ShareGPT format (conversations)
    ShareGpt,
    /// OpenAI fine-tuning format
    OpenAi,
    /// Custom format
    Custom,
}

/// Training configuration
#[derive(Debug, Clone)]
pub struct TrainingConfig {
    /// Enable training data collection
    pub enabled: bool,

    /// Minimum quality score to include (0.0 - 1.0)
    pub min_quality_score: f32,

    /// Maximum training samples to keep in memory
    pub max_samples: usize,

    /// Auto-save interval in seconds (0 = disabled)
    pub auto_save_interval: u64,

    /// Output directory for training data
    pub output_dir: PathBuf,

    /// Default export format
    pub default_format: TrainingFormat,

    /// Include thought processes in training data
    pub include_thoughts: bool,

    /// Include negative examples (low quality interactions)
    pub include_negative_examples: bool,

    /// Negative example ratio (0.0 - 1.0)
    pub negative_example_ratio: f32,

    /// Enable reinforcement learning feedback
    pub enable_rlhf: bool,

    /// Auto-label data based on quality metrics
    pub auto_label: bool,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_quality_score: 0.6,
            max_samples: 10000,
            auto_save_interval: 300, // 5 minutes
            output_dir: PathBuf::from("./training_data"),
            default_format: TrainingFormat::Jsonl,
            include_thoughts: true,
            include_negative_examples: true,
            negative_example_ratio: 0.1,
            enable_rlhf: true,
            auto_label: true,
        }
    }
}

/// Training sample representing a single interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingSample {
    /// Unique ID
    pub id: UUID,

    /// User prompt/input
    pub prompt: String,

    /// Agent response/output
    pub response: String,

    /// Agent's thought process (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<String>,

    /// Context/state at time of interaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<HashMap<String, String>>,

    /// Quality score (0.0 - 1.0)
    pub quality_score: f32,

    /// Feedback score from user (-1.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_score: Option<f32>,

    /// Category/label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Timestamp
    pub timestamp: i64,

    /// Original message IDs for reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_ids: Option<MessagePair>,

    /// Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Pair of message IDs (user message and agent response)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePair {
    /// User message ID
    pub user_message_id: UUID,

    /// Agent response ID
    pub agent_message_id: UUID,
}

/// Alpaca format training sample
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlpacaSample {
    /// Instruction
    pub instruction: String,

    /// Input context
    pub input: String,

    /// Expected output
    pub output: String,
}

/// ShareGPT format conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareGptConversation {
    /// Conversation messages
    pub conversations: Vec<ShareGptMessage>,
}

/// ShareGPT message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareGptMessage {
    /// Role (user, assistant, system)
    pub from: String,

    /// Message content
    pub value: String,
}

/// OpenAI fine-tuning format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiFineTuning {
    /// Messages in conversation
    pub messages: Vec<OpenAiMessage>,
}

/// OpenAI message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    /// Role
    pub role: String,

    /// Content
    pub content: String,
}

/// Training dataset statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetStatistics {
    /// Total samples
    pub total_samples: usize,

    /// High quality samples (>0.8)
    pub high_quality_count: usize,

    /// Medium quality samples (0.6-0.8)
    pub medium_quality_count: usize,

    /// Low quality samples (<0.6)
    pub low_quality_count: usize,

    /// Samples with thoughts
    pub with_thoughts_count: usize,

    /// Samples with feedback
    pub with_feedback_count: usize,

    /// Average quality score
    pub avg_quality_score: f32,

    /// Average feedback score
    pub avg_feedback_score: f32,

    /// Categories distribution
    pub categories: HashMap<String, usize>,

    /// Tags distribution
    pub tags: HashMap<String, usize>,
}

/// Training data collector
pub struct TrainingCollector {
    /// Configuration
    config: TrainingConfig,

    /// Collected samples
    samples: Arc<RwLock<Vec<TrainingSample>>>,

    /// Last save timestamp
    last_save: Arc<RwLock<std::time::Instant>>,
}

impl TrainingCollector {
    /// Create a new training collector
    pub fn new(config: TrainingConfig) -> Self {
        Self {
            config,
            samples: Arc::new(RwLock::new(Vec::new())),
            last_save: Arc::new(RwLock::new(std::time::Instant::now())),
        }
    }

    /// Check whether RLHF feedback is enabled
    pub fn is_rlhf_enabled(&self) -> bool {
        self.config.enable_rlhf
    }

    /// Record a training interaction
    #[instrument(skip(self, prompt, response, thought), level = "debug")]
    pub async fn record_interaction(
        &self,
        prompt: impl Into<String>,
        response: impl Into<String>,
        thought: Option<String>,
        quality_score: f32,
    ) -> Result<UUID> {
        if !self.config.enabled {
            return Err(ZoeyError::Config(
                "Training collection is disabled".to_string(),
            ));
        }

        let prompt = prompt.into();
        let response = response.into();

        // Validate quality score
        if quality_score < self.config.min_quality_score {
            debug!(
                "Skipping interaction due to low quality score: {}",
                quality_score
            );
            return Err(ZoeyError::Validation(
                "Quality score below threshold".to_string(),
            ));
        }

        let sample = TrainingSample {
            id: uuid::Uuid::new_v4(),
            prompt: prompt.clone(),
            response: response.clone(),
            thought,
            context: None,
            quality_score,
            feedback_score: None,
            category: if self.config.auto_label {
                Some(auto_categorize(&prompt, &response))
            } else {
                None
            },
            tags: if self.config.auto_label {
                auto_generate_tags(&prompt, &response)
            } else {
                vec![]
            },
            timestamp: Utc::now().timestamp_millis(),
            message_ids: None,
            metadata: None,
        };

        let sample_id = sample.id;

        // Add to collection
        {
            let mut samples = self.samples.write().unwrap();
            samples.push(sample);

            // Enforce max samples limit
            if samples.len() > self.config.max_samples {
                warn!("Training samples exceeded limit, removing oldest");
                samples.remove(0);
            }
        }

        info!(
            "Recorded training sample: {} (quality: {})",
            sample_id, quality_score
        );

        // Auto-save if interval elapsed
        self.check_auto_save().await?;

        Ok(sample_id)
    }

    /// Store agent's thought process for learning and reflection
    #[instrument(
        skip(self, runtime_any, thought_text, original_message),
        level = "info"
    )]
    pub async fn store_thought(
        &self,
        runtime_any: Arc<dyn std::any::Any + Send + Sync>,
        thought_text: &str,
        original_message: &Memory,
        quality_score: f32,
    ) -> Result<UUID> {
        info!(
            "💭 Storing agent thought ({} chars, quality: {})",
            thought_text.len(),
            quality_score
        );

        // Try to get runtime reference
        let runtime_ref = crate::runtime_ref::downcast_runtime_ref(&runtime_any)
            .ok_or_else(|| ZoeyError::Runtime("Invalid runtime reference".to_string()))?;

        let runtime_arc = runtime_ref
            .try_upgrade()
            .ok_or_else(|| ZoeyError::Runtime("Runtime has been dropped".to_string()))?;

        let agent_runtime = runtime_arc.read().unwrap();
        let agent_id = agent_runtime.agent_id;

        // Create thought memory with rich metadata
        let thought_memory = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: agent_id,
            agent_id,
            room_id: original_message.room_id,
            content: Content {
                text: thought_text.to_string(),
                source: Some("internal_thought".to_string()),
                thought: Some(thought_text.to_string()),
                ..Default::default()
            },
            embedding: None,
            metadata: Some(MemoryMetadata {
                memory_type: Some("thought".to_string()),
                entity_name: Some(agent_runtime.character.name.clone()),
                data: {
                    let mut meta = HashMap::new();
                    meta.insert("purpose".to_string(), serde_json::json!("reflection"));
                    meta.insert(
                        "related_message".to_string(),
                        serde_json::json!(original_message.id.to_string()),
                    );
                    meta.insert(
                        "timestamp".to_string(),
                        serde_json::json!(Utc::now().timestamp_millis()),
                    );
                    meta.insert(
                        "quality_score".to_string(),
                        serde_json::json!(quality_score),
                    );
                    meta.insert(
                        "can_be_used_for".to_string(),
                        serde_json::json!([
                            "decision_pattern_analysis",
                            "response_improvement",
                            "self_reflection",
                            "training_data",
                            "rlhf"
                        ]),
                    );
                    meta
                },
            }),
            created_at: Utc::now().timestamp_millis(),
            unique: Some(false),
            similarity: None,
        };

        let thought_id = thought_memory.id;

        // Store in database
        let adapter_opt = agent_runtime.adapter.read().unwrap().clone();
        if let Some(adapter) = adapter_opt.as_ref() {
            match adapter.create_memory(&thought_memory, "thoughts").await {
                Ok(id) => {
                    info!("✓ Thought stored with ID: {}", id);
                    info!("💾 Available for: pattern analysis, RLHF, training");
                }
                Err(e) => {
                    error!("Failed to store thought: {}", e);
                    return Err(e);
                }
            }
        }

        // Also add to training collector if enabled
        if self.config.enabled && quality_score >= self.config.min_quality_score {
            self.record_interaction(
                original_message.content.text.clone(),
                thought_text,
                Some(thought_text.to_string()),
                quality_score,
            )
            .await?;
        }

        Ok(thought_id)
    }

    /// Add user feedback to a training sample (for RLHF)
    #[instrument(skip(self), level = "info")]
    pub async fn add_feedback(
        &self,
        sample_id: UUID,
        feedback_score: f32,
        feedback_text: Option<String>,
    ) -> Result<()> {
        if !self.config.enable_rlhf {
            return Err(ZoeyError::Config("RLHF is disabled".to_string()));
        }

        // Validate feedback score
        if !(-1.0..=1.0).contains(&feedback_score) {
            return Err(ZoeyError::Validation(
                "Feedback score must be between -1.0 and 1.0".to_string(),
            ));
        }

        let mut samples = self.samples.write().unwrap();

        if let Some(sample) = samples.iter_mut().find(|s| s.id == sample_id) {
            sample.feedback_score = Some(feedback_score);

            // Add feedback text to metadata
            if let Some(text) = feedback_text {
                let mut metadata = sample.metadata.take().unwrap_or_default();
                metadata.insert("feedback_text".to_string(), serde_json::json!(text));
                metadata.insert(
                    "feedback_timestamp".to_string(),
                    serde_json::json!(Utc::now().timestamp_millis()),
                );
                sample.metadata = Some(metadata);
            }

            info!(
                "✓ Added feedback to sample {} (score: {})",
                sample_id, feedback_score
            );
            Ok(())
        } else {
            Err(ZoeyError::NotFound(format!(
                "Training sample {} not found",
                sample_id
            )))
        }
    }

    /// Add evaluator review to a training sample (always-on, independent of RLHF)
    #[instrument(skip(self), level = "info")]
    pub async fn add_review(
        &self,
        sample_id: UUID,
        review_score: f32,
        review_text: Option<String>,
    ) -> Result<()> {
        if !(0.0..=1.0).contains(&review_score) {
            return Err(ZoeyError::Validation(
                "Review score must be between 0.0 and 1.0".to_string(),
            ));
        }
        let mut samples = self.samples.write().unwrap();
        if let Some(sample) = samples.iter_mut().find(|s| s.id == sample_id) {
            let mut metadata = sample.metadata.take().unwrap_or_default();
            metadata.insert("review_score".to_string(), serde_json::json!(review_score));
            if let Some(text) = review_text {
                metadata.insert("review_text".to_string(), serde_json::json!(text));
            }
            metadata.insert(
                "review_timestamp".to_string(),
                serde_json::json!(Utc::now().timestamp_millis()),
            );
            sample.metadata = Some(metadata);
            info!(
                "✓ Added evaluator review to sample {} (score: {})",
                sample_id, review_score
            );
            Ok(())
        } else {
            Err(ZoeyError::NotFound(format!(
                "Training sample {} not found",
                sample_id
            )))
        }
    }

    /// Record a complete conversation turn (prompt, response, thought, context)
    #[instrument(skip(self, message, response, thought, state), level = "debug")]
    pub async fn record_conversation_turn(
        &self,
        message: &Memory,
        response: &Memory,
        thought: Option<String>,
        state: &State,
    ) -> Result<UUID> {
        if !self.config.enabled {
            return Err(ZoeyError::Config(
                "Training collection is disabled".to_string(),
            ));
        }

        // Calculate quality score based on multiple factors
        let quality_score = calculate_quality_score(message, response, &thought, state);

        if quality_score < self.config.min_quality_score {
            debug!(
                "Skipping low quality interaction (score: {})",
                quality_score
            );
            return Err(ZoeyError::Validation(
                "Quality score below threshold".to_string(),
            ));
        }

        // Extract context from state
        let context: HashMap<String, String> = state.values.clone();

        let sample = TrainingSample {
            id: uuid::Uuid::new_v4(),
            prompt: message.content.text.clone(),
            response: response.content.text.clone(),
            thought: if self.config.include_thoughts {
                thought
            } else {
                None
            },
            context: Some(context),
            quality_score,
            feedback_score: None,
            category: if self.config.auto_label {
                Some(auto_categorize(
                    &message.content.text,
                    &response.content.text,
                ))
            } else {
                None
            },
            tags: if self.config.auto_label {
                auto_generate_tags(&message.content.text, &response.content.text)
            } else {
                vec![]
            },
            timestamp: Utc::now().timestamp_millis(),
            message_ids: Some(MessagePair {
                user_message_id: message.id,
                agent_message_id: response.id,
            }),
            metadata: None,
        };

        let sample_id = sample.id;

        {
            let mut samples = self.samples.write().unwrap();
            samples.push(sample);

            if samples.len() > self.config.max_samples {
                samples.remove(0);
            }
        }

        info!(
            "Recorded conversation turn: {} (quality: {})",
            sample_id, quality_score
        );

        self.check_auto_save().await?;

        Ok(sample_id)
    }

    /// Get all training samples
    pub fn get_samples(&self) -> Vec<TrainingSample> {
        self.samples.read().unwrap().clone()
    }

    /// Get samples filtered by quality
    pub fn get_samples_by_quality(&self, min_score: f32, max_score: f32) -> Vec<TrainingSample> {
        self.samples
            .read()
            .unwrap()
            .iter()
            .filter(|s| s.quality_score >= min_score && s.quality_score <= max_score)
            .cloned()
            .collect()
    }

    /// Get samples with feedback (for RLHF)
    pub fn get_samples_with_feedback(&self) -> Vec<TrainingSample> {
        self.samples
            .read()
            .unwrap()
            .iter()
            .filter(|s| s.feedback_score.is_some())
            .cloned()
            .collect()
    }

    /// Get dataset statistics
    pub fn get_statistics(&self) -> DatasetStatistics {
        let samples = self.samples.read().unwrap();

        let total_samples = samples.len();
        let high_quality_count = samples.iter().filter(|s| s.quality_score > 0.8).count();
        let medium_quality_count = samples
            .iter()
            .filter(|s| s.quality_score >= 0.6 && s.quality_score <= 0.8)
            .count();
        let low_quality_count = samples.iter().filter(|s| s.quality_score < 0.6).count();
        let with_thoughts_count = samples.iter().filter(|s| s.thought.is_some()).count();
        let with_feedback_count = samples
            .iter()
            .filter(|s| s.feedback_score.is_some())
            .count();

        let avg_quality_score = if total_samples > 0 {
            samples.iter().map(|s| s.quality_score).sum::<f32>() / total_samples as f32
        } else {
            0.0
        };

        let feedback_samples: Vec<_> = samples.iter().filter_map(|s| s.feedback_score).collect();
        let avg_feedback_score = if !feedback_samples.is_empty() {
            feedback_samples.iter().sum::<f32>() / feedback_samples.len() as f32
        } else {
            // Fallback to review_score average when RLHF disabled
            let review_scores: Vec<f32> = samples
                .iter()
                .filter_map(|s| {
                    s.metadata
                        .as_ref()
                        .and_then(|m| m.get("review_score"))
                        .and_then(|v| v.as_f64())
                        .map(|f| f as f32)
                })
                .collect();
            if !review_scores.is_empty() {
                review_scores.iter().sum::<f32>() / review_scores.len() as f32
            } else {
                0.0
            }
        };

        let mut categories: HashMap<String, usize> = HashMap::new();
        for sample in samples.iter() {
            if let Some(cat) = &sample.category {
                *categories.entry(cat.clone()).or_insert(0) += 1;
            }
        }

        let mut tags: HashMap<String, usize> = HashMap::new();
        for sample in samples.iter() {
            for tag in &sample.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        DatasetStatistics {
            total_samples,
            high_quality_count,
            medium_quality_count,
            low_quality_count,
            with_thoughts_count,
            with_feedback_count,
            avg_quality_score,
            avg_feedback_score,
            categories,
            tags,
        }
    }

    /// Export training data as JSONL
    #[instrument(skip(self), level = "info")]
    pub async fn export_jsonl(&self) -> Result<String> {
        let samples = self.samples.read().unwrap();

        let jsonl = samples
            .iter()
            .map(|sample| {
                let mut s = sample.clone();
                // Ensure review_score remains in metadata for downstream use
                let _ = s.metadata.as_ref().and_then(|m| m.get("review_score"));
                serde_json::to_string(&s).unwrap()
            })
            .collect::<Vec<_>>()
            .join("\n");

        info!(
            "Exported {} samples as JSONL ({} bytes)",
            samples.len(),
            jsonl.len()
        );
        Ok(jsonl)
    }

    /// Export as Alpaca format
    #[instrument(skip(self), level = "info")]
    pub async fn export_alpaca(&self) -> Result<String> {
        let samples = self.samples.read().unwrap();

        let alpaca_samples: Vec<AlpacaSample> = samples
            .iter()
            .map(|sample| AlpacaSample {
                instruction: extract_instruction(&sample.prompt),
                input: sample.prompt.clone(),
                output: sample.response.clone(),
            })
            .collect();

        let json = serde_json::to_string_pretty(&alpaca_samples)?;
        info!("Exported {} samples as Alpaca format", samples.len());
        Ok(json)
    }

    /// Export as ShareGPT format
    #[instrument(skip(self), level = "info")]
    pub async fn export_sharegpt(&self) -> Result<String> {
        let samples = self.samples.read().unwrap();

        let conversations: Vec<ShareGptConversation> = samples
            .iter()
            .map(|sample| ShareGptConversation {
                conversations: vec![
                    ShareGptMessage {
                        from: "human".to_string(),
                        value: sample.prompt.clone(),
                    },
                    ShareGptMessage {
                        from: "gpt".to_string(),
                        value: sample.response.clone(),
                    },
                ],
            })
            .collect();

        let json = serde_json::to_string_pretty(&conversations)?;
        info!("Exported {} samples as ShareGPT format", samples.len());
        Ok(json)
    }

    /// Export as OpenAI fine-tuning format
    #[instrument(skip(self), level = "info")]
    pub async fn export_openai(&self) -> Result<String> {
        let samples = self.samples.read().unwrap();

        let training_data: Vec<OpenAiFineTuning> = samples
            .iter()
            .map(|sample| OpenAiFineTuning {
                messages: vec![
                    OpenAiMessage {
                        role: "user".to_string(),
                        content: sample.prompt.clone(),
                    },
                    OpenAiMessage {
                        role: "assistant".to_string(),
                        content: sample.response.clone(),
                    },
                ],
            })
            .collect();

        // OpenAI expects one JSON object per line
        let jsonl = training_data
            .iter()
            .map(|item| serde_json::to_string(item).unwrap())
            .collect::<Vec<_>>()
            .join("\n");

        info!(
            "Exported {} samples as OpenAI fine-tuning format",
            samples.len()
        );
        Ok(jsonl)
    }

    /// Save training data to file
    #[instrument(skip(self), level = "info")]
    pub async fn save_to_file(&self, format: TrainingFormat) -> Result<PathBuf> {
        // Create output directory if it doesn't exist
        tokio::fs::create_dir_all(&self.config.output_dir).await?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let (data, extension) = match format {
            TrainingFormat::Jsonl => (self.export_jsonl().await?, "jsonl"),
            TrainingFormat::Alpaca => (self.export_alpaca().await?, "json"),
            TrainingFormat::ShareGpt => (self.export_sharegpt().await?, "json"),
            TrainingFormat::OpenAi => (self.export_openai().await?, "jsonl"),
            TrainingFormat::Custom => (self.export_jsonl().await?, "jsonl"),
        };

        let filename = format!(
            "training_data_{}_{}.{}",
            format!("{:?}", format).to_lowercase(),
            timestamp,
            extension
        );
        let filepath = self.config.output_dir.join(filename);

        tokio::fs::write(&filepath, data).await?;

        info!("✓ Saved training data to: {:?}", filepath);
        Ok(filepath)
    }

    /// Check if auto-save should trigger
    async fn check_auto_save(&self) -> Result<()> {
        if self.config.auto_save_interval == 0 {
            return Ok(());
        }

        let should_save = {
            let last_save = self.last_save.read().unwrap();
            last_save.elapsed().as_secs() >= self.config.auto_save_interval
        };

        if should_save {
            info!("Auto-save triggered");
            self.save_to_file(self.config.default_format).await?;

            let mut last_save = self.last_save.write().unwrap();
            *last_save = std::time::Instant::now();
        }

        Ok(())
    }

    /// Remove a specific training sample by ID
    #[instrument(skip(self), level = "info")]
    pub fn remove_sample(&self, sample_id: UUID) -> Result<()> {
        let mut samples = self.samples.write().unwrap();
        let initial_len = samples.len();
        samples.retain(|s| s.id != sample_id);
        
        if samples.len() < initial_len {
            info!("Removed training sample: {}", sample_id);
            Ok(())
        } else {
            Err(ZoeyError::NotFound(format!(
                "Training sample {} not found",
                sample_id
            )))
        }
    }

    /// Get a specific sample by ID
    pub fn get_sample(&self, sample_id: UUID) -> Option<TrainingSample> {
        self.samples
            .read()
            .unwrap()
            .iter()
            .find(|s| s.id == sample_id)
            .cloned()
    }

    /// Clear all training samples
    pub fn clear(&self) {
        let mut samples = self.samples.write().unwrap();
        samples.clear();
        info!("Cleared all training samples");
    }

    /// Get sample count
    pub fn count(&self) -> usize {
        self.samples.read().unwrap().len()
    }
}

/// Calculate quality score for a training sample
fn calculate_quality_score(
    _message: &Memory,
    response: &Memory,
    thought: &Option<String>,
    state: &State,
) -> f32 {
    let mut score: f32 = 0.5; // Base score

    // Response length factor (prefer substantial responses)
    let response_len = response.content.text.len();
    if response_len > 20 && response_len < 1000 {
        score += 0.1;
    } else if response_len >= 1000 {
        score += 0.05; // Very long responses might be verbose
    }

    // Thought process factor (indicates deliberation)
    if thought.is_some() {
        score += 0.15;
    }

    // Context richness (more state = better context)
    if state.values.len() > 5 {
        score += 0.1;
    }

    // Response coherence (simple check for complete sentences)
    if response.content.text.ends_with('.')
        || response.content.text.ends_with('!')
        || response.content.text.ends_with('?')
    {
        score += 0.05;
    }

    // Avoid trivial responses
    if response.content.text.split_whitespace().count() > 3 {
        score += 0.1;
    }

    // Cap at 1.0
    score.min(1.0)
}

/// Auto-categorize a training sample based on content
fn auto_categorize(prompt: &str, response: &str) -> String {
    let prompt_lower = prompt.to_lowercase();
    let response_lower = response.to_lowercase();

    // Categorize based on content patterns
    if prompt_lower.contains("how")
        && (prompt_lower.contains("work") || prompt_lower.contains("do"))
    {
        "how_to".to_string()
    } else if prompt_lower.contains("what") || prompt_lower.contains("explain") {
        "explanation".to_string()
    } else if prompt_lower.contains("why") {
        "reasoning".to_string()
    } else if prompt_lower.contains("?") {
        "question_answer".to_string()
    } else if response_lower.contains("error") || response_lower.contains("sorry") {
        "error_handling".to_string()
    } else if prompt_lower.contains("thank") || response_lower.contains("welcome") {
        "social".to_string()
    } else if prompt_lower.contains("help") {
        "help_request".to_string()
    } else {
        "general".to_string()
    }
}

/// Auto-generate tags for a training sample
fn auto_generate_tags(prompt: &str, response: &str) -> Vec<String> {
    let mut tags = Vec::new();

    let prompt_lower = prompt.to_lowercase();
    let response_lower = response.to_lowercase();

    // Content-based tags
    if prompt_lower.contains("code") || response_lower.contains("```") {
        tags.push("code".to_string());
    }

    if prompt_lower.contains("data") || prompt_lower.contains("information") {
        tags.push("data".to_string());
    }

    if prompt_lower.len() > 200 {
        tags.push("long_prompt".to_string());
    }

    if response_lower.len() > 500 {
        tags.push("detailed_response".to_string());
    }

    if prompt_lower.contains("?") {
        tags.push("question".to_string());
    }

    if response_lower.contains("step") || response_lower.contains("first") {
        tags.push("instructional".to_string());
    }

    tags
}

/// Extract instruction from prompt (for Alpaca format)
fn extract_instruction(prompt: &str) -> String {
    // Simple heuristic: first sentence or first 100 chars
    let first_sentence = prompt.split('.').next().unwrap_or(prompt);

    if first_sentence.len() > 100 {
        format!("{}...", &first_sentence[..100])
    } else {
        first_sentence.to_string()
    }
}

/// Reinforcement learning feedback manager
pub struct RLHFManager {
    collector: Arc<TrainingCollector>,
}

impl RLHFManager {
    /// Create a new RLHF manager
    pub fn new(collector: Arc<TrainingCollector>) -> Self {
        Self { collector }
    }

    /// Record positive feedback
    pub async fn record_positive(&self, sample_id: UUID, reason: Option<String>) -> Result<()> {
        self.collector.add_feedback(sample_id, 1.0, reason).await
    }

    /// Record negative feedback
    pub async fn record_negative(&self, sample_id: UUID, reason: Option<String>) -> Result<()> {
        self.collector.add_feedback(sample_id, -1.0, reason).await
    }

    /// Record neutral feedback
    pub async fn record_neutral(&self, sample_id: UUID) -> Result<()> {
        self.collector.add_feedback(sample_id, 0.0, None).await
    }

    /// Get samples ready for reinforcement learning
    pub fn get_rlhf_dataset(&self) -> Vec<(TrainingSample, TrainingSample)> {
        let samples = self.collector.get_samples_with_feedback();

        // Create pairs of positive and negative examples
        let mut pairs = Vec::new();
        let positive: Vec<_> = samples
            .iter()
            .filter(|s| s.feedback_score.unwrap_or(0.0) > 0.5)
            .cloned()
            .collect();

        let negative: Vec<_> = samples
            .iter()
            .filter(|s| s.feedback_score.unwrap_or(0.0) < -0.5)
            .cloned()
            .collect();

        // Pair each positive with a negative for comparison
        for (pos, neg) in positive.iter().zip(negative.iter()) {
            pairs.push((pos.clone(), neg.clone()));
        }

        pairs
    }

    /// Generate reward scores for a batch of samples
    pub fn calculate_rewards(&self, sample_ids: &[UUID]) -> HashMap<UUID, f32> {
        let samples = self.collector.get_samples();
        let mut rewards = HashMap::new();

        for id in sample_ids {
            if let Some(sample) = samples.iter().find(|s| s.id == *id) {
                // Calculate reward based on quality and feedback
                let quality_reward = sample.quality_score;
                let feedback_reward = sample.feedback_score.unwrap_or(0.0);

                // Combined reward (weighted average)
                let total_reward = (quality_reward * 0.4) + (feedback_reward * 0.6);

                rewards.insert(*id, total_reward);
            }
        }

        rewards
    }
}

/// Training dataset builder for fine-tuning
pub struct DatasetBuilder {
    samples: Vec<TrainingSample>,
}

impl DatasetBuilder {
    /// Create a new dataset builder
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    /// Add samples from collector
    pub fn add_from_collector(mut self, collector: &TrainingCollector) -> Self {
        self.samples.extend(collector.get_samples());
        self
    }

    /// Filter by quality score
    pub fn filter_by_quality(mut self, min_score: f32) -> Self {
        self.samples.retain(|s| s.quality_score >= min_score);
        self
    }

    /// Filter by category
    pub fn filter_by_category(mut self, category: &str) -> Self {
        self.samples
            .retain(|s| s.category.as_ref().map(|c| c == category).unwrap_or(false));
        self
    }

    /// Filter by tags
    pub fn filter_by_tags(mut self, tags: &[String]) -> Self {
        self.samples
            .retain(|s| tags.iter().any(|tag| s.tags.contains(tag)));
        self
    }

    /// Include only samples with thoughts
    pub fn only_with_thoughts(mut self) -> Self {
        self.samples.retain(|s| s.thought.is_some());
        self
    }

    /// Include only samples with feedback
    pub fn only_with_feedback(mut self) -> Self {
        self.samples.retain(|s| s.feedback_score.is_some());
        self
    }

    /// Limit to top N samples by quality
    pub fn top_n(mut self, n: usize) -> Self {
        self.samples.sort_by(|a, b| {
            b.quality_score
                .partial_cmp(&a.quality_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.samples.truncate(n);
        self
    }

    /// Balance positive and negative examples
    pub fn balance_examples(mut self, positive_ratio: f32) -> Self {
        let positive: Vec<_> = self
            .samples
            .iter()
            .filter(|s| s.quality_score > 0.7)
            .cloned()
            .collect();

        let negative: Vec<_> = self
            .samples
            .iter()
            .filter(|s| s.quality_score < 0.5)
            .cloned()
            .collect();

        let target_positive = (positive.len() as f32 * positive_ratio) as usize;
        let target_negative = positive.len() - target_positive;

        self.samples.clear();
        self.samples
            .extend(positive.into_iter().take(target_positive));
        self.samples
            .extend(negative.into_iter().take(target_negative));

        self
    }

    /// Build final dataset
    pub fn build(self) -> Vec<TrainingSample> {
        self.samples
    }

    /// Get count
    pub fn count(&self) -> usize {
        self.samples.len()
    }
}

impl Default for DatasetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create a training collector with runtime
pub fn create_training_collector(config: TrainingConfig) -> Arc<TrainingCollector> {
    Arc::new(TrainingCollector::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_training_config() {
        let config = TrainingConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_quality_score, 0.6);
        assert_eq!(config.max_samples, 10000);
    }

    #[tokio::test]
    async fn test_record_interaction() {
        let config = TrainingConfig::default();
        let collector = TrainingCollector::new(config);

        let result = collector
            .record_interaction(
                "Hello, how are you?",
                "I'm doing well, thank you!",
                Some("User is greeting me".to_string()),
                0.8,
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(collector.count(), 1);
    }

    #[tokio::test]
    async fn test_low_quality_rejected() {
        let config = TrainingConfig::default();
        let collector = TrainingCollector::new(config);

        let result = collector
            .record_interaction(
                "test", "ok", None, 0.3, // Below default threshold
            )
            .await;

        assert!(result.is_err());
        assert_eq!(collector.count(), 0);
    }

    #[tokio::test]
    async fn test_feedback() {
        let config = TrainingConfig::default();
        let collector = TrainingCollector::new(config);

        let sample_id = collector
            .record_interaction(
                "What is Rust?",
                "Rust is a systems programming language",
                None,
                0.9,
            )
            .await
            .unwrap();

        collector
            .add_feedback(sample_id, 1.0, Some("Great answer!".to_string()))
            .await
            .unwrap();

        let samples = collector.get_samples_with_feedback();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].feedback_score, Some(1.0));
    }

    #[test]
    fn test_auto_categorize() {
        assert_eq!(
            auto_categorize("How does this work?", "It works by..."),
            "how_to"
        );
        assert_eq!(auto_categorize("What is AI?", "AI is..."), "explanation");
        assert_eq!(auto_categorize("Why is that?", "Because..."), "reasoning");
        assert_eq!(auto_categorize("Help me", "Sure!"), "help_request");
    }

    #[test]
    fn test_auto_generate_tags() {
        let tags = auto_generate_tags("Can you write some code?", "```python\nprint('hello')\n```");
        assert!(tags.contains(&"code".to_string()));
        assert!(tags.contains(&"question".to_string()));
    }

    #[tokio::test]
    async fn test_export_jsonl() {
        let config = TrainingConfig::default();
        let collector = TrainingCollector::new(config);

        collector
            .record_interaction("Test", "Response", None, 0.8)
            .await
            .unwrap();

        let jsonl = collector.export_jsonl().await.unwrap();
        assert!(jsonl.contains("Test"));
        assert!(jsonl.contains("Response"));
    }

    #[tokio::test]
    async fn test_statistics() {
        let config = TrainingConfig {
            min_quality_score: 0.5, // Lower threshold for this test
            ..Default::default()
        };
        let collector = TrainingCollector::new(config);

        collector
            .record_interaction("Q1", "A1", Some("T1".to_string()), 0.9)
            .await
            .unwrap();
        collector
            .record_interaction("Q2", "A2", None, 0.7)
            .await
            .unwrap();
        collector
            .record_interaction("Q3", "A3", Some("T3".to_string()), 0.5)
            .await
            .unwrap();

        let stats = collector.get_statistics();
        assert_eq!(stats.total_samples, 3);
        assert_eq!(stats.high_quality_count, 1); // >0.8
        assert_eq!(stats.with_thoughts_count, 2);
    }

    #[test]
    fn test_dataset_builder() {
        let config = TrainingConfig::default();
        let collector = TrainingCollector::new(config);

        let dataset = DatasetBuilder::new()
            .add_from_collector(&collector)
            .filter_by_quality(0.7)
            .top_n(10)
            .build();

        assert!(dataset.len() <= 10);
    }

    #[test]
    fn test_quality_score_calculation() {
        let message = Memory {
            id: Uuid::new_v4(),
            entity_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: Content {
                text: "Hello".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: 12345,
            unique: None,
            similarity: None,
        };

        let response = Memory {
            id: Uuid::new_v4(),
            entity_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: Content {
                text: "Hello! How can I help you today?".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: 12346,
            unique: None,
            similarity: None,
        };

        let thought = Some("User is greeting me".to_string());
        let state = State::new();

        let score = calculate_quality_score(&message, &response, &thought, &state);
        assert!(score >= 0.5);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_rlhf_manager() {
        let config = TrainingConfig::default();
        let collector = Arc::new(TrainingCollector::new(config));
        let rlhf = RLHFManager::new(collector);

        // Test compiles
        let _ = rlhf;
    }

    #[tokio::test]
    async fn test_export_formats() {
        let config = TrainingConfig::default();
        let collector = TrainingCollector::new(config);

        collector
            .record_interaction("Test Q", "Test A", None, 0.8)
            .await
            .unwrap();

        // Test all export formats
        let jsonl = collector.export_jsonl().await;
        assert!(jsonl.is_ok());

        let alpaca = collector.export_alpaca().await;
        assert!(alpaca.is_ok());

        let sharegpt = collector.export_sharegpt().await;
        assert!(sharegpt.is_ok());

        let openai = collector.export_openai().await;
        assert!(openai.is_ok());
    }

    #[tokio::test]
    async fn test_add_review_non_rlhf() {
        let config = TrainingConfig {
            enable_rlhf: false,
            ..Default::default()
        };
        let collector = TrainingCollector::new(config);

        let sample_id = collector
            .record_interaction("Prompt X", "Response Y", None, 0.8)
            .await
            .unwrap();

        collector
            .add_review(sample_id, 0.9, Some("Good coherence".to_string()))
            .await
            .unwrap();

        let samples = collector.get_samples_by_quality(0.0, 1.0);
        let sample = samples.into_iter().find(|s| s.id == sample_id).unwrap();
        let meta = sample.metadata.unwrap();
        assert_eq!(
            meta.get("review_score").and_then(|v| v.as_f64()).unwrap() as f32,
            0.9
        );
        assert_eq!(
            meta.get("review_text").and_then(|v| v.as_str()).unwrap(),
            "Good coherence"
        );

        let stats = collector.get_statistics();
        assert!(stats.avg_feedback_score > 0.0); // falls back to review_score when RLHF disabled
    }

    #[tokio::test]
    async fn test_export_jsonl_includes_review() {
        let config = TrainingConfig {
            enable_rlhf: false,
            ..Default::default()
        };
        let collector = TrainingCollector::new(config);
        let sample_id = collector
            .record_interaction("A", "B", None, 0.8)
            .await
            .unwrap();
        collector
            .add_review(sample_id, 0.6, Some("Okay".to_string()))
            .await
            .unwrap();
        let jsonl = collector.export_jsonl().await.unwrap();
        assert!(jsonl.contains("\"review_score\""));
        assert!(jsonl.contains("\"review_text\""));
    }

    #[tokio::test]
    async fn print_jsonl_preview() {
        let config = TrainingConfig {
            enable_rlhf: true,
            ..Default::default()
        };
        let collector = TrainingCollector::new(config);

        let s1 = collector
            .record_interaction("How are you?", "I'm well.", None, 0.82)
            .await
            .unwrap();
        collector
            .add_review(s1, 0.9, Some("Coherent".to_string()))
            .await
            .unwrap();

        let s2 = collector
            .record_interaction("Tell a joke", "Why did the dev cross the road?", None, 0.78)
            .await
            .unwrap();
        collector
            .add_feedback(s2, 1.0, Some("Funny".to_string()))
            .await
            .unwrap();

        let jsonl = collector.export_jsonl().await.unwrap();
        println!("{}", jsonl);
    }

    #[tokio::test]
    async fn e2e_conversation_logging_preview() {
        // Simulate a conversation and show logs of retrieval/summaries and behavior improvements
        let config = TrainingConfig {
            enable_rlhf: false,
            ..Default::default()
        };
        let collector = TrainingCollector::new(config);

        // Build synthetic state with provider-like values
        let mut state = State::new();
        state.set_value("UI_TONE", "friendly".to_string());
        state.set_value("UI_VERBOSITY", "concise".to_string());
        state.set_value(
            "CONTEXT_LAST_THOUGHT",
            "User asked about project status; earlier we shipped v1".to_string(),
        );
        state.set_value(
            "DIALOGUE_SUMMARY",
            "Discussed roadmap, blockers, and timelines".to_string(),
        );

        // User message and assistant response
        let room_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let message = Memory {
            id: Uuid::new_v4(),
            entity_id: user_id,
            agent_id,
            room_id,
            content: Content {
                text: "What is the current project status?".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: Some(false),
            similarity: None,
        };
        let response = Memory {
            id: Uuid::new_v4(),
            entity_id: agent_id,
            agent_id,
            room_id,
            content: Content {
                text: "We completed the core milestones and are preparing the release.".to_string(),
                ..Default::default()
            },
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: Some(false),
            similarity: None,
        };

        // Log provider/state values indicating improved retrieval and summaries
        println!(
            "[STATE] UI_TONE={}",
            state.get_value("UI_TONE").cloned().unwrap_or_default()
        );
        println!(
            "[STATE] UI_VERBOSITY={}",
            state.get_value("UI_VERBOSITY").cloned().unwrap_or_default()
        );
        println!(
            "[STATE] CONTEXT_LAST_THOUGHT={}",
            state
                .get_value("CONTEXT_LAST_THOUGHT")
                .cloned()
                .unwrap_or_default()
        );
        println!(
            "[STATE] DIALOGUE_SUMMARY={}",
            state
                .get_value("DIALOGUE_SUMMARY")
                .cloned()
                .unwrap_or_default()
        );

        // Record the conversation turn (quality scoring considers thought/state)
        let sample_id = collector
            .record_conversation_turn(&message, &response, None, &state)
            .await
            .unwrap();

        // Simulate evaluator signals improving behavior
        collector
            .add_review(
                sample_id,
                0.88,
                Some("Direct, concise, and helpful".to_string()),
            )
            .await
            .unwrap();

        // RLHF is disabled here; still show potential human feedback path
        // Print dataset preview with metadata
        let jsonl = collector.export_jsonl().await.unwrap();
        println!("[DATASET]\n{}", jsonl);

        // Show a summary metric snapshot
        let stats = collector.get_statistics();
        println!(
            "[STATS] total={}, avg_quality={:.2}, avg_feedback_or_review={:.2}",
            stats.total_samples, stats.avg_quality_score, stats.avg_feedback_score
        );
    }
}
