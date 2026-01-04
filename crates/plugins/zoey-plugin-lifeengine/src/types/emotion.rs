//! Emotional State Model
//!
//! A rich emotional model that tracks multiple dimensions:
//! - Core affect (valence + arousal)
//! - Discrete emotions (joy, sadness, anger, fear, etc.)
//! - Mood (longer-term emotional baseline)
//! - Emotional memory (how emotions change over time)

use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Core affect model - the fundamental emotional space
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CoreAffect {
    /// Pleasure-displeasure dimension (-1.0 to 1.0)
    pub valence: f32,
    
    /// Activation-deactivation dimension (0.0 to 1.0)
    pub arousal: f32,
    
    /// Dominance-submissiveness (0.0 to 1.0)
    pub dominance: f32,
}

impl Default for CoreAffect {
    fn default() -> Self {
        Self {
            valence: 0.0,
            arousal: 0.5,
            dominance: 0.5,
        }
    }
}

impl CoreAffect {
    /// Create a new core affect state
    pub fn new(valence: f32, arousal: f32, dominance: f32) -> Self {
        Self {
            valence: valence.clamp(-1.0, 1.0),
            arousal: arousal.clamp(0.0, 1.0),
            dominance: dominance.clamp(0.0, 1.0),
        }
    }
    
    /// Interpolate between two affect states
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            valence: self.valence + (other.valence - self.valence) * t,
            arousal: self.arousal + (other.arousal - self.arousal) * t,
            dominance: self.dominance + (other.dominance - self.dominance) * t,
        }
    }
    
    /// Calculate distance from another affect state
    pub fn distance(&self, other: &Self) -> f32 {
        let dv = self.valence - other.valence;
        let da = self.arousal - other.arousal;
        let dd = self.dominance - other.dominance;
        (dv * dv + da * da + dd * dd).sqrt()
    }
    
    /// Map to the closest discrete emotion
    pub fn to_discrete_emotion(&self) -> DiscreteEmotion {
        // PAD (Pleasure-Arousal-Dominance) to emotion mapping
        let emotions = [
            (DiscreteEmotion::Joy, CoreAffect::new(0.8, 0.6, 0.7)),
            (DiscreteEmotion::Trust, CoreAffect::new(0.6, 0.3, 0.4)),
            (DiscreteEmotion::Fear, CoreAffect::new(-0.6, 0.8, 0.2)),
            (DiscreteEmotion::Surprise, CoreAffect::new(0.2, 0.8, 0.4)),
            (DiscreteEmotion::Sadness, CoreAffect::new(-0.7, 0.2, 0.3)),
            (DiscreteEmotion::Disgust, CoreAffect::new(-0.6, 0.5, 0.6)),
            (DiscreteEmotion::Anger, CoreAffect::new(-0.7, 0.8, 0.7)),
            (DiscreteEmotion::Anticipation, CoreAffect::new(0.4, 0.6, 0.5)),
        ];
        
        emotions
            .iter()
            .min_by_key(|(_, affect)| OrderedFloat(self.distance(affect)))
            .map(|(emotion, _)| *emotion)
            .unwrap_or(DiscreteEmotion::Neutral)
    }
}

/// Plutchik's wheel of emotions + neutral
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscreteEmotion {
    // Primary emotions
    Joy,
    Trust,
    Fear,
    Surprise,
    Sadness,
    Disgust,
    Anger,
    Anticipation,
    
    // Neutral state
    Neutral,
    
    // Combined emotions (dyads)
    Love,          // Joy + Trust
    Submission,    // Trust + Fear
    Awe,           // Fear + Surprise
    Disapproval,   // Surprise + Sadness
    Remorse,       // Sadness + Disgust
    Contempt,      // Disgust + Anger
    Aggressiveness, // Anger + Anticipation
    Optimism,      // Anticipation + Joy
}

impl DiscreteEmotion {
    /// Get the default core affect for this emotion
    pub fn to_core_affect(&self) -> CoreAffect {
        match self {
            DiscreteEmotion::Joy => CoreAffect::new(0.8, 0.6, 0.7),
            DiscreteEmotion::Trust => CoreAffect::new(0.6, 0.3, 0.4),
            DiscreteEmotion::Fear => CoreAffect::new(-0.6, 0.8, 0.2),
            DiscreteEmotion::Surprise => CoreAffect::new(0.2, 0.8, 0.4),
            DiscreteEmotion::Sadness => CoreAffect::new(-0.7, 0.2, 0.3),
            DiscreteEmotion::Disgust => CoreAffect::new(-0.6, 0.5, 0.6),
            DiscreteEmotion::Anger => CoreAffect::new(-0.7, 0.8, 0.7),
            DiscreteEmotion::Anticipation => CoreAffect::new(0.4, 0.6, 0.5),
            DiscreteEmotion::Neutral => CoreAffect::default(),
            DiscreteEmotion::Love => CoreAffect::new(0.7, 0.45, 0.55),
            DiscreteEmotion::Submission => CoreAffect::new(0.0, 0.55, 0.3),
            DiscreteEmotion::Awe => CoreAffect::new(-0.2, 0.8, 0.3),
            DiscreteEmotion::Disapproval => CoreAffect::new(-0.25, 0.5, 0.35),
            DiscreteEmotion::Remorse => CoreAffect::new(-0.65, 0.35, 0.45),
            DiscreteEmotion::Contempt => CoreAffect::new(-0.65, 0.65, 0.65),
            DiscreteEmotion::Aggressiveness => CoreAffect::new(-0.15, 0.7, 0.6),
            DiscreteEmotion::Optimism => CoreAffect::new(0.6, 0.6, 0.6),
        }
    }
    
    /// Get a description of this emotion
    pub fn description(&self) -> &'static str {
        match self {
            DiscreteEmotion::Joy => "feeling pleasure and happiness",
            DiscreteEmotion::Trust => "feeling safe and confident in others",
            DiscreteEmotion::Fear => "anticipating danger or threat",
            DiscreteEmotion::Surprise => "experiencing the unexpected",
            DiscreteEmotion::Sadness => "feeling loss or disappointment",
            DiscreteEmotion::Disgust => "rejecting something unpleasant",
            DiscreteEmotion::Anger => "feeling frustrated or wronged",
            DiscreteEmotion::Anticipation => "expecting something to happen",
            DiscreteEmotion::Neutral => "emotionally balanced",
            DiscreteEmotion::Love => "deep affection and connection",
            DiscreteEmotion::Submission => "yielding to authority",
            DiscreteEmotion::Awe => "wonder mixed with fear",
            DiscreteEmotion::Disapproval => "judging negatively",
            DiscreteEmotion::Remorse => "regret and guilt",
            DiscreteEmotion::Contempt => "superiority and disdain",
            DiscreteEmotion::Aggressiveness => "forceful determination",
            DiscreteEmotion::Optimism => "hopeful expectation",
        }
    }
}

/// Record of an emotional event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalEvent {
    /// When this event occurred
    pub timestamp: DateTime<Utc>,
    
    /// Trigger for this emotion
    pub trigger: String,
    
    /// The emotion experienced
    pub emotion: DiscreteEmotion,
    
    /// Intensity (0.0 to 1.0)
    pub intensity: f32,
    
    /// Duration estimate in seconds
    pub duration_secs: u64,
}

/// Long-term mood state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mood {
    /// Baseline affect state
    pub baseline: CoreAffect,
    
    /// How easily affected by events
    pub volatility: f32,
    
    /// How quickly mood returns to baseline
    pub resilience: f32,
    
    /// Current deviation from baseline
    pub current_deviation: CoreAffect,
    
    /// Last update time
    pub last_update: DateTime<Utc>,
}

impl Default for Mood {
    fn default() -> Self {
        Self {
            baseline: CoreAffect::default(),
            volatility: 0.5,
            resilience: 0.3,
            current_deviation: CoreAffect::new(0.0, 0.0, 0.0),
            last_update: Utc::now(),
        }
    }
}

impl Mood {
    /// Get current mood (baseline + deviation)
    pub fn current(&self) -> CoreAffect {
        CoreAffect::new(
            (self.baseline.valence + self.current_deviation.valence).clamp(-1.0, 1.0),
            (self.baseline.arousal + self.current_deviation.arousal).clamp(0.0, 1.0),
            (self.baseline.dominance + self.current_deviation.dominance).clamp(0.0, 1.0),
        )
    }
    
    /// Decay deviation towards zero based on time elapsed
    pub fn decay(&mut self) {
        let elapsed = Utc::now().signed_duration_since(self.last_update);
        let decay_factor = (-self.resilience * elapsed.num_seconds() as f32 / 3600.0).exp();
        
        self.current_deviation.valence *= decay_factor;
        self.current_deviation.arousal *= decay_factor;
        self.current_deviation.dominance *= decay_factor;
        self.last_update = Utc::now();
    }
    
    /// Apply an emotional event to mood
    pub fn apply_event(&mut self, event: &EmotionalEvent) {
        self.decay();
        
        let emotion_affect = event.emotion.to_core_affect();
        let impact = event.intensity * self.volatility;
        
        self.current_deviation.valence += (emotion_affect.valence - self.baseline.valence) * impact * 0.3;
        self.current_deviation.arousal += (emotion_affect.arousal - self.baseline.arousal) * impact * 0.2;
        self.current_deviation.dominance += (emotion_affect.dominance - self.baseline.dominance) * impact * 0.1;
        
        // Clamp deviations
        self.current_deviation.valence = self.current_deviation.valence.clamp(-0.5, 0.5);
        self.current_deviation.arousal = self.current_deviation.arousal.clamp(-0.3, 0.3);
        self.current_deviation.dominance = self.current_deviation.dominance.clamp(-0.3, 0.3);
    }
}

/// Complete emotional state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalState {
    /// Current immediate affect
    pub affect: CoreAffect,
    
    /// Current discrete emotion
    pub primary_emotion: DiscreteEmotion,
    
    /// Intensity of primary emotion
    pub intensity: f32,
    
    /// Secondary emotions present
    pub secondary_emotions: HashMap<DiscreteEmotion, f32>,
    
    /// Long-term mood
    pub mood: Mood,
    
    /// Recent emotional history
    pub history: Vec<EmotionalEvent>,
    
    /// Maximum history size
    #[serde(skip)]
    max_history: usize,
}

impl Default for EmotionalState {
    fn default() -> Self {
        Self::new()
    }
}

impl EmotionalState {
    /// Create a new emotional state
    pub fn new() -> Self {
        Self {
            affect: CoreAffect::default(),
            primary_emotion: DiscreteEmotion::Neutral,
            intensity: 0.5,
            secondary_emotions: HashMap::new(),
            mood: Mood::default(),
            history: Vec::new(),
            max_history: 100,
        }
    }
    
    /// Create with a specific baseline mood
    pub fn with_baseline(valence: f32, arousal: f32, dominance: f32) -> Self {
        let mut state = Self::new();
        state.mood.baseline = CoreAffect::new(valence, arousal, dominance);
        state.affect = state.mood.baseline;
        state
    }
    
    /// Update emotional state based on an event
    pub fn process_event(&mut self, trigger: impl Into<String>, emotion: DiscreteEmotion, intensity: f32) {
        let event = EmotionalEvent {
            timestamp: Utc::now(),
            trigger: trigger.into(),
            emotion,
            intensity: intensity.clamp(0.0, 1.0),
            duration_secs: (intensity * 300.0) as u64, // Higher intensity = longer duration
        };
        
        // Update affect
        let emotion_affect = emotion.to_core_affect();
        self.affect = self.affect.lerp(&emotion_affect, intensity * 0.5);
        
        // Update primary emotion if intense enough
        if intensity > self.intensity * 0.8 {
            // Move current primary to secondary
            if self.primary_emotion != DiscreteEmotion::Neutral && self.intensity > 0.3 {
                self.secondary_emotions.insert(self.primary_emotion, self.intensity * 0.5);
            }
            self.primary_emotion = emotion;
            self.intensity = intensity;
        } else {
            // Add as secondary
            let current = self.secondary_emotions.get(&emotion).copied().unwrap_or(0.0);
            self.secondary_emotions.insert(emotion, (current + intensity).min(1.0));
        }
        
        // Apply to mood
        self.mood.apply_event(&event);
        
        // Add to history
        self.history.push(event);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
        
        // Decay secondary emotions
        let decay = 0.9;
        self.secondary_emotions.retain(|_, v| {
            *v *= decay;
            *v > 0.1
        });
    }
    
    /// Decay emotional state over time
    pub fn decay(&mut self) {
        // Primary emotion intensity decays
        self.intensity *= 0.95;
        if self.intensity < 0.2 {
            self.primary_emotion = self.mood.current().to_discrete_emotion();
            self.intensity = 0.3;
        }
        
        // Affect drifts towards mood
        self.affect = self.affect.lerp(&self.mood.current(), 0.1);
        
        // Mood decays
        self.mood.decay();
        
        // Secondary emotions decay
        self.secondary_emotions.retain(|_, v| {
            *v *= 0.9;
            *v > 0.1
        });
    }
    
    /// Get a human-readable description of current emotional state
    pub fn describe(&self) -> String {
        let intensity_word = if self.intensity > 0.8 {
            "intensely"
        } else if self.intensity > 0.5 {
            "moderately"
        } else {
            "slightly"
        };
        
        let mut desc = format!(
            "Currently {} {} ({})",
            intensity_word,
            format!("{:?}", self.primary_emotion).to_lowercase(),
            self.primary_emotion.description()
        );
        
        if !self.secondary_emotions.is_empty() {
            let secondary: Vec<_> = self.secondary_emotions
                .iter()
                .filter(|(_, &v)| v > 0.3)
                .map(|(e, _)| format!("{:?}", e).to_lowercase())
                .collect();
            if !secondary.is_empty() {
                desc.push_str(&format!(", with undertones of {}", secondary.join(", ")));
            }
        }
        
        desc
    }
    
    /// Serialize to context for LLM prompts
    pub fn to_context(&self) -> String {
        format!(
            "Emotional state: {}\n\
             Valence: {:.2} (negative to positive)\n\
             Arousal: {:.2} (calm to excited)\n\
             Mood baseline: valence={:.2}, arousal={:.2}",
            self.describe(),
            self.affect.valence,
            self.affect.arousal,
            self.mood.baseline.valence,
            self.mood.baseline.arousal
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_core_affect_to_emotion() {
        let affect = CoreAffect::new(0.8, 0.6, 0.7);
        assert_eq!(affect.to_discrete_emotion(), DiscreteEmotion::Joy);
        
        let sad = CoreAffect::new(-0.7, 0.2, 0.3);
        assert_eq!(sad.to_discrete_emotion(), DiscreteEmotion::Sadness);
    }
    
    #[test]
    fn test_emotional_state_process_event() {
        let mut state = EmotionalState::new();
        state.process_event("received good news", DiscreteEmotion::Joy, 0.9);
        
        assert_eq!(state.primary_emotion, DiscreteEmotion::Joy);
        assert!(state.intensity > 0.8);
        assert!(state.affect.valence > 0.3);
    }
    
    #[test]
    fn test_mood_decay() {
        let mut mood = Mood::default();
        mood.current_deviation = CoreAffect::new(0.3, 0.2, 0.1);
        mood.last_update = Utc::now() - chrono::Duration::hours(1);
        
        mood.decay();
        
        // Should have decayed towards zero
        assert!(mood.current_deviation.valence.abs() < 0.3);
    }
}

