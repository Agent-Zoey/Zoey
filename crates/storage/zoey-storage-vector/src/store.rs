//! Local Vector Store Implementation
//!
//! Uses HNSW (Hierarchical Navigable Small World) for efficient similarity search.

use zoey_core::{types::UUID, ZoeyError, Result};
use hnsw_rs::prelude::*;
use ndarray::Array1;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

use crate::VectorStoreStats;

/// Configuration for the local vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStoreConfig {
    /// Directory to store index data
    pub data_dir: PathBuf,
    /// Dimension of vectors
    pub dimension: usize,
    /// Maximum number of elements
    pub max_elements: usize,
    /// HNSW M parameter (number of bi-directional links per node)
    pub m: usize,
    /// HNSW ef_construction parameter (search depth during construction)
    pub ef_construction: usize,
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./.zoey/db/vectors"),
            dimension: 1536,
            max_elements: 100_000,
            m: 16,
            ef_construction: 200,
        }
    }
}

/// Metadata for persisting the vector store
#[derive(Serialize, Deserialize)]
struct StoreMetadata {
    uuid_to_id: HashMap<UUID, usize>,
    id_to_uuid: HashMap<usize, UUID>,
    next_id: usize,
    dimension: usize,
    max_elements: usize,
    m: usize,
    ef_construction: usize,
    /// Stored vectors for rebuilding the index
    vectors: Vec<(usize, Vec<f32>)>,
}

/// Local vector store using HNSW
pub struct LocalVectorStore {
    config: VectorStoreConfig,
    /// HNSW index
    index: Hnsw<'static, f32, DistCosine>,
    /// Mapping from UUID to internal ID
    uuid_to_id: HashMap<UUID, usize>,
    /// Mapping from internal ID to UUID
    id_to_uuid: HashMap<usize, UUID>,
    /// Stored vectors (for persistence)
    vectors: HashMap<usize, Vec<f32>>,
    /// Next available internal ID
    next_id: usize,
}

impl LocalVectorStore {
    /// Create a new local vector store
    pub fn new(config: VectorStoreConfig) -> Result<Self> {
        // Create data directory if it doesn't exist
        fs::create_dir_all(&config.data_dir)
            .map_err(|e| ZoeyError::other(format!("Failed to create data directory: {}", e)))?;

        // Create HNSW index
        let index = Hnsw::<f32, DistCosine>::new(
            config.m,
            config.max_elements,
            config.ef_construction,
            config.ef_construction,
            DistCosine {},
        );

        Ok(Self {
            config,
            index,
            uuid_to_id: HashMap::new(),
            id_to_uuid: HashMap::new(),
            vectors: HashMap::new(),
            next_id: 0,
        })
    }

    /// Create with auto-load from existing data
    pub fn new_with_load(config: VectorStoreConfig) -> Result<Self> {
        let mut store = Self::new(config)?;
        if store.has_saved_index() {
            store.load()?;
        }
        Ok(store)
    }

    /// Search for similar vectors
    pub fn search(
        &self,
        embedding: Vec<f32>,
        count: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<(UUID, f32)>> {
        if embedding.len() != self.config.dimension {
            return Err(ZoeyError::validation(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.config.dimension,
                embedding.len()
            )));
        }

        // Convert to ndarray
        let query = Array1::from_vec(embedding);

        // Perform search
        let neighbors = self
            .index
            .search(&query.to_vec(), count, self.config.ef_construction);

        // Convert results
        let mut results = Vec::new();
        for neighbor in neighbors {
            let id = neighbor.d_id;

            if let Some(uuid) = self.id_to_uuid.get(&id) {
                // HNSW returns distance, convert to similarity (1 - distance for cosine)
                let similarity = 1.0 - neighbor.distance;

                // Filter by threshold if provided
                if let Some(min_threshold) = threshold {
                    if similarity < min_threshold {
                        continue;
                    }
                }

                results.push((*uuid, similarity));
            }
        }

        Ok(results)
    }

    /// Add or update an embedding
    pub fn add_embedding(&mut self, id: UUID, embedding: Vec<f32>) -> Result<()> {
        if embedding.len() != self.config.dimension {
            return Err(ZoeyError::validation(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.config.dimension,
                embedding.len()
            )));
        }

        // Get or create internal ID
        let internal_id = if let Some(&existing_id) = self.uuid_to_id.get(&id) {
            // Update existing
            existing_id
        } else {
            // Add new
            let new_id = self.next_id;
            self.uuid_to_id.insert(id, new_id);
            self.id_to_uuid.insert(new_id, id);
            self.next_id += 1;
            new_id
        };

        // Store vector for persistence
        self.vectors.insert(internal_id, embedding.clone());

        // Insert into HNSW index
        let data_point = (&embedding[..], internal_id);
        self.index.insert(data_point);

        debug!("Added embedding for ID {} (internal: {})", id, internal_id);

        Ok(())
    }

    /// Add multiple embeddings in batch
    pub fn batch_add_embeddings(&mut self, embeddings: Vec<(UUID, Vec<f32>)>) -> Result<()> {
        for (id, embedding) in embeddings {
            self.add_embedding(id, embedding)?;
        }
        Ok(())
    }

    /// Remove an embedding
    pub fn remove_embedding(&mut self, id: UUID) -> Result<()> {
        if let Some(internal_id) = self.uuid_to_id.remove(&id) {
            self.id_to_uuid.remove(&internal_id);
            self.vectors.remove(&internal_id);
            // Note: HNSW doesn't support efficient deletion, so we just remove from mappings
            // The vector stays in the index but won't be returned in results
            debug!("Removed embedding for ID {}", id);
        }
        Ok(())
    }

    /// Save the index to disk
    ///
    /// Persists both the UUID mappings and the vectors to the data directory.
    /// Uses bincode serialization for portability across hnsw_rs versions.
    pub fn save(&self) -> Result<()> {
        info!("Saving vector store to {:?}", self.config.data_dir);

        // Collect vectors for serialization
        let vectors: Vec<(usize, Vec<f32>)> = self
            .vectors
            .iter()
            .map(|(&id, vec)| (id, vec.clone()))
            .collect();

        // Save metadata (UUID mappings + vectors)
        let metadata = StoreMetadata {
            uuid_to_id: self.uuid_to_id.clone(),
            id_to_uuid: self.id_to_uuid.clone(),
            next_id: self.next_id,
            dimension: self.config.dimension,
            max_elements: self.config.max_elements,
            m: self.config.m,
            ef_construction: self.config.ef_construction,
            vectors,
        };

        let metadata_path = self.config.data_dir.join("metadata.bin");
        let metadata_bytes = bincode::serialize(&metadata)
            .map_err(|e| ZoeyError::other(format!("Failed to serialize metadata: {}", e)))?;

        fs::write(&metadata_path, metadata_bytes)
            .map_err(|e| ZoeyError::other(format!("Failed to write metadata file: {}", e)))?;

        info!(
            "Vector store saved successfully ({} vectors)",
            self.uuid_to_id.len()
        );
        Ok(())
    }

    /// Load the index from disk
    ///
    /// Loads the UUID mappings and vectors, then rebuilds the HNSW index.
    pub fn load(&mut self) -> Result<()> {
        info!("Loading vector store from {:?}", self.config.data_dir);

        let metadata_path = self.config.data_dir.join("metadata.bin");

        // Check if files exist
        if !metadata_path.exists() {
            return Err(ZoeyError::not_found(
                "Vector store metadata not found - no saved index".to_string(),
            ));
        }

        // Load metadata
        let metadata_bytes = fs::read(&metadata_path)
            .map_err(|e| ZoeyError::other(format!("Failed to read metadata file: {}", e)))?;

        let metadata: StoreMetadata = bincode::deserialize(&metadata_bytes)
            .map_err(|e| ZoeyError::other(format!("Failed to deserialize metadata: {}", e)))?;

        // Verify configuration matches
        if metadata.dimension != self.config.dimension {
            return Err(ZoeyError::validation(format!(
                "Dimension mismatch: saved={}, config={}",
                metadata.dimension, self.config.dimension
            )));
        }

        // Rebuild HNSW index
        let index = Hnsw::<f32, DistCosine>::new(
            metadata.m,
            metadata.max_elements,
            metadata.ef_construction,
            metadata.ef_construction,
            DistCosine {},
        );

        // Update store state
        self.index = index;
        self.uuid_to_id = metadata.uuid_to_id;
        self.id_to_uuid = metadata.id_to_uuid;
        self.next_id = metadata.next_id;
        self.vectors.clear();

        // Re-insert all vectors into the index
        for (internal_id, vector) in metadata.vectors {
            self.vectors.insert(internal_id, vector.clone());
            let data_point = (&vector[..], internal_id);
            self.index.insert(data_point);
        }

        info!(
            "Vector store loaded successfully ({} vectors)",
            self.uuid_to_id.len()
        );
        Ok(())
    }

    /// Check if a saved index exists
    pub fn has_saved_index(&self) -> bool {
        self.config.data_dir.join("metadata.bin").exists()
    }

    /// Clear all data and remove persisted files
    pub fn clear(&mut self) -> Result<()> {
        info!("Clearing vector store");

        // Remove persisted files
        let metadata_path = self.config.data_dir.join("metadata.bin");
        if metadata_path.exists() {
            fs::remove_file(&metadata_path)
                .map_err(|e| ZoeyError::other(format!("Failed to remove metadata file: {}", e)))?;
        }

        // Reset in-memory state
        self.uuid_to_id.clear();
        self.id_to_uuid.clear();
        self.vectors.clear();
        self.next_id = 0;

        // Create fresh index
        self.index = Hnsw::<f32, DistCosine>::new(
            self.config.m,
            self.config.max_elements,
            self.config.ef_construction,
            self.config.ef_construction,
            DistCosine {},
        );

        info!("Vector store cleared");
        Ok(())
    }

    /// Get statistics about the vector store
    pub fn stats(&self) -> VectorStoreStats {
        VectorStoreStats {
            total_vectors: self.uuid_to_id.len(),
            dimension: self.config.dimension,
            max_elements: self.config.max_elements,
            index_type: "HNSW".to_string(),
        }
    }

    /// Get the number of stored vectors
    pub fn len(&self) -> usize {
        self.uuid_to_id.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.uuid_to_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_add_and_search() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorStoreConfig {
            data_dir: temp_dir.path().to_path_buf(),
            dimension: 3,
            ..Default::default()
        };

        let mut store = LocalVectorStore::new(config).unwrap();

        let id1 = UUID::new_v4();
        let id2 = UUID::new_v4();
        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];

        store.add_embedding(id1, embedding1.clone()).unwrap();
        store.add_embedding(id2, embedding2).unwrap();

        let results = store.search(embedding1, 1, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, id1);
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorStoreConfig {
            data_dir: temp_dir.path().to_path_buf(),
            dimension: 3,
            ..Default::default()
        };

        let id = UUID::new_v4();
        let embedding = vec![1.0, 0.0, 0.0];

        // Create and save
        {
            let mut store = LocalVectorStore::new(config.clone()).unwrap();
            store.add_embedding(id, embedding.clone()).unwrap();
            store.save().unwrap();
        }

        // Load and verify
        {
            let mut store = LocalVectorStore::new(config).unwrap();
            store.load().unwrap();

            let results = store.search(embedding, 1, None).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].0, id);
        }
    }

    #[test]
    fn test_batch_add() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorStoreConfig {
            data_dir: temp_dir.path().to_path_buf(),
            dimension: 3,
            ..Default::default()
        };

        let mut store = LocalVectorStore::new(config).unwrap();

        let embeddings = vec![
            (UUID::new_v4(), vec![1.0, 0.0, 0.0]),
            (UUID::new_v4(), vec![0.0, 1.0, 0.0]),
            (UUID::new_v4(), vec![0.0, 0.0, 1.0]),
        ];

        store.batch_add_embeddings(embeddings).unwrap();
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn test_clear() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorStoreConfig {
            data_dir: temp_dir.path().to_path_buf(),
            dimension: 3,
            ..Default::default()
        };

        let mut store = LocalVectorStore::new(config).unwrap();

        let id = UUID::new_v4();
        store.add_embedding(id, vec![1.0, 0.0, 0.0]).unwrap();
        store.save().unwrap();

        assert!(store.has_saved_index());
        store.clear().unwrap();

        assert!(store.is_empty());
        assert!(!store.has_saved_index());
    }

    #[test]
    fn test_remove_embedding() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorStoreConfig {
            data_dir: temp_dir.path().to_path_buf(),
            dimension: 3,
            ..Default::default()
        };

        let mut store = LocalVectorStore::new(config).unwrap();

        let id1 = UUID::new_v4();
        let id2 = UUID::new_v4();

        store.add_embedding(id1, vec![1.0, 0.0, 0.0]).unwrap();
        store.add_embedding(id2, vec![0.0, 1.0, 0.0]).unwrap();

        assert_eq!(store.len(), 2);

        store.remove_embedding(id1).unwrap();

        // UUID mapping is removed, but HNSW index still has the vector
        // The search should not return id1 anymore
        let results = store.search(vec![1.0, 0.0, 0.0], 2, None).unwrap();
        assert!(!results.iter().any(|(uuid, _)| *uuid == id1));
    }
}
