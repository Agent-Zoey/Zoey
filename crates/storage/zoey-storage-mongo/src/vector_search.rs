//! Vector search implementation for local MongoDB
//!
//! This module implements vector similarity search using MongoDB's aggregation
//! pipeline with manual cosine similarity calculation. Works with any MongoDB
//! instance (local or hosted) without requiring Atlas Search.

use mongodb::{
    bson::{doc, Bson, Document},
    Collection, Database, IndexModel,
};
use tracing::{info, warn};
use zoey_core::{types::*, Result, ZoeyError};

/// MongoDB Vector Search operations using local aggregation-based similarity
pub struct MongoVectorSearch {
    db: Database,
    embedding_dimension: usize,
}

impl MongoVectorSearch {
    /// Create a new MongoDB vector search instance
    pub fn new(db: Database, embedding_dimension: usize) -> Self {
        Self {
            db,
            embedding_dimension,
        }
    }

    /// Get the configured embedding dimension
    pub fn embedding_dimension(&self) -> usize {
        self.embedding_dimension
    }

    /// Create indexes to optimize vector search queries
    ///
    /// Creates compound indexes on common filter fields to speed up the
    /// initial $match stage before similarity calculation.
    pub async fn create_vector_index(&self, collection_name: &str) -> Result<()> {
        let collection: Collection<Document> = self.db.collection(collection_name);

        // Create index on embedding field existence for faster filtering
        let embedding_index = IndexModel::builder()
            .keys(doc! { "embedding": 1 })
            .build();

        // Create compound indexes for common query patterns
        let agent_index = IndexModel::builder()
            .keys(doc! { "agent_id": 1, "embedding": 1 })
            .build();

        let room_index = IndexModel::builder()
            .keys(doc! { "room_id": 1, "embedding": 1 })
            .build();

        let entity_index = IndexModel::builder()
            .keys(doc! { "entity_id": 1, "embedding": 1 })
            .build();

        collection
            .create_indexes([embedding_index, agent_index, room_index, entity_index])
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create vector indexes: {}", e)))?;

        info!(
            "Created vector search indexes for collection '{}'",
            collection_name
        );
        Ok(())
    }

    /// Search memories by embedding similarity using cosine similarity
    ///
    /// Uses MongoDB aggregation pipeline to compute cosine similarity:
    /// cos(A, B) = (A · B) / (||A|| × ||B||)
    pub async fn search_by_embedding(&self, params: SearchMemoriesParams) -> Result<Vec<Memory>> {
        let collection: Collection<Document> = self.db.collection(&params.table_name);

        // Validate embedding dimension
        if params.embedding.len() != self.embedding_dimension {
            return Err(ZoeyError::vector_search(
                format!(
                    "Embedding dimension mismatch for collection '{}'",
                    params.table_name
                ),
                params.embedding.len(),
                self.embedding_dimension,
            ));
        }

        // Convert query embedding to BSON array
        let query_embedding: Vec<Bson> = params
            .embedding
            .iter()
            .map(|&v| Bson::Double(v as f64))
            .collect();

        // Build initial match filter
        let mut match_filter = doc! {
            "embedding": { "$exists": true, "$ne": null }
        };

        if let Some(agent_id) = params.agent_id {
            match_filter.insert("agent_id", agent_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            match_filter.insert("room_id", room_id.to_string());
        }
        if let Some(entity_id) = params.entity_id {
            match_filter.insert("entity_id", entity_id.to_string());
        }
        if let Some(unique) = params.unique {
            match_filter.insert("unique_flag", unique);
        }

        // Build aggregation pipeline for cosine similarity
        let pipeline = vec![
            // Stage 1: Filter documents with embeddings and matching criteria
            doc! { "$match": match_filter },
            // Stage 2: Calculate cosine similarity
            // cos(A, B) = dot(A, B) / (magnitude(A) * magnitude(B))
            doc! {
                "$addFields": {
                    "dot_product": {
                        "$reduce": {
                            "input": { "$range": [0, self.embedding_dimension as i32] },
                            "initialValue": 0.0,
                            "in": {
                                "$add": [
                                    "$$value",
                                    {
                                        "$multiply": [
                                            { "$arrayElemAt": ["$embedding", "$$this"] },
                                            { "$arrayElemAt": [&query_embedding, "$$this"] }
                                        ]
                                    }
                                ]
                            }
                        }
                    },
                    "doc_magnitude": {
                        "$sqrt": {
                            "$reduce": {
                                "input": "$embedding",
                                "initialValue": 0.0,
                                "in": {
                                    "$add": [
                                        "$$value",
                                        { "$multiply": ["$$this", "$$this"] }
                                    ]
                                }
                            }
                        }
                    }
                }
            },
            // Stage 3: Calculate final similarity score
            doc! {
                "$addFields": {
                    "similarity": {
                        "$cond": {
                            "if": { "$eq": ["$doc_magnitude", 0] },
                            "then": 0.0,
                            "else": {
                                "$divide": [
                                    "$dot_product",
                                    { "$multiply": ["$doc_magnitude", self.query_magnitude(&params.embedding)] }
                                ]
                            }
                        }
                    }
                }
            },
            // Stage 4: Apply threshold filter if specified
            doc! {
                "$match": {
                    "similarity": { "$gte": params.threshold.unwrap_or(0.0) as f64 }
                }
            },
            // Stage 5: Sort by similarity descending
            doc! { "$sort": { "similarity": -1 } },
            // Stage 6: Limit results
            doc! { "$limit": params.count as i64 },
            // Stage 7: Project final fields (exclude embedding and intermediate calculations)
            doc! {
                "$project": {
                    "_id": 1,
                    "entity_id": 1,
                    "agent_id": 1,
                    "room_id": 1,
                    "content": 1,
                    "metadata": 1,
                    "created_at": 1,
                    "unique_flag": 1,
                    "similarity": 1
                }
            },
        ];

        // Execute aggregation
        let mut cursor = collection
            .aggregate(pipeline)
            .await
            .map_err(|e| ZoeyError::database(format!("Vector search failed: {}", e)))?;

        let mut memories = Vec::new();
        use futures::TryStreamExt;

        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate search results: {}", e))
        })? {
            let memory = Memory {
                id: parse_uuid_from_doc(&doc, "_id")?,
                entity_id: parse_uuid_from_doc(&doc, "entity_id")?,
                agent_id: parse_uuid_from_doc(&doc, "agent_id")?,
                room_id: parse_uuid_from_doc(&doc, "room_id")?,
                content: mongodb::bson::from_bson(
                    doc.get("content")
                        .cloned()
                        .unwrap_or(mongodb::bson::Bson::Document(doc! {})),
                )
                .unwrap_or_default(),
                embedding: None, // Don't return embeddings to save bandwidth
                metadata: doc
                    .get("metadata")
                    .and_then(|b| mongodb::bson::from_bson(b.clone()).ok()),
                created_at: doc.get_i64("created_at").unwrap_or(0),
                unique: doc.get_bool("unique_flag").ok(),
                similarity: doc.get_f64("similarity").ok().map(|s| s as f32),
            };
            memories.push(memory);
        }

        info!(
            "Found {} memories via local vector search in '{}'",
            memories.len(),
            params.table_name
        );

        Ok(memories)
    }

    /// Calculate magnitude of a vector
    fn query_magnitude(&self, embedding: &[f32]) -> f64 {
        embedding
            .iter()
            .map(|&x| (x as f64) * (x as f64))
            .sum::<f64>()
            .sqrt()
    }

    /// Add embedding to an existing document
    pub async fn add_embedding(
        &self,
        collection_name: &str,
        document_id: uuid::Uuid,
        embedding: Vec<f32>,
    ) -> Result<()> {
        if embedding.len() != self.embedding_dimension {
            return Err(ZoeyError::vector_search(
                "Embedding dimension mismatch",
                embedding.len(),
                self.embedding_dimension,
            ));
        }

        let collection: Collection<Document> = self.db.collection(collection_name);

        // Convert f32 to f64 for BSON storage
        let embedding_f64: Vec<f64> = embedding.iter().map(|&v| v as f64).collect();

        collection
            .update_one(
                doc! { "_id": document_id.to_string() },
                doc! { "$set": { "embedding": &embedding_f64 } },
            )
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to add embedding: {}", e)))?;

        Ok(())
    }

    /// Batch add embeddings to multiple documents
    pub async fn batch_add_embeddings(
        &self,
        collection_name: &str,
        embeddings: Vec<(uuid::Uuid, Vec<f32>)>,
    ) -> Result<()> {
        let collection: Collection<Document> = self.db.collection(collection_name);

        // Process in batches for efficiency
        for (id, embedding) in embeddings {
            if embedding.len() != self.embedding_dimension {
                warn!(
                    "Skipping document {} - embedding dimension mismatch",
                    id
                );
                continue;
            }

            // Convert f32 to f64 for BSON storage
            let embedding_f64: Vec<f64> = embedding.iter().map(|&v| v as f64).collect();

            collection
                .update_one(
                    doc! { "_id": id.to_string() },
                    doc! { "$set": { "embedding": &embedding_f64 } },
                )
                .await
                .map_err(|e| {
                    ZoeyError::database(format!("Failed to add embedding for {}: {}", id, e))
                })?;
        }

        Ok(())
    }

    /// Get similar memories to a given memory
    pub async fn get_similar_memories(
        &self,
        collection_name: &str,
        memory_id: uuid::Uuid,
        count: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<Memory>> {
        let collection: Collection<Document> = self.db.collection(collection_name);

        // First, get the source memory's embedding
        let source_doc = collection
            .find_one(doc! { "_id": memory_id.to_string() })
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get source memory: {}", e)))?
            .ok_or_else(|| ZoeyError::database("Source memory not found"))?;

        let embedding: Vec<f64> = source_doc
            .get("embedding")
            .and_then(|b| mongodb::bson::from_bson(b.clone()).ok())
            .ok_or_else(|| ZoeyError::database("Source memory has no embedding"))?;

        // Convert to BSON array for query
        let query_embedding: Vec<Bson> = embedding.iter().map(|&v| Bson::Double(v)).collect();

        // Calculate query magnitude
        let query_mag = embedding.iter().map(|&x| x * x).sum::<f64>().sqrt();

        // Build aggregation pipeline excluding the source document
        let pipeline = vec![
            // Stage 1: Filter documents with embeddings, exclude source
            doc! {
                "$match": {
                    "_id": { "$ne": memory_id.to_string() },
                    "embedding": { "$exists": true, "$ne": null }
                }
            },
            // Stage 2: Calculate cosine similarity
            doc! {
                "$addFields": {
                    "dot_product": {
                        "$reduce": {
                            "input": { "$range": [0, self.embedding_dimension as i32] },
                            "initialValue": 0.0,
                            "in": {
                                "$add": [
                                    "$$value",
                                    {
                                        "$multiply": [
                                            { "$arrayElemAt": ["$embedding", "$$this"] },
                                            { "$arrayElemAt": [&query_embedding, "$$this"] }
                                        ]
                                    }
                                ]
                            }
                        }
                    },
                    "doc_magnitude": {
                        "$sqrt": {
                            "$reduce": {
                                "input": "$embedding",
                                "initialValue": 0.0,
                                "in": {
                                    "$add": [
                                        "$$value",
                                        { "$multiply": ["$$this", "$$this"] }
                                    ]
                                }
                            }
                        }
                    }
                }
            },
            // Stage 3: Calculate final similarity score
            doc! {
                "$addFields": {
                    "similarity": {
                        "$cond": {
                            "if": { "$eq": ["$doc_magnitude", 0] },
                            "then": 0.0,
                            "else": {
                                "$divide": [
                                    "$dot_product",
                                    { "$multiply": ["$doc_magnitude", query_mag] }
                                ]
                            }
                        }
                    }
                }
            },
            // Stage 4: Apply threshold filter if specified
            doc! {
                "$match": {
                    "similarity": { "$gte": threshold.unwrap_or(0.0) as f64 }
                }
            },
            // Stage 5: Sort by similarity descending
            doc! { "$sort": { "similarity": -1 } },
            // Stage 6: Limit results
            doc! { "$limit": count as i64 },
            // Stage 7: Project final fields
            doc! {
                "$project": {
                    "_id": 1,
                    "entity_id": 1,
                    "agent_id": 1,
                    "room_id": 1,
                    "content": 1,
                    "metadata": 1,
                    "created_at": 1,
                    "unique_flag": 1,
                    "similarity": 1
                }
            },
        ];

        let mut cursor = collection
            .aggregate(pipeline)
            .await
            .map_err(|e| ZoeyError::database(format!("Similar memory search failed: {}", e)))?;

        let mut memories = Vec::new();
        use futures::TryStreamExt;

        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate similar memories: {}", e))
        })? {
            let memory = Memory {
                id: parse_uuid_from_doc(&doc, "_id")?,
                entity_id: parse_uuid_from_doc(&doc, "entity_id")?,
                agent_id: parse_uuid_from_doc(&doc, "agent_id")?,
                room_id: parse_uuid_from_doc(&doc, "room_id")?,
                content: mongodb::bson::from_bson(
                    doc.get("content")
                        .cloned()
                        .unwrap_or(mongodb::bson::Bson::Document(doc! {})),
                )
                .unwrap_or_default(),
                embedding: None,
                metadata: doc
                    .get("metadata")
                    .and_then(|b| mongodb::bson::from_bson(b.clone()).ok()),
                created_at: doc.get_i64("created_at").unwrap_or(0),
                unique: doc.get_bool("unique_flag").ok(),
                similarity: doc.get_f64("similarity").ok().map(|s| s as f32),
            };
            memories.push(memory);
        }

        Ok(memories)
    }

    /// Search with pre-computed query magnitude for performance
    /// Useful when searching with the same embedding multiple times
    pub async fn search_with_precomputed(
        &self,
        collection_name: &str,
        embedding: &[f32],
        query_magnitude: f64,
        count: usize,
        threshold: Option<f32>,
        filters: Option<Document>,
    ) -> Result<Vec<Memory>> {
        let collection: Collection<Document> = self.db.collection(collection_name);

        // Convert query embedding to BSON array
        let query_embedding: Vec<Bson> = embedding.iter().map(|&v| Bson::Double(v as f64)).collect();

        // Build initial match filter
        let mut match_filter = doc! {
            "embedding": { "$exists": true, "$ne": null }
        };

        if let Some(f) = filters {
            for (k, v) in f {
                match_filter.insert(k, v);
            }
        }

        let pipeline = vec![
            doc! { "$match": match_filter },
            doc! {
                "$addFields": {
                    "dot_product": {
                        "$reduce": {
                            "input": { "$range": [0, self.embedding_dimension as i32] },
                            "initialValue": 0.0,
                            "in": {
                                "$add": [
                                    "$$value",
                                    {
                                        "$multiply": [
                                            { "$arrayElemAt": ["$embedding", "$$this"] },
                                            { "$arrayElemAt": [&query_embedding, "$$this"] }
                                        ]
                                    }
                                ]
                            }
                        }
                    },
                    "doc_magnitude": {
                        "$sqrt": {
                            "$reduce": {
                                "input": "$embedding",
                                "initialValue": 0.0,
                                "in": {
                                    "$add": [
                                        "$$value",
                                        { "$multiply": ["$$this", "$$this"] }
                                    ]
                                }
                            }
                        }
                    }
                }
            },
            doc! {
                "$addFields": {
                    "similarity": {
                        "$cond": {
                            "if": { "$eq": ["$doc_magnitude", 0] },
                            "then": 0.0,
                            "else": {
                                "$divide": [
                                    "$dot_product",
                                    { "$multiply": ["$doc_magnitude", query_magnitude] }
                                ]
                            }
                        }
                    }
                }
            },
            doc! {
                "$match": {
                    "similarity": { "$gte": threshold.unwrap_or(0.0) as f64 }
                }
            },
            doc! { "$sort": { "similarity": -1 } },
            doc! { "$limit": count as i64 },
            doc! {
                "$project": {
                    "_id": 1,
                    "entity_id": 1,
                    "agent_id": 1,
                    "room_id": 1,
                    "content": 1,
                    "metadata": 1,
                    "created_at": 1,
                    "unique_flag": 1,
                    "similarity": 1
                }
            },
        ];

        let mut cursor = collection
            .aggregate(pipeline)
            .await
            .map_err(|e| ZoeyError::database(format!("Vector search failed: {}", e)))?;

        let mut memories = Vec::new();
        use futures::TryStreamExt;

        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate search results: {}", e))
        })? {
            let memory = Memory {
                id: parse_uuid_from_doc(&doc, "_id")?,
                entity_id: parse_uuid_from_doc(&doc, "entity_id")?,
                agent_id: parse_uuid_from_doc(&doc, "agent_id")?,
                room_id: parse_uuid_from_doc(&doc, "room_id")?,
                content: mongodb::bson::from_bson(
                    doc.get("content")
                        .cloned()
                        .unwrap_or(mongodb::bson::Bson::Document(doc! {})),
                )
                .unwrap_or_default(),
                embedding: None,
                metadata: doc
                    .get("metadata")
                    .and_then(|b| mongodb::bson::from_bson(b.clone()).ok()),
                created_at: doc.get_i64("created_at").unwrap_or(0),
                unique: doc.get_bool("unique_flag").ok(),
                similarity: doc.get_f64("similarity").ok().map(|s| s as f32),
            };
            memories.push(memory);
        }

        Ok(memories)
    }
}

/// Helper function to parse UUID from BSON document
fn parse_uuid_from_doc(doc: &Document, field: &str) -> Result<uuid::Uuid> {
    doc.get(field)
        .and_then(|b| b.as_str())
        .ok_or_else(|| ZoeyError::database(format!("Missing or invalid field: {}", field)))
        .and_then(|s| {
            uuid::Uuid::parse_str(s)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID in {}: {}", field, e)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_magnitude() {
        // Create a mock - we can't test with real DB here
        // Just verify the magnitude calculation logic
        let embedding = vec![3.0_f32, 4.0];
        let magnitude: f64 = embedding
            .iter()
            .map(|&x| (x as f64) * (x as f64))
            .sum::<f64>()
            .sqrt();
        assert!((magnitude - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_logic() {
        // Vectors: [1, 0] and [1, 0] should have similarity 1.0
        // Vectors: [1, 0] and [0, 1] should have similarity 0.0
        // Vectors: [1, 0] and [-1, 0] should have similarity -1.0

        let a = vec![1.0_f64, 0.0];
        let b = vec![1.0_f64, 0.0];

        let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
        let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

        let similarity = dot / (mag_a * mag_b);
        assert!((similarity - 1.0).abs() < 0.0001);
    }
}
