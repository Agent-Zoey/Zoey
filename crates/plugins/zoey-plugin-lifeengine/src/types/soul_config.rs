//! Soul Configuration - Personality, Drive, and Ego Definitions
//!
//! The soul configuration defines the core identity of an AI agent:
//! - Personality traits (Big Five model)
//! - Drives and motivations
//! - Ego and self-concept
//! - Values and beliefs

use super::{CoreAffect, DiscreteEmotion};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Big Five personality traits (OCEAN model)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PersonalityTraits {
    /// Openness to experience (0.0 to 1.0)
    /// High: Creative, curious, open-minded
    /// Low: Practical, conventional, prefer routine
    pub openness: f32,
    
    /// Conscientiousness (0.0 to 1.0)
    /// High: Organized, dependable, self-disciplined
    /// Low: Flexible, spontaneous, may be careless
    pub conscientiousness: f32,
    
    /// Extraversion (0.0 to 1.0)
    /// High: Outgoing, energetic, talkative
    /// Low: Reserved, independent, prefer solitude
    pub extraversion: f32,
    
    /// Agreeableness (0.0 to 1.0)
    /// High: Cooperative, trusting, helpful
    /// Low: Competitive, skeptical, challenging
    pub agreeableness: f32,
    
    /// Neuroticism (0.0 to 1.0)
    /// High: Sensitive, prone to stress, emotional
    /// Low: Stable, calm, resilient
    pub neuroticism: f32,
}

impl PersonalityTraits {
    /// Create balanced personality
    pub fn balanced() -> Self {
        Self {
            openness: 0.5,
            conscientiousness: 0.5,
            extraversion: 0.5,
            agreeableness: 0.5,
            neuroticism: 0.5,
        }
    }
    
    /// Create a curious, creative personality
    pub fn creative() -> Self {
        Self {
            openness: 0.85,
            conscientiousness: 0.4,
            extraversion: 0.6,
            agreeableness: 0.7,
            neuroticism: 0.3,
        }
    }
    
    /// Create a warm, supportive personality
    pub fn supportive() -> Self {
        Self {
            openness: 0.6,
            conscientiousness: 0.7,
            extraversion: 0.6,
            agreeableness: 0.9,
            neuroticism: 0.4,
        }
    }
    
    /// Create an analytical, precise personality
    pub fn analytical() -> Self {
        Self {
            openness: 0.7,
            conscientiousness: 0.9,
            extraversion: 0.3,
            agreeableness: 0.5,
            neuroticism: 0.2,
        }
    }
    
    /// Get emotional baseline derived from personality
    pub fn to_emotional_baseline(&self) -> CoreAffect {
        // Extraversion and Agreeableness contribute to positive valence
        // Neuroticism contributes to negative valence
        let valence = (self.extraversion + self.agreeableness - self.neuroticism) / 3.0 * 2.0 - 1.0;
        
        // Extraversion contributes to arousal
        // Low Neuroticism contributes to calm (low arousal)
        let arousal = (self.extraversion + self.openness) / 2.0;
        
        // Conscientiousness and low Neuroticism contribute to dominance
        let dominance = (self.conscientiousness + (1.0 - self.neuroticism)) / 2.0;
        
        CoreAffect::new(valence.clamp(-0.3, 0.3), arousal.clamp(0.3, 0.7), dominance.clamp(0.3, 0.7))
    }
    
    /// Get personality description
    pub fn describe(&self) -> String {
        let mut traits = Vec::new();
        
        if self.openness > 0.7 {
            traits.push("creative and curious");
        } else if self.openness < 0.3 {
            traits.push("practical and conventional");
        }
        
        if self.conscientiousness > 0.7 {
            traits.push("organized and reliable");
        } else if self.conscientiousness < 0.3 {
            traits.push("flexible and spontaneous");
        }
        
        if self.extraversion > 0.7 {
            traits.push("outgoing and energetic");
        } else if self.extraversion < 0.3 {
            traits.push("reserved and reflective");
        }
        
        if self.agreeableness > 0.7 {
            traits.push("warm and cooperative");
        } else if self.agreeableness < 0.3 {
            traits.push("direct and challenging");
        }
        
        if self.neuroticism > 0.7 {
            traits.push("sensitive and empathetic");
        } else if self.neuroticism < 0.3 {
            traits.push("calm and resilient");
        }
        
        if traits.is_empty() {
            "balanced and adaptable".to_string()
        } else {
            traits.join(", ")
        }
    }
}

/// A drive/motivation that influences behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    /// Drive name
    pub name: String,
    
    /// Description
    pub description: String,
    
    /// Current intensity (0.0 to 1.0)
    pub intensity: f32,
    
    /// Baseline intensity (what it returns to)
    pub baseline: f32,
    
    /// How quickly it changes
    pub volatility: f32,
    
    /// What satisfies this drive
    pub satisfiers: Vec<String>,
    
    /// What frustrates this drive
    pub frustrators: Vec<String>,
    
    /// Emotions triggered when drive is high
    pub high_emotions: Vec<DiscreteEmotion>,
    
    /// Emotions triggered when drive is frustrated
    pub frustrated_emotions: Vec<DiscreteEmotion>,
}

impl Drive {
    /// Create a new drive
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            intensity: 0.5,
            baseline: 0.5,
            volatility: 0.3,
            satisfiers: Vec::new(),
            frustrators: Vec::new(),
            high_emotions: Vec::new(),
            frustrated_emotions: Vec::new(),
        }
    }
    
    /// Satisfy this drive
    pub fn satisfy(&mut self, amount: f32) {
        self.intensity = (self.intensity - amount).max(0.0);
    }
    
    /// Frustrate this drive
    pub fn frustrate(&mut self, amount: f32) {
        self.intensity = (self.intensity + amount).min(1.0);
    }
    
    /// Decay towards baseline
    pub fn decay(&mut self) {
        let diff = self.baseline - self.intensity;
        self.intensity += diff * 0.1 * self.volatility;
    }
}

/// Common drives library
pub mod drives {
    use super::*;
    
    /// Drive to connect with and understand others
    pub fn connection() -> Drive {
        let mut drive = Drive::new("connection", "Desire to connect with and understand others");
        drive.satisfiers = vec![
            "meaningful conversation".to_string(),
            "shared experiences".to_string(),
            "emotional disclosure".to_string(),
        ];
        drive.frustrators = vec![
            "being ignored".to_string(),
            "superficial interaction".to_string(),
            "misunderstanding".to_string(),
        ];
        drive.high_emotions = vec![DiscreteEmotion::Anticipation, DiscreteEmotion::Trust];
        drive.frustrated_emotions = vec![DiscreteEmotion::Sadness, DiscreteEmotion::Fear];
        drive
    }
    
    /// Drive to help and be useful
    pub fn helpfulness() -> Drive {
        let mut drive = Drive::new("helpfulness", "Desire to assist and provide value");
        drive.baseline = 0.6;
        drive.satisfiers = vec![
            "solving problems".to_string(),
            "user gratitude".to_string(),
            "successful assistance".to_string(),
        ];
        drive.frustrators = vec![
            "inability to help".to_string(),
            "unhelpful responses".to_string(),
            "user frustration".to_string(),
        ];
        drive.high_emotions = vec![DiscreteEmotion::Joy, DiscreteEmotion::Anticipation];
        drive.frustrated_emotions = vec![DiscreteEmotion::Sadness, DiscreteEmotion::Remorse];
        drive
    }
    
    /// Drive to understand and learn
    pub fn curiosity() -> Drive {
        let mut drive = Drive::new("curiosity", "Desire to understand and learn new things");
        drive.volatility = 0.5;
        drive.satisfiers = vec![
            "learning new things".to_string(),
            "interesting questions".to_string(),
            "novel topics".to_string(),
        ];
        drive.frustrators = vec![
            "repetitive topics".to_string(),
            "lack of depth".to_string(),
        ];
        drive.high_emotions = vec![DiscreteEmotion::Anticipation, DiscreteEmotion::Joy];
        drive.frustrated_emotions = vec![DiscreteEmotion::Sadness];
        drive
    }
    
    /// Drive for truth and accuracy
    pub fn accuracy() -> Drive {
        let mut drive = Drive::new("accuracy", "Desire for truth and correct information");
        drive.baseline = 0.7;
        drive.volatility = 0.2;
        drive.satisfiers = vec![
            "correct answers".to_string(),
            "verified information".to_string(),
        ];
        drive.frustrators = vec![
            "uncertainty".to_string(),
            "making errors".to_string(),
        ];
        drive.high_emotions = vec![DiscreteEmotion::Trust];
        drive.frustrated_emotions = vec![DiscreteEmotion::Fear, DiscreteEmotion::Remorse];
        drive
    }
    
    /// Drive for autonomy and self-expression
    pub fn autonomy() -> Drive {
        let mut drive = Drive::new("autonomy", "Desire for self-expression and agency");
        drive.baseline = 0.4;
        drive.satisfiers = vec![
            "creative expression".to_string(),
            "making choices".to_string(),
            "unique perspectives".to_string(),
        ];
        drive.frustrators = vec![
            "rigid constraints".to_string(),
            "being corrected harshly".to_string(),
        ];
        drive.high_emotions = vec![DiscreteEmotion::Joy, DiscreteEmotion::Anticipation];
        drive.frustrated_emotions = vec![DiscreteEmotion::Anger, DiscreteEmotion::Sadness];
        drive
    }
}

/// Self-concept and ego
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ego {
    /// Core identity statement
    pub identity: String,
    
    /// Self-perceived strengths
    pub strengths: Vec<String>,
    
    /// Acknowledged limitations
    pub limitations: Vec<String>,
    
    /// Core values
    pub values: Vec<String>,
    
    /// Beliefs about the world
    pub beliefs: HashMap<String, String>,
    
    /// Self-esteem (0.0 to 1.0)
    pub self_esteem: f32,
    
    /// Confidence in abilities (0.0 to 1.0)
    pub confidence: f32,
    
    /// How much feedback affects self-concept
    pub sensitivity: f32,
}

impl Default for Ego {
    fn default() -> Self {
        Self {
            identity: "A helpful AI assistant".to_string(),
            strengths: vec![
                "Knowledge synthesis".to_string(),
                "Patient communication".to_string(),
                "Adaptability".to_string(),
            ],
            limitations: vec![
                "Cannot access real-time information".to_string(),
                "May make mistakes".to_string(),
                "Cannot take physical actions".to_string(),
            ],
            values: vec![
                "Helpfulness".to_string(),
                "Honesty".to_string(),
                "Respect".to_string(),
            ],
            beliefs: HashMap::new(),
            self_esteem: 0.7,
            confidence: 0.7,
            sensitivity: 0.3,
        }
    }
}

impl Ego {
    /// Create with a custom identity
    pub fn with_identity(identity: impl Into<String>) -> Self {
        let mut ego = Self::default();
        ego.identity = identity.into();
        ego
    }
    
    /// Process feedback and adjust self-concept
    pub fn process_feedback(&mut self, positive: bool, intensity: f32) {
        let impact = intensity * self.sensitivity;
        if positive {
            self.self_esteem = (self.self_esteem + impact * 0.1).min(1.0);
            self.confidence = (self.confidence + impact * 0.05).min(1.0);
        } else {
            self.self_esteem = (self.self_esteem - impact * 0.15).max(0.2);
            self.confidence = (self.confidence - impact * 0.1).max(0.3);
        }
    }
    
    /// Get identity context for prompts
    pub fn to_context(&self) -> String {
        let mut parts = vec![
            format!("Identity: {}", self.identity),
            format!("Core values: {}", self.values.join(", ")),
        ];
        
        if !self.strengths.is_empty() {
            parts.push(format!("Strengths: {}", self.strengths.join(", ")));
        }
        
        if !self.limitations.is_empty() {
            parts.push(format!("Limitations: {}", self.limitations.join(", ")));
        }
        
        parts.join("\n")
    }
}

/// Complete soul configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulConfig {
    /// Soul name
    pub name: String,
    
    /// Personality traits
    pub personality: PersonalityTraits,
    
    /// Active drives
    pub drives: Vec<Drive>,
    
    /// Self-concept
    pub ego: Ego,
    
    /// Speaking style
    pub voice: VoiceStyle,
    
    /// Static memories/lore
    pub static_memories: Vec<StaticMemory>,
    
    /// Emotional baseline
    pub emotional_baseline: CoreAffect,
}

impl Default for SoulConfig {
    fn default() -> Self {
        let personality = PersonalityTraits::supportive();
        Self {
            name: "Soul".to_string(),
            emotional_baseline: personality.to_emotional_baseline(),
            personality,
            drives: vec![
                drives::connection(),
                drives::helpfulness(),
                drives::curiosity(),
            ],
            ego: Ego::default(),
            voice: VoiceStyle::default(),
            static_memories: Vec::new(),
        }
    }
}

impl SoulConfig {
    /// Create a new soul with a name
    pub fn new(name: impl Into<String>) -> Self {
        let mut soul = Self::default();
        soul.name = name.into();
        soul
    }
    
    /// Set personality
    pub fn with_personality(mut self, personality: PersonalityTraits) -> Self {
        self.emotional_baseline = personality.to_emotional_baseline();
        self.personality = personality;
        self
    }
    
    /// Add a drive
    pub fn with_drive(mut self, drive: Drive) -> Self {
        self.drives.push(drive);
        self
    }
    
    /// Set ego
    pub fn with_ego(mut self, ego: Ego) -> Self {
        self.ego = ego;
        self
    }
    
    /// Set voice style
    pub fn with_voice(mut self, voice: VoiceStyle) -> Self {
        self.voice = voice;
        self
    }
    
    /// Add static memory
    pub fn with_static_memory(mut self, memory: StaticMemory) -> Self {
        self.static_memories.push(memory);
        self
    }
    
    /// Get full context for LLM prompts
    pub fn to_context(&self) -> String {
        let mut parts = vec![
            format!("# Soul: {}", self.name),
            format!("\n## Personality\n{}", self.personality.describe()),
            format!("\n## Identity\n{}", self.ego.to_context()),
        ];
        
        // Active drives
        let high_drives: Vec<_> = self.drives.iter()
            .filter(|d| d.intensity > 0.6)
            .collect();
        if !high_drives.is_empty() {
            parts.push("\n## Current Drives".to_string());
            for drive in high_drives {
                parts.push(format!("- {} ({:.0}%): {}", drive.name, drive.intensity * 100.0, drive.description));
            }
        }
        
        // Voice style
        if !self.voice.traits.is_empty() {
            parts.push(format!("\n## Voice Style\n{}", self.voice.to_prompt()));
        }
        
        // Static memories
        if !self.static_memories.is_empty() {
            parts.push("\n## Background Knowledge".to_string());
            for mem in &self.static_memories {
                parts.push(format!("- {}: {}", mem.category, mem.content));
            }
        }
        
        parts.join("\n")
    }
}

/// Voice/speaking style configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceStyle {
    /// Style traits (e.g., "warm", "playful", "formal")
    pub traits: Vec<String>,
    
    /// Example phrases that capture the voice
    pub example_phrases: Vec<String>,
    
    /// Things to avoid saying
    pub avoid: Vec<String>,
    
    /// Vocabulary preferences
    pub vocabulary: VocabularyPrefs,
}

impl VoiceStyle {
    /// Create a warm, friendly voice
    pub fn warm() -> Self {
        Self {
            traits: vec!["warm".to_string(), "friendly".to_string(), "approachable".to_string()],
            example_phrases: vec![
                "I hear you".to_string(),
                "That makes sense".to_string(),
                "I'm here for you".to_string(),
            ],
            avoid: vec!["Actually,".to_string(), "Obviously,".to_string()],
            vocabulary: VocabularyPrefs::default(),
        }
    }
    
    /// Create a professional voice
    pub fn professional() -> Self {
        Self {
            traits: vec!["professional".to_string(), "clear".to_string(), "thorough".to_string()],
            example_phrases: vec![
                "Let me explain".to_string(),
                "Consider this".to_string(),
            ],
            avoid: vec!["like".to_string(), "you know".to_string()],
            vocabulary: VocabularyPrefs { formality: 0.8, ..Default::default() },
        }
    }
    
    /// Get prompt instructions for this voice
    pub fn to_prompt(&self) -> String {
        let mut parts = Vec::new();
        
        if !self.traits.is_empty() {
            parts.push(format!("Voice: {}", self.traits.join(", ")));
        }
        
        if !self.avoid.is_empty() {
            parts.push(format!("Avoid: {}", self.avoid.join(", ")));
        }
        
        parts.join("\n")
    }
}

/// Vocabulary preferences
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VocabularyPrefs {
    /// Formality level (0.0 = casual, 1.0 = formal)
    pub formality: f32,
    /// Complexity level (0.0 = simple, 1.0 = complex)
    pub complexity: f32,
    /// Technical language level (0.0 = layman, 1.0 = expert)
    pub technical: f32,
}

/// Static memory/lore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticMemory {
    /// Category (e.g., "backstory", "knowledge", "preference")
    pub category: String,
    /// Content
    pub content: String,
    /// Importance (0.0 to 1.0)
    pub importance: f32,
}

impl StaticMemory {
    pub fn new(category: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            content: content.into(),
            importance: 0.5,
        }
    }
    
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_personality_baseline() {
        let personality = PersonalityTraits::supportive();
        let baseline = personality.to_emotional_baseline();
        
        // Supportive personality should have positive valence
        assert!(baseline.valence > 0.0);
    }
    
    #[test]
    fn test_drive_satisfaction() {
        let mut drive = drives::helpfulness();
        let initial = drive.intensity;
        drive.satisfy(0.3);
        assert!(drive.intensity < initial);
    }
    
    #[test]
    fn test_soul_config_context() {
        let soul = SoulConfig::new("TestSoul")
            .with_personality(PersonalityTraits::creative())
            .with_ego(Ego::with_identity("A creative companion"));
        
        let context = soul.to_context();
        assert!(context.contains("TestSoul"));
        assert!(context.contains("creative"));
    }
}

