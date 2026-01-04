//! Local Vector Database Plugin
//!
//! Provides in-memory vector search with disk persistence (no PostgreSQL required).
//! Uses HNSW (Hierarchical Navigable Small World) for fast similarity search.
//!
//! # Features
//! - No external database dependencies
//! - Fast HNSW-based similarity search
//! - Automatic persistence to disk
//! - Cosine similarity metric
//! - Concurrent access support

use async_trait::async_trait;
use zoey_core::{types::UUID, Plugin, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

mod store;
pub use store::{LocalVectorStore, VectorStoreConfig};

/// Local Vector Database Plugin
pub struct LocalVectorPlugin {
    store: Arc<RwLock<LocalVectorStore>>,
}

impl LocalVectorPlugin {
    /// Create a new local vector plugin with default configuration
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let config = VectorStoreConfig {
            data_dir: data_dir.into(),
            dimension: 1536, // Default to OpenAI embedding size
            max_elements: 100_000,
            m: 16,                // HNSW M parameter
            ef_construction: 200, // HNSW ef_construction parameter
        };

        let store = LocalVectorStore::new(config)?;

        Ok(Self {
            store: Arc::new(RwLock::new(store)),
        })
    }

    /// Create with custom configuration
    pub fn with_config(config: VectorStoreConfig) -> Result<Self> {
        let store = LocalVectorStore::new(config)?;

        Ok(Self {
            store: Arc::new(RwLock::new(store)),
        })
    }

    /// Search for similar vectors
    pub async fn search(
        &self,
        embedding: Vec<f32>,
        count: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<(UUID, f32)>> {
        let store = self.store.read();
        store.search(embedding, count, threshold)
    }

    /// Add or update an embedding
    pub async fn add_embedding(&self, id: UUID, embedding: Vec<f32>) -> Result<()> {
        let mut store = self.store.write();
        store.add_embedding(id, embedding)
    }

    /// Add multiple embeddings in batch
    pub async fn batch_add_embeddings(&self, embeddings: Vec<(UUID, Vec<f32>)>) -> Result<()> {
        let mut store = self.store.write();
        store.batch_add_embeddings(embeddings)
    }

    /// Remove an embedding
    pub async fn remove_embedding(&self, id: UUID) -> Result<()> {
        let mut store = self.store.write();
        store.remove_embedding(id)
    }

    /// Save the index to disk
    pub async fn save(&self) -> Result<()> {
        let store = self.store.read();
        store.save()
    }

    /// Load saved index from disk
    pub async fn load(&mut self) -> Result<()> {
        let mut store = self.store.write();
        store.load()
    }

    /// Get statistics about the vector store
    pub async fn stats(&self) -> VectorStoreStats {
        let store = self.store.read();
        store.stats()
    }
}

#[async_trait]
impl Plugin for LocalVectorPlugin {
    fn name(&self) -> &str {
        "local-vector"
    }

    fn description(&self) -> &str {
        "Local vector database with HNSW search (no PostgreSQL required)"
    }

    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        info!("Initializing local vector database plugin");

        // Load existing index if available
        let mut store = self.store.write();
        if let Err(e) = store.load() {
            warn!("Could not load existing index: {}. Starting fresh.", e);
        } else {
            let stats = store.stats();
            info!(
                "Loaded local vector index: {} vectors, dimension {}",
                stats.total_vectors, stats.dimension
            );
        }

        Ok(())
    }
}

/// Statistics about the vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStoreStats {
    pub total_vectors: usize,
    pub dimension: usize,
    pub max_elements: usize,
    pub index_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(data_dir: impl Into<std::path::PathBuf>) -> VectorStoreConfig {
        VectorStoreConfig {
            data_dir: data_dir.into(),
            dimension: 3, // Small dimension for testing
            max_elements: 1000,
            m: 16,
            ef_construction: 200,
        }
    }

    #[tokio::test]
    async fn test_add_and_search() {
        let temp_dir = TempDir::new().unwrap();
        let plugin = LocalVectorPlugin::with_config(test_config(temp_dir.path())).unwrap();

        // Add some test embeddings
        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];
        let embedding3 = vec![0.9, 0.1, 0.0];

        let id1 = UUID::new_v4();
        let id2 = UUID::new_v4();
        let id3 = UUID::new_v4();

        plugin.add_embedding(id1, embedding1.clone()).await.unwrap();
        plugin.add_embedding(id2, embedding2).await.unwrap();
        plugin.add_embedding(id3, embedding3).await.unwrap();

        // Search for similar to embedding1
        let results = plugin.search(embedding1, 2, None).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, id1); // Most similar should be itself
        assert_eq!(results[1].0, id3); // Second most similar
    }

    #[tokio::test]
    async fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let embedding = vec![1.0, 0.0, 0.0];
        let id = UUID::new_v4();

        {
            let plugin = LocalVectorPlugin::with_config(test_config(temp_dir.path())).unwrap();
            plugin.add_embedding(id, embedding.clone()).await.unwrap();
            plugin.save().await.unwrap();
        }

        // Create new plugin instance and verify data persisted
        {
            let mut plugin = LocalVectorPlugin::with_config(test_config(temp_dir.path())).unwrap();
            plugin.load().await.unwrap();

            let results = plugin.search(embedding, 1, None).await.unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].0, id);
        }
    }
}
