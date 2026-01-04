/*!
# Confidence Scoring Module

Quantifies uncertainty with confidence intervals and bounds.
*/

use serde::{Deserialize, Serialize};

/// Confidence level categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    /// Very Low: 0-20%
    VeryLow,
    /// Low: 20-40%
    Low,
    /// Medium: 40-60%
    Medium,
    /// High: 60-80%
    High,
    /// Very High: 80-100%
    VeryHigh,
}

/// Uncertainty bounds (confidence interval)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UncertaintyBounds {
    /// Lower bound of confidence interval
    pub lower: f64,

    /// Upper bound of confidence interval
    pub upper: f64,

    /// Standard deviation
    pub std_dev: Option<f64>,
}

/// Complete confidence score with uncertainty quantification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceScore {
    /// Point estimate (0.0-1.0)
    pub value: f64,

    /// Categorical level
    pub level: ConfidenceLevel,

    /// Uncertainty bounds
    pub bounds: UncertaintyBounds,

    /// Factors contributing to confidence
    pub factors: Vec<ConfidenceFactor>,

    /// Factors reducing confidence
    pub uncertainties: Vec<UncertaintyFactor>,
}

/// A factor that increases confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceFactor {
    /// Description
    pub description: String,

    /// Impact on confidence (0.0-1.0)
    pub impact: f64,
}

/// A factor that introduces uncertainty
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyFactor {
    /// Description
    pub description: String,

    /// Impact on uncertainty (0.0-1.0)
    pub impact: f64,

    /// Type of uncertainty
    pub uncertainty_type: UncertaintyType,
}

/// Type of uncertainty
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UncertaintyType {
    /// Lack of data
    InsufficientData,

    /// Conflicting evidence
    ConflictingEvidence,

    /// Model limitations
    ModelLimitation,

    /// Ambiguous input
    Ambiguity,

    /// Out of distribution
    OutOfDistribution,

    /// Known unknowns
    KnownUnknowns,
}

impl ConfidenceScore {
    /// Create a new confidence score
    pub fn new(value: f64) -> Self {
        let value = value.clamp(0.0, 1.0);
        let level = Self::value_to_level(value);

        // Default bounds: ±10%
        let margin = 0.1;
        let lower = (value - margin).max(0.0);
        let upper = (value + margin).min(1.0);

        Self {
            value,
            level,
            bounds: UncertaintyBounds {
                lower,
                upper,
                std_dev: None,
            },
            factors: Vec::new(),
            uncertainties: Vec::new(),
        }
    }

    /// Create with explicit bounds
    pub fn with_bounds(value: f64, lower: f64, upper: f64) -> Self {
        let value = value.clamp(0.0, 1.0);
        let lower = lower.clamp(0.0, 1.0);
        let upper = upper.clamp(0.0, 1.0);

        let std_dev = (upper - lower) / 4.0; // Rough estimate

        Self {
            value,
            level: Self::value_to_level(value),
            bounds: UncertaintyBounds {
                lower,
                upper,
                std_dev: Some(std_dev),
            },
            factors: Vec::new(),
            uncertainties: Vec::new(),
        }
    }

    /// Add a confidence factor
    pub fn add_factor(&mut self, description: impl Into<String>, impact: f64) {
        self.factors.push(ConfidenceFactor {
            description: description.into(),
            impact: impact.clamp(0.0, 1.0),
        });
    }

    /// Add an uncertainty factor
    pub fn add_uncertainty(
        &mut self,
        description: impl Into<String>,
        impact: f64,
        uncertainty_type: UncertaintyType,
    ) {
        self.uncertainties.push(UncertaintyFactor {
            description: description.into(),
            impact: impact.clamp(0.0, 1.0),
            uncertainty_type,
        });
    }

    /// Calculate adjusted confidence based on factors
    pub fn adjusted_confidence(&self) -> f64 {
        let positive: f64 = self.factors.iter().map(|f| f.impact).sum();
        let negative: f64 = self.uncertainties.iter().map(|u| u.impact).sum();

        let adjustment = (positive - negative) * 0.1; // Scale factor
        (self.value + adjustment).clamp(0.0, 1.0)
    }

    /// Get confidence interval width
    pub fn interval_width(&self) -> f64 {
        self.bounds.upper - self.bounds.lower
    }

    /// Check if confidence is above threshold
    pub fn is_confident(&self, threshold: f64) -> bool {
        self.value >= threshold
    }

    /// Get human-readable description
    pub fn to_description(&self) -> String {
        format!(
            "{:?} confidence ({:.1}%), uncertainty range: {:.1}% - {:.1}%",
            self.level,
            self.value * 100.0,
            self.bounds.lower * 100.0,
            self.bounds.upper * 100.0
        )
    }

    /// Convert value to level
    fn value_to_level(value: f64) -> ConfidenceLevel {
        match value {
            v if v < 0.2 => ConfidenceLevel::VeryLow,
            v if v < 0.4 => ConfidenceLevel::Low,
            v if v < 0.6 => ConfidenceLevel::Medium,
            v if v < 0.8 => ConfidenceLevel::High,
            _ => ConfidenceLevel::VeryHigh,
        }
    }
}

impl Default for ConfidenceScore {
    fn default() -> Self {
        Self::new(0.5)
    }
}

/// Builder for constructing confidence scores
pub struct ConfidenceBuilder {
    base_value: f64,
    factors: Vec<ConfidenceFactor>,
    uncertainties: Vec<UncertaintyFactor>,
    bounds: Option<(f64, f64)>,
}

impl ConfidenceBuilder {
    /// Create a new builder
    pub fn new(base_value: f64) -> Self {
        Self {
            base_value: base_value.clamp(0.0, 1.0),
            factors: Vec::new(),
            uncertainties: Vec::new(),
            bounds: None,
        }
    }

    /// Add confidence factor
    pub fn factor(mut self, description: impl Into<String>, impact: f64) -> Self {
        self.factors.push(ConfidenceFactor {
            description: description.into(),
            impact: impact.clamp(0.0, 1.0),
        });
        self
    }

    /// Add uncertainty factor
    pub fn uncertainty(
        mut self,
        description: impl Into<String>,
        impact: f64,
        uncertainty_type: UncertaintyType,
    ) -> Self {
        self.uncertainties.push(UncertaintyFactor {
            description: description.into(),
            impact: impact.clamp(0.0, 1.0),
            uncertainty_type,
        });
        self
    }

    /// Set explicit bounds
    pub fn bounds(mut self, lower: f64, upper: f64) -> Self {
        self.bounds = Some((lower.clamp(0.0, 1.0), upper.clamp(0.0, 1.0)));
        self
    }

    /// Build the confidence score
    pub fn build(self) -> ConfidenceScore {
        let mut score = if let Some((lower, upper)) = self.bounds {
            ConfidenceScore::with_bounds(self.base_value, lower, upper)
        } else {
            ConfidenceScore::new(self.base_value)
        };

        score.factors = self.factors;
        score.uncertainties = self.uncertainties;

        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_levels() {
        assert_eq!(ConfidenceScore::new(0.1).level, ConfidenceLevel::VeryLow);
        assert_eq!(ConfidenceScore::new(0.3).level, ConfidenceLevel::Low);
        assert_eq!(ConfidenceScore::new(0.5).level, ConfidenceLevel::Medium);
        assert_eq!(ConfidenceScore::new(0.7).level, ConfidenceLevel::High);
        assert_eq!(ConfidenceScore::new(0.9).level, ConfidenceLevel::VeryHigh);
    }

    #[test]
    fn test_bounds() {
        let score = ConfidenceScore::with_bounds(0.8, 0.7, 0.9);
        assert_eq!(score.bounds.lower, 0.7);
        assert_eq!(score.bounds.upper, 0.9);
        assert!((score.interval_width() - 0.2).abs() < 1e-10); // Floating point comparison
    }

    #[test]
    fn test_confidence_builder() {
        let score = ConfidenceBuilder::new(0.7)
            .factor("Multiple supporting sources", 0.8)
            .factor("Recent data", 0.6)
            .uncertainty(
                "Limited sample size",
                0.3,
                UncertaintyType::InsufficientData,
            )
            .build();

        assert_eq!(score.value, 0.7);
        assert_eq!(score.factors.len(), 2);
        assert_eq!(score.uncertainties.len(), 1);
    }

    #[test]
    fn test_adjusted_confidence() {
        let mut score = ConfidenceScore::new(0.5);
        score.add_factor("Strong evidence", 0.8);
        score.add_uncertainty("Some ambiguity", 0.3, UncertaintyType::Ambiguity);

        let adjusted = score.adjusted_confidence();
        assert!(adjusted > 0.5); // Should be higher due to net positive factors
    }
}
