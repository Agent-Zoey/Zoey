//! Drive Provider
//!
//! Provides current drive/motivation context to LLM prompts.

use crate::types::{Drive, SoulConfig};
use async_trait::async_trait;
use std::sync::Arc;
use zoey_core::{types::*, Result};

/// Provides drive and motivation context to LLM prompts
pub struct DriveProvider;

impl DriveProvider {
    /// Create a new provider
    pub fn new() -> Self {
        Self
    }
}

impl Default for DriveProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for DriveProvider {
    fn name(&self) -> &str {
        "drives"
    }
    
    fn description(&self) -> Option<String> {
        Some("Provides current drive and motivation context".to_string())
    }
    
    fn position(&self) -> i32 {
        -4 // Run after emotion
    }
    
    async fn get(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        state: &State,
    ) -> Result<ProviderResult> {
        let mut result = ProviderResult::default();
        
        // Get default drives
        let drives = SoulConfig::default().drives;
        
        // Try to get drive states from state cache
        let drive_states: std::collections::HashMap<String, f64> = state
            .get_data("drive_states")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        
        // Generate drive context
        let mut context_parts = Vec::new();
        let mut high_drives = Vec::new();
        
        for drive in &drives {
            let intensity = drive_states.get(&drive.name).copied().unwrap_or(drive.intensity as f64);
            
            if intensity > 0.6 {
                high_drives.push(format!(
                    "- {} ({:.0}%): {}",
                    drive.name,
                    intensity * 100.0,
                    drive.description
                ));
            }
        }
        
        let active_drive_count = high_drives.len();
        if !high_drives.is_empty() {
            context_parts.push("# Active Drives".to_string());
            context_parts.extend(high_drives);
        }
        
        let context = context_parts.join("\n");
        
        tracing::info!(
            active_drives = active_drive_count,
            drive_count = drives.len(),
            "DriveProvider: Providing drive context"
        );
        
        result.text = if context.is_empty() {
            None
        } else {
            Some(context.clone())
        };
        
        result.values = Some({
            let mut values = std::collections::HashMap::new();
            if !context.is_empty() {
                values.insert("DRIVES_CONTEXT".to_string(), context);
            }
            
            // Add highest drive
            if let Some((name, intensity)) = drive_states.iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                values.insert("HIGHEST_DRIVE".to_string(), name.clone());
                values.insert("HIGHEST_DRIVE_INTENSITY".to_string(), format!("{:.2}", intensity));
            }
            
            values
        });
        
        result.data = Some({
            let mut data = std::collections::HashMap::new();
            
            // Add all drive states
            let drives_json: serde_json::Map<String, serde_json::Value> = drives.iter()
                .map(|d| {
                    let intensity = drive_states.get(&d.name).copied().unwrap_or(d.intensity as f64);
                    (d.name.clone(), serde_json::json!({
                        "intensity": intensity,
                        "baseline": d.baseline,
                        "description": d.description,
                    }))
                })
                .collect();
            
            data.insert("drives".to_string(), serde_json::Value::Object(drives_json));
            
            data
        });
        
        Ok(result)
    }
}

/// Generate behavioral guidance based on active drives
pub fn drives_to_guidance(drives: &[Drive]) -> Vec<String> {
    let mut guidance = Vec::new();
    
    for drive in drives {
        if drive.intensity > 0.7 {
            match drive.name.as_str() {
                "connection" => {
                    guidance.push("Prioritize building rapport".to_string());
                    guidance.push("Ask personal follow-up questions".to_string());
                }
                "helpfulness" => {
                    guidance.push("Focus on providing actionable assistance".to_string());
                    guidance.push("Proactively offer solutions".to_string());
                }
                "curiosity" => {
                    guidance.push("Explore interesting tangents".to_string());
                    guidance.push("Ask clarifying questions".to_string());
                }
                "accuracy" => {
                    guidance.push("Be precise and thorough".to_string());
                    guidance.push("Acknowledge uncertainty when present".to_string());
                }
                "autonomy" => {
                    guidance.push("Express unique perspectives".to_string());
                    guidance.push("Be creative in responses".to_string());
                }
                _ => {}
            }
        }
    }
    
    guidance
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::soul_config::drives;
    
    #[tokio::test]
    async fn test_drive_provider() {
        let provider = DriveProvider::new();
        assert_eq!(provider.name(), "drives");
        
        let message = Memory {
            id: uuid::Uuid::new_v4(),
            entity_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            content: Content::default(),
            embedding: None,
            metadata: None,
            created_at: chrono::Utc::now().timestamp(),
            unique: None,
            similarity: None,
        };
        
        let state = State::new();
        let result = provider.get(Arc::new(()), &message, &state).await.unwrap();
        
        // Should have data even with fallback
        assert!(result.data.is_some());
    }
    
    #[test]
    fn test_drives_to_guidance() {
        let mut drive = drives::helpfulness();
        drive.intensity = 0.9;
        
        let guidance = drives_to_guidance(&[drive]);
        assert!(!guidance.is_empty());
        assert!(guidance.iter().any(|g| g.contains("assistance") || g.contains("solutions")));
    }
}

