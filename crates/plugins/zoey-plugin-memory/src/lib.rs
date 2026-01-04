#![warn(missing_docs)]
#![warn(clippy::all)]

use async_trait::async_trait;
use zoey_core::{types::*, Result};
use std::collections::HashMap;
use std::sync::Arc;

mod evaluators;
mod plugin;
mod provider;
mod service;

pub use evaluators::{LongTermExtractionEvaluator, SummarizationEvaluator};
pub use plugin::MemoryManagerPlugin;
pub use provider::ContextMemoriesProvider;
pub use service::{MemoryPolicy, MemoryTier, RetrievalMode, TieredMemoryService};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_name() {
        let plugin = MemoryManagerPlugin::default();
        assert_eq!(plugin.name(), "memory-manager");
    }
}
