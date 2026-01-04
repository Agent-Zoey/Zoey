//! Thread-safe runtime reference wrapper for components
//!
//! Provides a type-erased, Send+Sync wrapper around AgentRuntime
//! that can be safely passed to actions, providers, and evaluators

use std::sync::{Arc, RwLock, Weak};
use uuid::Uuid;

/// Thread-safe runtime reference that can be passed to components
///
/// This wrapper provides:
/// - Thread safety (Send + Sync)
/// - Type erasure (implements Any)
/// - Weak reference to avoid circular dependencies
/// - Safe downcasting when components need runtime access
pub struct RuntimeRef {
    /// Weak reference to avoid circular ownership
    runtime_weak: Weak<RwLock<crate::AgentRuntime>>,

    /// Cached agent ID for quick access
    agent_id: Uuid,

    /// Cached agent name for logging
    agent_name: String,
}

impl RuntimeRef {
    /// Create a new runtime reference
    pub fn new(runtime: &Arc<RwLock<crate::AgentRuntime>>) -> Self {
        let rt = runtime.read().unwrap();
        Self {
            runtime_weak: Arc::downgrade(runtime),
            agent_id: rt.agent_id,
            agent_name: rt.character.name.clone(),
        }
    }

    /// Get agent ID without accessing full runtime
    pub fn agent_id(&self) -> Uuid {
        self.agent_id
    }

    /// Get agent name without accessing full runtime
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    /// Try to upgrade to full runtime access
    /// Returns None if runtime has been dropped
    pub fn try_upgrade(&self) -> Option<Arc<RwLock<crate::AgentRuntime>>> {
        self.runtime_weak.upgrade()
    }

    /// Get a setting from runtime if available
    pub fn get_setting(&self, key: &str) -> Option<serde_json::Value> {
        self.try_upgrade().and_then(|rt| {
            let runtime = rt.read().unwrap();
            runtime.get_setting(key)
        })
    }
}

// Implement Send + Sync manually since we control the internals
unsafe impl Send for RuntimeRef {}
unsafe impl Sync for RuntimeRef {}

impl std::fmt::Debug for RuntimeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeRef")
            .field("agent_id", &self.agent_id)
            .field("agent_name", &self.agent_name)
            .field("runtime_available", &self.runtime_weak.strong_count())
            .finish()
    }
}

/// Convert RuntimeRef to type-erased Arc for component interfaces
impl RuntimeRef {
    /// Convert to type-erased Arc<dyn Any + Send + Sync>
    pub fn as_any_arc(self: &Arc<Self>) -> Arc<dyn std::any::Any + Send + Sync> {
        Arc::clone(self) as Arc<dyn std::any::Any + Send + Sync>
    }
}

/// Helper to downcast from Any back to RuntimeRef
pub fn downcast_runtime_ref(any: &Arc<dyn std::any::Any + Send + Sync>) -> Option<Arc<RuntimeRef>> {
    any.clone().downcast::<RuntimeRef>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Character, RuntimeOpts};

    #[tokio::test]
    async fn test_runtime_ref() {
        let runtime = crate::AgentRuntime::new(RuntimeOpts {
            character: Some(Character {
                name: "TestBot".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();

        let runtime_ref = Arc::new(RuntimeRef::new(&runtime));

        assert_eq!(runtime_ref.agent_name(), "TestBot");
        assert!(runtime_ref.try_upgrade().is_some());
    }

    #[tokio::test]
    async fn test_runtime_ref_weak() {
        let runtime = crate::AgentRuntime::new(RuntimeOpts {
            character: Some(Character {
                name: "TestBot".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();

        let runtime_ref = Arc::new(RuntimeRef::new(&runtime));

        // Runtime still exists
        assert!(runtime_ref.try_upgrade().is_some());

        // Drop runtime
        drop(runtime);

        // Weak reference should fail to upgrade
        assert!(runtime_ref.try_upgrade().is_none());
    }

    #[tokio::test]
    async fn test_downcast() {
        let runtime = crate::AgentRuntime::new(RuntimeOpts {
            character: Some(Character {
                name: "TestBot".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();

        let runtime_ref = Arc::new(RuntimeRef::new(&runtime));
        let any_arc = runtime_ref.as_any_arc();

        let downcasted = downcast_runtime_ref(&any_arc);
        assert!(downcasted.is_some());
        assert_eq!(downcasted.unwrap().agent_name(), "TestBot");
    }
}
