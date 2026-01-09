//! Vector search implementation using Supabase with pgvector
//!
//! Supabase supports pgvector for vector similarity search.
//! This module provides utilities for setting up and using vector search.

use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use zoey_core::{types::*, Result, ZoeyError};

/// Supabase Vector Search operations using pgvector
pub struct SupabaseVectorSearch {
    client: Client,
    base_url: String,
    embedding_dimension: usize,
}

impl SupabaseVectorSearch {
    /// Create a new Supabase vector search instance
    pub fn new(base_url: String, api_key: &str, embedding_dimension: usize) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "apikey",
            header::HeaderValue::from_str(api_key)
                .map_err(|e| ZoeyError::database(format!("Invalid API key: {}", e)))?,
        );
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {}", api_key))
                .map_err(|e| ZoeyError::database(format!("Invalid API key: {}", e)))?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| ZoeyError::database(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            base_url,
            embedding_dimension,
        })
    }

    /// Get the configured embedding dimension
    pub fn embedding_dimension(&self) -> usize {
        self.embedding_dimension
    }

    /// Search memories by embedding similarity using Supabase RPC
    ///
    /// This requires a PostgreSQL function like:
    /// ```sql
    /// CREATE OR REPLACE FUNCTION match_memories(
    ///   query_embedding vector(1536),
    ///   match_count int DEFAULT 10,
    ///   filter_agent_id uuid DEFAULT NULL,
    ///   filter_room_id uuid DEFAULT NULL,
    ///   similarity_threshold float DEFAULT 0.7
    /// )
    /// RETURNS TABLE (
    ///   id uuid,
    ///   entity_id uuid,
    ///   agent_id uuid,
    ///   room_id uuid,
    ///   content jsonb,
    ///   metadata jsonb,
    ///   created_at bigint,
    ///   unique_flag boolean,
    ///   similarity float
    /// )
    /// LANGUAGE plpgsql
    /// AS $$
    /// BEGIN
    ///   RETURN QUERY
    ///   SELECT
    ///     m.id,
    ///     m.entity_id,
    ///     m.agent_id,
    ///     m.room_id,
    ///     m.content,
    ///     m.metadata,
    ///     m.created_at,
    ///     m.unique_flag,
    ///     1 - (m.embedding <=> query_embedding) as similarity
    ///   FROM memories m
    ///   WHERE
    ///     m.embedding IS NOT NULL
    ///     AND (filter_agent_id IS NULL OR m.agent_id = filter_agent_id)
    ///     AND (filter_room_id IS NULL OR m.room_id = filter_room_id)
    ///     AND 1 - (m.embedding <=> query_embedding) > similarity_threshold
    ///   ORDER BY m.embedding <=> query_embedding
    ///   LIMIT match_count;
    /// END;
    /// $$;
    /// ```
    pub async fn search_by_embedding(&self, params: SearchMemoriesParams) -> Result<Vec<Memory>> {
        // Validate embedding dimension
        if params.embedding.len() != self.embedding_dimension {
            return Err(ZoeyError::vector_search(
                format!(
                    "Embedding dimension mismatch for table '{}'",
                    params.table_name
                ),
                params.embedding.len(),
                self.embedding_dimension,
            ));
        }

        #[derive(Serialize)]
        struct MatchMemoriesParams {
            query_embedding: Vec<f32>,
            match_count: i32,
            filter_agent_id: Option<String>,
            filter_room_id: Option<String>,
            similarity_threshold: f32,
        }

        let rpc_params = MatchMemoriesParams {
            query_embedding: params.embedding,
            match_count: params.count as i32,
            filter_agent_id: params.agent_id.map(|id| id.to_string()),
            filter_room_id: params.room_id.map(|id| id.to_string()),
            similarity_threshold: params.threshold.unwrap_or(0.7),
        };

        let url = format!("{}/rest/v1/rpc/match_memories", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&rpc_params)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Vector search request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Vector search failed ({}): {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct MemoryResult {
            id: String,
            entity_id: String,
            agent_id: String,
            room_id: String,
            content: serde_json::Value,
            metadata: Option<serde_json::Value>,
            created_at: i64,
            unique_flag: Option<bool>,
            similarity: Option<f32>,
        }

        let results: Vec<MemoryResult> = response
            .json()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to parse search results: {}", e)))?;

        let memories: Vec<Memory> = results
            .into_iter()
            .filter_map(|r| {
                Some(Memory {
                    id: uuid::Uuid::parse_str(&r.id).ok()?,
                    entity_id: uuid::Uuid::parse_str(&r.entity_id).ok()?,
                    agent_id: uuid::Uuid::parse_str(&r.agent_id).ok()?,
                    room_id: uuid::Uuid::parse_str(&r.room_id).ok()?,
                    content: serde_json::from_value(r.content).unwrap_or_default(),
                    embedding: None, // Don't return embeddings to save bandwidth
                    metadata: r.metadata.and_then(|m| serde_json::from_value(m).ok()),
                    created_at: r.created_at,
                    unique: r.unique_flag,
                    similarity: r.similarity,
                })
            })
            .collect();

        info!(
            "Found {} memories via Supabase vector search",
            memories.len()
        );

        Ok(memories)
    }

    /// Add embedding to an existing document
    pub async fn add_embedding(
        &self,
        table_name: &str,
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

        #[derive(Serialize)]
        struct UpdateEmbedding {
            embedding: Vec<f32>,
        }

        let url = format!(
            "{}/rest/v1/{}?id=eq.{}",
            self.base_url, table_name, document_id
        );

        let response = self
            .client
            .patch(&url)
            .json(&UpdateEmbedding { embedding })
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to add embedding: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Add embedding failed ({}): {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Batch add embeddings using RPC function
    ///
    /// This requires a PostgreSQL function like:
    /// ```sql
    /// CREATE OR REPLACE FUNCTION batch_update_embeddings(
    ///   updates jsonb
    /// )
    /// RETURNS void
    /// LANGUAGE plpgsql
    /// AS $$
    /// DECLARE
    ///   update_item jsonb;
    /// BEGIN
    ///   FOR update_item IN SELECT * FROM jsonb_array_elements(updates)
    ///   LOOP
    ///     UPDATE memories
    ///     SET embedding = (update_item->>'embedding')::vector
    ///     WHERE id = (update_item->>'id')::uuid;
    ///   END LOOP;
    /// END;
    /// $$;
    /// ```
    pub async fn batch_add_embeddings(
        &self,
        table_name: &str,
        embeddings: Vec<(uuid::Uuid, Vec<f32>)>,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        // Validate all embeddings
        for (id, emb) in &embeddings {
            if emb.len() != self.embedding_dimension {
                warn!(
                    "Skipping document {} - embedding dimension mismatch",
                    id
                );
            }
        }

        #[derive(Serialize)]
        struct EmbeddingUpdate {
            id: String,
            embedding: Vec<f32>,
        }

        let updates: Vec<EmbeddingUpdate> = embeddings
            .into_iter()
            .filter(|(_, emb)| emb.len() == self.embedding_dimension)
            .map(|(id, emb)| EmbeddingUpdate {
                id: id.to_string(),
                embedding: emb,
            })
            .collect();

        if updates.is_empty() {
            return Ok(());
        }

        #[derive(Serialize)]
        struct BatchUpdateParams {
            updates: serde_json::Value,
        }

        let url = format!("{}/rest/v1/rpc/batch_update_embeddings", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&BatchUpdateParams {
                updates: serde_json::to_value(&updates).unwrap_or_default(),
            })
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Batch embedding update failed: {}", e)))?;

        if !response.status().is_success() {
            // Fall back to individual updates
            warn!(
                "Batch embedding update RPC failed, falling back to individual updates for {}",
                table_name
            );
            for update in updates {
                if let Ok(id) = uuid::Uuid::parse_str(&update.id) {
                    self.add_embedding(table_name, id, update.embedding).await?;
                }
            }
        }

        Ok(())
    }

    /// Get similar memories to a given memory using RPC
    ///
    /// This requires a PostgreSQL function like:
    /// ```sql
    /// CREATE OR REPLACE FUNCTION get_similar_memories(
    ///   source_memory_id uuid,
    ///   match_count int DEFAULT 10,
    ///   similarity_threshold float DEFAULT 0.7
    /// )
    /// RETURNS TABLE (
    ///   id uuid,
    ///   entity_id uuid,
    ///   agent_id uuid,
    ///   room_id uuid,
    ///   content jsonb,
    ///   metadata jsonb,
    ///   created_at bigint,
    ///   unique_flag boolean,
    ///   similarity float
    /// )
    /// LANGUAGE plpgsql
    /// AS $$
    /// DECLARE
    ///   source_embedding vector;
    /// BEGIN
    ///   SELECT embedding INTO source_embedding
    ///   FROM memories
    ///   WHERE id = source_memory_id;
    ///
    ///   IF source_embedding IS NULL THEN
    ///     RAISE EXCEPTION 'Source memory has no embedding';
    ///   END IF;
    ///
    ///   RETURN QUERY
    ///   SELECT
    ///     m.id,
    ///     m.entity_id,
    ///     m.agent_id,
    ///     m.room_id,
    ///     m.content,
    ///     m.metadata,
    ///     m.created_at,
    ///     m.unique_flag,
    ///     1 - (m.embedding <=> source_embedding) as similarity
    ///   FROM memories m
    ///   WHERE
    ///     m.id != source_memory_id
    ///     AND m.embedding IS NOT NULL
    ///     AND 1 - (m.embedding <=> source_embedding) > similarity_threshold
    ///   ORDER BY m.embedding <=> source_embedding
    ///   LIMIT match_count;
    /// END;
    /// $$;
    /// ```
    pub async fn get_similar_memories(
        &self,
        memory_id: uuid::Uuid,
        count: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<Memory>> {
        #[derive(Serialize)]
        struct GetSimilarParams {
            source_memory_id: String,
            match_count: i32,
            similarity_threshold: f32,
        }

        let url = format!("{}/rest/v1/rpc/get_similar_memories", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&GetSimilarParams {
                source_memory_id: memory_id.to_string(),
                match_count: count as i32,
                similarity_threshold: threshold.unwrap_or(0.7),
            })
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Similar memories request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Get similar memories failed ({}): {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct MemoryResult {
            id: String,
            entity_id: String,
            agent_id: String,
            room_id: String,
            content: serde_json::Value,
            metadata: Option<serde_json::Value>,
            created_at: i64,
            unique_flag: Option<bool>,
            similarity: Option<f32>,
        }

        let results: Vec<MemoryResult> = response
            .json()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to parse similar memories: {}", e)))?;

        Ok(results
            .into_iter()
            .filter_map(|r| {
                Some(Memory {
                    id: uuid::Uuid::parse_str(&r.id).ok()?,
                    entity_id: uuid::Uuid::parse_str(&r.entity_id).ok()?,
                    agent_id: uuid::Uuid::parse_str(&r.agent_id).ok()?,
                    room_id: uuid::Uuid::parse_str(&r.room_id).ok()?,
                    content: serde_json::from_value(r.content).unwrap_or_default(),
                    embedding: None,
                    metadata: r.metadata.and_then(|m| serde_json::from_value(m).ok()),
                    created_at: r.created_at,
                    unique: r.unique_flag,
                    similarity: r.similarity,
                })
            })
            .collect())
    }
}

/// SQL migrations for setting up pgvector in Supabase
pub mod migrations {
    /// SQL to enable pgvector extension
    pub const ENABLE_PGVECTOR: &str = "CREATE EXTENSION IF NOT EXISTS vector;";

    /// SQL to add embedding column to memories table
    pub fn add_embedding_column(dimension: usize) -> String {
        format!(
            "ALTER TABLE memories ADD COLUMN IF NOT EXISTS embedding vector({});",
            dimension
        )
    }

    /// SQL to create HNSW index for fast similarity search
    pub const CREATE_HNSW_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS memories_embedding_hnsw_idx
        ON memories
        USING hnsw (embedding vector_cosine_ops);
    "#;

    /// SQL to create the match_memories function
    pub fn create_match_memories_function(dimension: usize) -> String {
        format!(
            r#"
            CREATE OR REPLACE FUNCTION match_memories(
              query_embedding vector({}),
              match_count int DEFAULT 10,
              filter_agent_id uuid DEFAULT NULL,
              filter_room_id uuid DEFAULT NULL,
              similarity_threshold float DEFAULT 0.7
            )
            RETURNS TABLE (
              id uuid,
              entity_id uuid,
              agent_id uuid,
              room_id uuid,
              content jsonb,
              metadata jsonb,
              created_at bigint,
              unique_flag boolean,
              similarity float
            )
            LANGUAGE plpgsql
            AS $$
            BEGIN
              RETURN QUERY
              SELECT
                m.id,
                m.entity_id,
                m.agent_id,
                m.room_id,
                m.content,
                m.metadata,
                m.created_at,
                m.unique_flag,
                1 - (m.embedding <=> query_embedding) as similarity
              FROM memories m
              WHERE
                m.embedding IS NOT NULL
                AND (filter_agent_id IS NULL OR m.agent_id = filter_agent_id)
                AND (filter_room_id IS NULL OR m.room_id = filter_room_id)
                AND 1 - (m.embedding <=> query_embedding) > similarity_threshold
              ORDER BY m.embedding <=> query_embedding
              LIMIT match_count;
            END;
            $$;
            "#,
            dimension
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_search_creation() {
        // This is a compilation test
        assert!(true);
    }

    #[test]
    fn test_migrations() {
        let enable = migrations::ENABLE_PGVECTOR;
        assert!(enable.contains("vector"));

        let add_col = migrations::add_embedding_column(1536);
        assert!(add_col.contains("1536"));

        let func = migrations::create_match_memories_function(1536);
        assert!(func.contains("match_memories"));
    }
}
