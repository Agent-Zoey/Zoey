//! Supabase database adapter
//!
//! Implements core database operations for Supabase using the PostgREST API.
//! Supports pgvector for embedding-based search.

use async_trait::async_trait;
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use zoey_core::observability::types::LLMCostRecord;
use zoey_core::{types::*, Result, ZoeyError};

/// Supabase configuration
#[derive(Debug, Clone)]
pub struct SupabaseConfig {
    /// Supabase project URL (e.g., https://xxx.supabase.co)
    pub url: String,
    /// Supabase anon/service key
    pub api_key: String,
    /// Use service role key for admin operations
    pub use_service_role: bool,
}

impl SupabaseConfig {
    /// Create a new Supabase configuration
    pub fn new(url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            api_key: api_key.into(),
            use_service_role: false,
        }
    }

    /// Use service role key for admin operations
    pub fn with_service_role(mut self) -> Self {
        self.use_service_role = true;
        self
    }
}

/// Supabase database adapter using PostgREST API
pub struct SupabaseAdapter {
    config: SupabaseConfig,
    client: Client,
    embedding_dimension: std::sync::RwLock<usize>,
}

impl SupabaseAdapter {
    /// Create a new Supabase adapter
    pub async fn new(config: SupabaseConfig) -> Result<Self> {
        info!("Connecting to Supabase: {}", config.url);

        let mut headers = header::HeaderMap::new();
        headers.insert(
            "apikey",
            header::HeaderValue::from_str(&config.api_key)
                .map_err(|e| ZoeyError::database(format!("Invalid API key: {}", e)))?,
        );
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {}", config.api_key))
                .map_err(|e| ZoeyError::database(format!("Invalid API key: {}", e)))?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "Prefer",
            header::HeaderValue::from_static("return=representation"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| ZoeyError::database(format!("Failed to create HTTP client: {}", e)))?;

        // Test connection
        let health_url = format!("{}/rest/v1/", config.url);
        client
            .get(&health_url)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to connect to Supabase: {}", e)))?;

        info!("Successfully connected to Supabase");

        Ok(Self {
            config,
            client,
            embedding_dimension: std::sync::RwLock::new(1536),
        })
    }

    /// Get the REST API URL for a table
    fn table_url(&self, table: &str) -> String {
        format!("{}/rest/v1/{}", self.config.url, table)
    }

    /// Get the RPC URL for a function
    fn rpc_url(&self, function: &str) -> String {
        format!("{}/rest/v1/rpc/{}", self.config.url, function)
    }

    /// Execute a SELECT query
    async fn select<T: for<'de> Deserialize<'de>>(
        &self,
        table: &str,
        query: &str,
    ) -> Result<Vec<T>> {
        let url = format!("{}?{}", self.table_url(table), query);
        
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Supabase query failed ({}): {}",
                status, body
            )));
        }

        response
            .json()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to parse response: {}", e)))
    }

    /// Execute an INSERT query
    async fn insert<T: Serialize>(&self, table: &str, data: &T) -> Result<()> {
        let url = self.table_url(table);

        let response = self
            .client
            .post(&url)
            .json(data)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Insert failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Supabase insert failed ({}): {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Execute an UPSERT query
    async fn upsert<T: Serialize>(&self, table: &str, data: &T, on_conflict: &str) -> Result<()> {
        let url = self.table_url(table);

        let response = self
            .client
            .post(&url)
            .header("Prefer", format!("resolution=merge-duplicates,return=representation"))
            .header("On-Conflict", on_conflict)
            .json(data)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Upsert failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Supabase upsert failed ({}): {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Execute an UPDATE query
    async fn update<T: Serialize>(&self, table: &str, filter: &str, data: &T) -> Result<bool> {
        let url = format!("{}?{}", self.table_url(table), filter);

        let response = self
            .client
            .patch(&url)
            .json(data)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Update failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Supabase update failed ({}): {}",
                status, body
            )));
        }

        Ok(true)
    }

    /// Execute a DELETE query
    async fn delete(&self, table: &str, filter: &str) -> Result<bool> {
        let url = format!("{}?{}", self.table_url(table), filter);

        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Delete failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Supabase delete failed ({}): {}",
                status, body
            )));
        }

        Ok(true)
    }

    /// Call an RPC function
    async fn rpc<T: for<'de> Deserialize<'de>, P: Serialize>(
        &self,
        function: &str,
        params: &P,
    ) -> Result<T> {
        let url = self.rpc_url(function);

        let response = self
            .client
            .post(&url)
            .json(params)
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("RPC call failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ZoeyError::database(format!(
                "Supabase RPC failed ({}): {}",
                status, body
            )));
        }

        response
            .json()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to parse RPC response: {}", e)))
    }
}

// Supabase row types for serialization
#[derive(Debug, Serialize, Deserialize)]
struct AgentRow {
    id: String,
    name: String,
    character: serde_json::Value,
    created_at: Option<i64>,
    updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntityRow {
    id: String,
    agent_id: String,
    name: Option<String>,
    username: Option<String>,
    email: Option<String>,
    avatar_url: Option<String>,
    metadata: serde_json::Value,
    created_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MemoryRow {
    id: String,
    entity_id: String,
    agent_id: String,
    room_id: String,
    content: serde_json::Value,
    embedding: Option<Vec<f32>>,
    metadata: Option<serde_json::Value>,
    created_at: i64,
    unique_flag: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorldRow {
    id: String,
    name: String,
    agent_id: String,
    server_id: Option<String>,
    metadata: serde_json::Value,
    created_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RoomRow {
    id: String,
    agent_id: Option<String>,
    name: String,
    source: String,
    #[serde(rename = "type")]
    channel_type: String,
    channel_id: Option<String>,
    server_id: Option<String>,
    world_id: String,
    metadata: serde_json::Value,
    created_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ParticipantRow {
    entity_id: String,
    room_id: String,
    joined_at: i64,
    metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct RelationshipRow {
    entity_id_a: String,
    entity_id_b: String,
    #[serde(rename = "type")]
    relationship_type: String,
    agent_id: String,
    metadata: serde_json::Value,
    created_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ComponentRow {
    id: String,
    entity_id: String,
    world_id: String,
    source_entity_id: Option<String>,
    #[serde(rename = "type")]
    component_type: String,
    data: serde_json::Value,
    created_at: Option<i64>,
    updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TaskRow {
    id: String,
    agent_id: String,
    task_type: String,
    data: serde_json::Value,
    status: String,
    priority: i32,
    scheduled_at: Option<i64>,
    executed_at: Option<i64>,
    retry_count: i32,
    max_retries: i32,
    error: Option<String>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LogRow {
    id: String,
    entity_id: String,
    room_id: Option<String>,
    body: serde_json::Value,
    #[serde(rename = "type")]
    log_type: String,
    created_at: i64,
}

#[async_trait]
impl IDatabaseAdapter for SupabaseAdapter {
    fn db(&self) -> &dyn std::any::Any {
        &self.client
    }

    async fn initialize(&mut self, _config: Option<serde_json::Value>) -> Result<()> {
        // Supabase tables should be created via migrations
        // This just verifies connection
        info!("Supabase adapter initialized (tables should be created via migrations)");
        Ok(())
    }

    async fn is_ready(&self) -> Result<bool> {
        let url = format!("{}/rest/v1/", self.config.url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }

    async fn get_connection(&self) -> Result<Box<dyn std::any::Any + Send>> {
        Ok(Box::new(self.client.clone()))
    }

    async fn run_plugin_migrations(
        &self,
        plugins: Vec<PluginMigration>,
        options: MigrationOptions,
    ) -> Result<()> {
        if plugins.is_empty() {
            return Ok(());
        }

        // Supabase migrations should be run via SQL migrations
        // This is a no-op that logs what needs to be done
        for plugin in plugins {
            if let Some(schema) = plugin.schema {
                if options.verbose {
                    info!(
                        "Plugin '{}' schema should be created via Supabase migrations: {:?}",
                        plugin.name, schema
                    );
                }
            }
        }

        warn!("Supabase plugin migrations should be applied via SQL migrations");
        Ok(())
    }

    // Agent operations
    async fn get_agent(&self, agent_id: UUID) -> Result<Option<Agent>> {
        let query = format!("id=eq.{}", agent_id);
        let rows: Vec<AgentRow> = self.select("agents", &query).await?;

        Ok(rows.into_iter().next().map(|row| Agent {
            id: uuid::Uuid::parse_str(&row.id).unwrap_or_default(),
            name: row.name,
            character: row.character,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }

    async fn get_agents(&self) -> Result<Vec<Agent>> {
        let rows: Vec<AgentRow> = self.select("agents", "select=*").await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(Agent {
                    id: uuid::Uuid::parse_str(&row.id).ok()?,
                    name: row.name,
                    character: row.character,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                })
            })
            .collect())
    }

    async fn create_agent(&self, agent: &Agent) -> Result<bool> {
        let row = AgentRow {
            id: agent.id.to_string(),
            name: agent.name.clone(),
            character: agent.character.clone(),
            created_at: agent.created_at.or(Some(chrono::Utc::now().timestamp())),
            updated_at: agent.updated_at,
        };

        self.insert("agents", &row).await?;
        Ok(true)
    }

    async fn update_agent(&self, agent_id: UUID, agent: &Agent) -> Result<bool> {
        #[derive(Serialize)]
        struct UpdateAgent {
            name: String,
            character: serde_json::Value,
            updated_at: i64,
        }

        let update = UpdateAgent {
            name: agent.name.clone(),
            character: agent.character.clone(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        let filter = format!("id=eq.{}", agent_id);
        self.update("agents", &filter, &update).await
    }

    async fn delete_agent(&self, agent_id: UUID) -> Result<bool> {
        let filter = format!("id=eq.{}", agent_id);
        self.delete("agents", &filter).await
    }

    async fn ensure_embedding_dimension(&self, dimension: usize) -> Result<()> {
        *self.embedding_dimension.write().unwrap() = dimension;
        Ok(())
    }

    async fn get_entities_by_ids(&self, entity_ids: Vec<UUID>) -> Result<Vec<Entity>> {
        if entity_ids.is_empty() {
            return Ok(vec![]);
        }

        let ids: Vec<String> = entity_ids.iter().map(|id| id.to_string()).collect();
        let query = format!("id=in.({})", ids.join(","));
        let rows: Vec<EntityRow> = self.select("entities", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| self.entity_row_to_entity(row).ok())
            .collect())
    }

    async fn get_entities_for_room(
        &self,
        room_id: UUID,
        _include_components: bool,
    ) -> Result<Vec<Entity>> {
        let query = format!("room_id=eq.{}&select=entity_id", room_id);
        let participants: Vec<ParticipantRow> = self.select("participants", &query).await?;

        let entity_ids: Vec<UUID> = participants
            .iter()
            .filter_map(|p| uuid::Uuid::parse_str(&p.entity_id).ok())
            .collect();

        self.get_entities_by_ids(entity_ids).await
    }

    async fn create_entities(&self, entities: Vec<Entity>) -> Result<bool> {
        for entity in entities {
            let row = EntityRow {
                id: entity.id.to_string(),
                agent_id: entity.agent_id.to_string(),
                name: entity.name,
                username: entity.username,
                email: entity.email,
                avatar_url: entity.avatar_url,
                metadata: serde_json::to_value(&entity.metadata).unwrap_or_default(),
                created_at: entity.created_at,
            };

            self.upsert("entities", &row, "id").await?;
        }
        Ok(true)
    }

    async fn update_entity(&self, entity: &Entity) -> Result<()> {
        #[derive(Serialize)]
        struct UpdateEntity {
            name: Option<String>,
            username: Option<String>,
            email: Option<String>,
            avatar_url: Option<String>,
            metadata: serde_json::Value,
        }

        let update = UpdateEntity {
            name: entity.name.clone(),
            username: entity.username.clone(),
            email: entity.email.clone(),
            avatar_url: entity.avatar_url.clone(),
            metadata: serde_json::to_value(&entity.metadata).unwrap_or_default(),
        };

        let filter = format!("id=eq.{}", entity.id);
        self.update("entities", &filter, &update).await?;
        Ok(())
    }

    async fn get_entity_by_id(&self, entity_id: UUID) -> Result<Option<Entity>> {
        let query = format!("id=eq.{}", entity_id);
        let rows: Vec<EntityRow> = self.select("entities", &query).await?;

        Ok(rows
            .into_iter()
            .next()
            .and_then(|row| self.entity_row_to_entity(row).ok()))
    }

    async fn get_component(
        &self,
        entity_id: UUID,
        component_type: &str,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Option<Component>> {
        let mut query = format!(
            "entity_id=eq.{}&type=eq.{}",
            entity_id,
            urlencoding::encode(component_type)
        );

        if let Some(wid) = world_id {
            query.push_str(&format!("&world_id=eq.{}", wid));
        }
        if let Some(seid) = source_entity_id {
            query.push_str(&format!("&source_entity_id=eq.{}", seid));
        }

        let rows: Vec<ComponentRow> = self.select("components", &query).await?;

        Ok(rows
            .into_iter()
            .next()
            .and_then(|row| self.component_row_to_component(row).ok()))
    }

    async fn get_components(
        &self,
        entity_id: UUID,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Vec<Component>> {
        let mut query = format!("entity_id=eq.{}", entity_id);

        if let Some(wid) = world_id {
            query.push_str(&format!("&world_id=eq.{}", wid));
        }
        if let Some(seid) = source_entity_id {
            query.push_str(&format!("&source_entity_id=eq.{}", seid));
        }

        let rows: Vec<ComponentRow> = self.select("components", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| self.component_row_to_component(row).ok())
            .collect())
    }

    async fn create_component(&self, component: &Component) -> Result<bool> {
        let row = ComponentRow {
            id: component.id.to_string(),
            entity_id: component.entity_id.to_string(),
            world_id: component.world_id.to_string(),
            source_entity_id: component.source_entity_id.map(|id| id.to_string()),
            component_type: component.component_type.clone(),
            data: component.data.clone(),
            created_at: component.created_at,
            updated_at: component.updated_at,
        };

        self.insert("components", &row).await?;
        Ok(true)
    }

    async fn update_component(&self, component: &Component) -> Result<()> {
        #[derive(Serialize)]
        struct UpdateComponent {
            data: serde_json::Value,
            updated_at: i64,
        }

        let update = UpdateComponent {
            data: component.data.clone(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        let filter = format!("id=eq.{}", component.id);
        self.update("components", &filter, &update).await?;
        Ok(())
    }

    async fn delete_component(&self, component_id: UUID) -> Result<()> {
        let filter = format!("id=eq.{}", component_id);
        self.delete("components", &filter).await?;
        Ok(())
    }

    async fn get_memories(&self, params: MemoryQuery) -> Result<Vec<Memory>> {
        let mut filters = Vec::new();

        if let Some(agent_id) = params.agent_id {
            filters.push(format!("agent_id=eq.{}", agent_id));
        }
        if let Some(room_id) = params.room_id {
            filters.push(format!("room_id=eq.{}", room_id));
        }
        if let Some(entity_id) = params.entity_id {
            filters.push(format!("entity_id=eq.{}", entity_id));
        }
        if let Some(unique) = params.unique {
            filters.push(format!("unique_flag=eq.{}", unique));
        }

        filters.push("order=created_at.desc".to_string());

        if let Some(count) = params.count {
            filters.push(format!("limit={}", count));
        }

        let query = filters.join("&");
        let rows: Vec<MemoryRow> = self.select("memories", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| self.memory_row_to_memory(row).ok())
            .collect())
    }

    async fn create_memory(&self, memory: &Memory, _table_name: &str) -> Result<UUID> {
        let row = MemoryRow {
            id: memory.id.to_string(),
            entity_id: memory.entity_id.to_string(),
            agent_id: memory.agent_id.to_string(),
            room_id: memory.room_id.to_string(),
            content: serde_json::to_value(&memory.content).unwrap_or_default(),
            embedding: memory.embedding.clone(),
            metadata: memory.metadata.as_ref().map(|m| serde_json::to_value(m).unwrap_or_default()),
            created_at: memory.created_at,
            unique_flag: memory.unique.unwrap_or(false),
        };

        self.insert("memories", &row).await?;
        Ok(memory.id)
    }

    async fn search_memories_by_embedding(
        &self,
        params: SearchMemoriesParams,
    ) -> Result<Vec<Memory>> {
        // Use Supabase RPC function for vector search
        // This requires a PostgreSQL function like:
        // CREATE FUNCTION match_memories(
        //   query_embedding vector(1536),
        //   match_count int,
        //   filter_agent_id uuid DEFAULT NULL,
        //   filter_room_id uuid DEFAULT NULL
        // ) RETURNS TABLE(...)

        #[derive(Serialize)]
        struct MatchMemoriesParams {
            query_embedding: Vec<f32>,
            match_count: i32,
            filter_agent_id: Option<String>,
            filter_room_id: Option<String>,
        }

        let rpc_params = MatchMemoriesParams {
            query_embedding: params.embedding,
            match_count: params.count as i32,
            filter_agent_id: params.agent_id.map(|id| id.to_string()),
            filter_room_id: params.room_id.map(|id| id.to_string()),
        };

        match self.rpc::<Vec<MemoryRow>, _>("match_memories", &rpc_params).await {
            Ok(rows) => Ok(rows
                .into_iter()
                .filter_map(|row| self.memory_row_to_memory(row).ok())
                .collect()),
            Err(e) => {
                warn!("Vector search RPC failed (function may not exist): {}", e);
                // Fall back to regular query
                let query = MemoryQuery {
                    agent_id: params.agent_id,
                    room_id: params.room_id,
                    entity_id: params.entity_id,
                    world_id: params.world_id,
                    unique: params.unique,
                    count: Some(params.count),
                    offset: None,
                    table_name: params.table_name,
                    start: None,
                    end: None,
                };
                self.get_memories(query).await
            }
        }
    }

    async fn get_cached_embeddings(&self, params: MemoryQuery) -> Result<Vec<Memory>> {
        let mut filters = vec!["embedding=not.is.null".to_string()];

        if let Some(agent_id) = params.agent_id {
            filters.push(format!("agent_id=eq.{}", agent_id));
        }
        if let Some(room_id) = params.room_id {
            filters.push(format!("room_id=eq.{}", room_id));
        }
        if let Some(entity_id) = params.entity_id {
            filters.push(format!("entity_id=eq.{}", entity_id));
        }

        filters.push("order=created_at.desc".to_string());

        if let Some(count) = params.count {
            filters.push(format!("limit={}", count));
        }

        let query = filters.join("&");
        let rows: Vec<MemoryRow> = self.select("memories", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| self.memory_row_to_memory(row).ok())
            .collect())
    }

    async fn update_memory(&self, memory: &Memory) -> Result<bool> {
        #[derive(Serialize)]
        struct UpdateMemory {
            content: serde_json::Value,
            embedding: Option<Vec<f32>>,
            metadata: Option<serde_json::Value>,
        }

        let update = UpdateMemory {
            content: serde_json::to_value(&memory.content).unwrap_or_default(),
            embedding: memory.embedding.clone(),
            metadata: memory.metadata.as_ref().map(|m| serde_json::to_value(m).unwrap_or_default()),
        };

        let filter = format!("id=eq.{}", memory.id);
        self.update("memories", &filter, &update).await
    }

    async fn remove_memory(&self, memory_id: UUID, _table_name: &str) -> Result<bool> {
        let filter = format!("id=eq.{}", memory_id);
        self.delete("memories", &filter).await
    }

    async fn remove_all_memories(&self, agent_id: UUID, _table_name: &str) -> Result<bool> {
        let filter = format!("agent_id=eq.{}", agent_id);
        self.delete("memories", &filter).await
    }

    async fn count_memories(&self, params: MemoryQuery) -> Result<usize> {
        let mut filters = Vec::new();

        if let Some(agent_id) = params.agent_id {
            filters.push(format!("agent_id=eq.{}", agent_id));
        }
        if let Some(room_id) = params.room_id {
            filters.push(format!("room_id=eq.{}", room_id));
        }
        if let Some(entity_id) = params.entity_id {
            filters.push(format!("entity_id=eq.{}", entity_id));
        }
        if let Some(unique) = params.unique {
            filters.push(format!("unique_flag=eq.{}", unique));
        }

        filters.push("select=count".to_string());

        let query = filters.join("&");
        let url = format!("{}?{}", self.table_url("memories"), query);

        let response = self
            .client
            .get(&url)
            .header("Prefer", "count=exact")
            .send()
            .await
            .map_err(|e| ZoeyError::database(format!("Count query failed: {}", e)))?;

        let count = response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split('/').last())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        Ok(count)
    }

    async fn get_world(&self, world_id: UUID) -> Result<Option<World>> {
        let query = format!("id=eq.{}", world_id);
        let rows: Vec<WorldRow> = self.select("worlds", &query).await?;

        Ok(rows.into_iter().next().and_then(|row| self.world_row_to_world(row).ok()))
    }

    async fn ensure_world(&self, world: &World) -> Result<()> {
        let row = WorldRow {
            id: world.id.to_string(),
            name: world.name.clone(),
            agent_id: world.agent_id.to_string(),
            server_id: world.server_id.clone(),
            metadata: serde_json::to_value(&world.metadata).unwrap_or_default(),
            created_at: world.created_at.or(Some(chrono::Utc::now().timestamp())),
        };

        self.upsert("worlds", &row, "id").await
    }

    async fn get_room(&self, room_id: UUID) -> Result<Option<Room>> {
        let query = format!("id=eq.{}", room_id);
        let rows: Vec<RoomRow> = self.select("rooms", &query).await?;

        Ok(rows.into_iter().next().and_then(|row| self.room_row_to_room(row).ok()))
    }

    async fn create_room(&self, room: &Room) -> Result<UUID> {
        let channel_type_str = match room.channel_type {
            ChannelType::Dm => "DM",
            ChannelType::VoiceDm => "VOICE_DM",
            ChannelType::GroupDm => "GROUP_DM",
            ChannelType::GuildText => "GUILD_TEXT",
            ChannelType::GuildVoice => "GUILD_VOICE",
            ChannelType::Thread => "THREAD",
            ChannelType::Feed => "FEED",
            ChannelType::SelfChannel => "SELF",
            ChannelType::Api => "API",
            ChannelType::World => "WORLD",
            ChannelType::Unknown => "UNKNOWN",
        };

        let row = RoomRow {
            id: room.id.to_string(),
            agent_id: room.agent_id.map(|id| id.to_string()),
            name: room.name.clone(),
            source: room.source.clone(),
            channel_type: channel_type_str.to_string(),
            channel_id: room.channel_id.clone(),
            server_id: room.server_id.clone(),
            world_id: room.world_id.to_string(),
            metadata: serde_json::to_value(&room.metadata).unwrap_or_default(),
            created_at: room.created_at.or(Some(chrono::Utc::now().timestamp())),
        };

        self.insert("rooms", &row).await?;
        Ok(room.id)
    }

    async fn get_rooms(&self, world_id: UUID) -> Result<Vec<Room>> {
        let query = format!("world_id=eq.{}", world_id);
        let rows: Vec<RoomRow> = self.select("rooms", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| self.room_row_to_room(row).ok())
            .collect())
    }

    async fn get_rooms_for_agent(&self, agent_id: UUID) -> Result<Vec<Room>> {
        let query = format!("agent_id=eq.{}", agent_id);
        let rows: Vec<RoomRow> = self.select("rooms", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| self.room_row_to_room(row).ok())
            .collect())
    }

    async fn add_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let row = ParticipantRow {
            entity_id: entity_id.to_string(),
            room_id: room_id.to_string(),
            joined_at: chrono::Utc::now().timestamp(),
            metadata: serde_json::json!({}),
        };

        self.upsert("participants", &row, "entity_id,room_id").await?;
        Ok(true)
    }

    async fn remove_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let filter = format!("entity_id=eq.{}&room_id=eq.{}", entity_id, room_id);
        self.delete("participants", &filter).await
    }

    async fn get_participants(&self, room_id: UUID) -> Result<Vec<Participant>> {
        let query = format!("room_id=eq.{}", room_id);
        let rows: Vec<ParticipantRow> = self.select("participants", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(Participant {
                    entity_id: uuid::Uuid::parse_str(&row.entity_id).ok()?,
                    room_id: uuid::Uuid::parse_str(&row.room_id).ok()?,
                    joined_at: Some(row.joined_at),
                    metadata: serde_json::from_value(row.metadata).unwrap_or_default(),
                })
            })
            .collect())
    }

    async fn create_relationship(&self, relationship: &Relationship) -> Result<bool> {
        let row = RelationshipRow {
            entity_id_a: relationship.entity_id_a.to_string(),
            entity_id_b: relationship.entity_id_b.to_string(),
            relationship_type: relationship.relationship_type.clone(),
            agent_id: relationship.agent_id.to_string(),
            metadata: serde_json::to_value(&relationship.metadata).unwrap_or_default(),
            created_at: relationship.created_at.or(Some(chrono::Utc::now().timestamp())),
        };

        self.upsert("relationships", &row, "entity_id_a,entity_id_b,type")
            .await?;
        Ok(true)
    }

    async fn get_relationship(
        &self,
        entity_id_a: UUID,
        entity_id_b: UUID,
    ) -> Result<Option<Relationship>> {
        let query = format!(
            "or=(and(entity_id_a.eq.{},entity_id_b.eq.{}),and(entity_id_a.eq.{},entity_id_b.eq.{}))",
            entity_id_a, entity_id_b, entity_id_b, entity_id_a
        );
        let rows: Vec<RelationshipRow> = self.select("relationships", &query).await?;

        Ok(rows.into_iter().next().and_then(|row| {
            Some(Relationship {
                entity_id_a: uuid::Uuid::parse_str(&row.entity_id_a).ok()?,
                entity_id_b: uuid::Uuid::parse_str(&row.entity_id_b).ok()?,
                relationship_type: row.relationship_type,
                agent_id: uuid::Uuid::parse_str(&row.agent_id).ok()?,
                metadata: serde_json::from_value(row.metadata).unwrap_or_default(),
                created_at: row.created_at,
            })
        }))
    }

    async fn create_task(&self, task: &Task) -> Result<UUID> {
        let status_str = match task.status {
            TaskStatus::Pending => "PENDING",
            TaskStatus::Running => "RUNNING",
            TaskStatus::Completed => "COMPLETED",
            TaskStatus::Failed => "FAILED",
            TaskStatus::Cancelled => "CANCELLED",
        };

        let row = TaskRow {
            id: task.id.to_string(),
            agent_id: task.agent_id.to_string(),
            task_type: task.task_type.clone(),
            data: task.data.clone(),
            status: status_str.to_string(),
            priority: task.priority,
            scheduled_at: task.scheduled_at,
            executed_at: task.executed_at,
            retry_count: task.retry_count,
            max_retries: task.max_retries,
            error: task.error.clone(),
            created_at: Some(task.created_at),
            updated_at: task.updated_at,
        };

        self.insert("tasks", &row).await?;
        Ok(task.id)
    }

    async fn update_task(&self, task: &Task) -> Result<bool> {
        let status_str = match task.status {
            TaskStatus::Pending => "PENDING",
            TaskStatus::Running => "RUNNING",
            TaskStatus::Completed => "COMPLETED",
            TaskStatus::Failed => "FAILED",
            TaskStatus::Cancelled => "CANCELLED",
        };

        #[derive(Serialize)]
        struct UpdateTask {
            data: serde_json::Value,
            status: String,
            priority: i32,
            scheduled_at: Option<i64>,
            executed_at: Option<i64>,
            retry_count: i32,
            max_retries: i32,
            error: Option<String>,
            updated_at: i64,
        }

        let update = UpdateTask {
            data: task.data.clone(),
            status: status_str.to_string(),
            priority: task.priority,
            scheduled_at: task.scheduled_at,
            executed_at: task.executed_at,
            retry_count: task.retry_count,
            max_retries: task.max_retries,
            error: task.error.clone(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        let filter = format!("id=eq.{}", task.id);
        self.update("tasks", &filter, &update).await
    }

    async fn get_task(&self, task_id: UUID) -> Result<Option<Task>> {
        let query = format!("id=eq.{}", task_id);
        let rows: Vec<TaskRow> = self.select("tasks", &query).await?;

        Ok(rows.into_iter().next().and_then(|row| self.task_row_to_task(row).ok()))
    }

    async fn get_pending_tasks(&self, agent_id: UUID) -> Result<Vec<Task>> {
        let query = format!(
            "agent_id=eq.{}&status=eq.PENDING&order=scheduled_at.asc.nullslast,created_at.asc",
            agent_id
        );
        let rows: Vec<TaskRow> = self.select("tasks", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| self.task_row_to_task(row).ok())
            .collect())
    }

    async fn log(&self, log: &Log) -> Result<()> {
        let id = log.id.unwrap_or_else(uuid::Uuid::new_v4);

        let row = LogRow {
            id: id.to_string(),
            entity_id: log.entity_id.to_string(),
            room_id: log.room_id.map(|id| id.to_string()),
            body: log.body.clone(),
            log_type: log.log_type.clone(),
            created_at: log.created_at,
        };

        self.insert("logs", &row).await
    }

    async fn get_logs(&self, params: LogQuery) -> Result<Vec<Log>> {
        let mut filters = Vec::new();

        if let Some(entity_id) = params.entity_id {
            filters.push(format!("entity_id=eq.{}", entity_id));
        }
        if let Some(room_id) = params.room_id {
            filters.push(format!("room_id=eq.{}", room_id));
        }
        if let Some(log_type) = params.log_type {
            filters.push(format!("type=eq.{}", urlencoding::encode(&log_type)));
        }

        filters.push("order=created_at.desc".to_string());

        if let Some(limit) = params.limit {
            filters.push(format!("limit={}", limit));
        }
        if let Some(offset) = params.offset {
            filters.push(format!("offset={}", offset));
        }

        let query = filters.join("&");
        let rows: Vec<LogRow> = self.select("logs", &query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(Log {
                    id: uuid::Uuid::parse_str(&row.id).ok(),
                    entity_id: uuid::Uuid::parse_str(&row.entity_id).ok()?,
                    room_id: row.room_id.and_then(|id| uuid::Uuid::parse_str(&id).ok()),
                    body: row.body,
                    log_type: row.log_type,
                    created_at: row.created_at,
                })
            })
            .collect())
    }

    async fn get_agent_run_summaries(
        &self,
        params: RunSummaryQuery,
    ) -> Result<AgentRunSummaryResult> {
        // Supabase run summaries would require custom RPC function
        Ok(AgentRunSummaryResult {
            runs: vec![],
            total: 0,
            has_more: false,
        })
    }

    async fn persist_llm_cost(&self, record: LLMCostRecord) -> Result<()> {
        #[derive(Serialize)]
        struct LLMCostRow {
            id: String,
            timestamp: i64,
            agent_id: String,
            user_id: Option<String>,
            conversation_id: Option<String>,
            action_name: Option<String>,
            evaluator_name: Option<String>,
            provider: String,
            model: String,
            temperature: f32,
            prompt_tokens: i64,
            completion_tokens: i64,
            total_tokens: i64,
            cached_tokens: Option<i64>,
            input_cost_usd: f64,
            output_cost_usd: f64,
            total_cost_usd: f64,
            latency_ms: i64,
            ttft_ms: Option<i64>,
            success: bool,
            error: Option<String>,
            prompt_hash: Option<String>,
            prompt_preview: Option<String>,
        }

        let row = LLMCostRow {
            id: record.id.to_string(),
            timestamp: record.timestamp.timestamp(),
            agent_id: record.agent_id.to_string(),
            user_id: record.user_id,
            conversation_id: record.conversation_id.map(|id| id.to_string()),
            action_name: record.action_name,
            evaluator_name: record.evaluator_name,
            provider: record.provider,
            model: record.model,
            temperature: record.temperature,
            prompt_tokens: record.prompt_tokens as i64,
            completion_tokens: record.completion_tokens as i64,
            total_tokens: record.total_tokens as i64,
            cached_tokens: record.cached_tokens.map(|t| t as i64),
            input_cost_usd: record.input_cost_usd,
            output_cost_usd: record.output_cost_usd,
            total_cost_usd: record.total_cost_usd,
            latency_ms: record.latency_ms as i64,
            ttft_ms: record.ttft_ms.map(|t| t as i64),
            success: record.success,
            error: record.error,
            prompt_hash: record.prompt_hash,
            prompt_preview: record.prompt_preview,
        };

        self.insert("llm_costs", &row).await
    }
}

// Helper methods for row conversion
impl SupabaseAdapter {
    fn entity_row_to_entity(&self, row: EntityRow) -> Result<Entity> {
        Ok(Entity {
            id: uuid::Uuid::parse_str(&row.id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            agent_id: uuid::Uuid::parse_str(&row.agent_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            name: row.name,
            username: row.username,
            email: row.email,
            avatar_url: row.avatar_url,
            metadata: serde_json::from_value(row.metadata).unwrap_or_default(),
            created_at: row.created_at,
        })
    }

    fn component_row_to_component(&self, row: ComponentRow) -> Result<Component> {
        Ok(Component {
            id: uuid::Uuid::parse_str(&row.id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            entity_id: uuid::Uuid::parse_str(&row.entity_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            world_id: uuid::Uuid::parse_str(&row.world_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            source_entity_id: row
                .source_entity_id
                .and_then(|id| uuid::Uuid::parse_str(&id).ok()),
            component_type: row.component_type,
            data: row.data,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    fn memory_row_to_memory(&self, row: MemoryRow) -> Result<Memory> {
        Ok(Memory {
            id: uuid::Uuid::parse_str(&row.id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            entity_id: uuid::Uuid::parse_str(&row.entity_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            agent_id: uuid::Uuid::parse_str(&row.agent_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            room_id: uuid::Uuid::parse_str(&row.room_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            content: serde_json::from_value(row.content).unwrap_or_default(),
            embedding: row.embedding,
            metadata: row.metadata.and_then(|m| serde_json::from_value(m).ok()),
            created_at: row.created_at,
            unique: Some(row.unique_flag),
            similarity: None,
        })
    }

    fn world_row_to_world(&self, row: WorldRow) -> Result<World> {
        Ok(World {
            id: uuid::Uuid::parse_str(&row.id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            name: row.name,
            agent_id: uuid::Uuid::parse_str(&row.agent_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            server_id: row.server_id,
            metadata: serde_json::from_value(row.metadata).unwrap_or_default(),
            created_at: row.created_at,
        })
    }

    fn room_row_to_room(&self, row: RoomRow) -> Result<Room> {
        let channel_type = match row.channel_type.as_str() {
            "DM" => ChannelType::Dm,
            "VOICE_DM" => ChannelType::VoiceDm,
            "GROUP_DM" => ChannelType::GroupDm,
            "GUILD_TEXT" => ChannelType::GuildText,
            "GUILD_VOICE" => ChannelType::GuildVoice,
            "THREAD" => ChannelType::Thread,
            "FEED" => ChannelType::Feed,
            "SELF" => ChannelType::SelfChannel,
            "API" => ChannelType::Api,
            "WORLD" => ChannelType::World,
            _ => ChannelType::Unknown,
        };

        Ok(Room {
            id: uuid::Uuid::parse_str(&row.id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            agent_id: row.agent_id.and_then(|id| uuid::Uuid::parse_str(&id).ok()),
            name: row.name,
            source: row.source,
            channel_type,
            channel_id: row.channel_id,
            server_id: row.server_id,
            world_id: uuid::Uuid::parse_str(&row.world_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            metadata: serde_json::from_value(row.metadata).unwrap_or_default(),
            created_at: row.created_at,
        })
    }

    fn task_row_to_task(&self, row: TaskRow) -> Result<Task> {
        let status = match row.status.as_str() {
            "PENDING" => TaskStatus::Pending,
            "RUNNING" => TaskStatus::Running,
            "COMPLETED" => TaskStatus::Completed,
            "FAILED" => TaskStatus::Failed,
            "CANCELLED" => TaskStatus::Cancelled,
            _ => TaskStatus::Pending,
        };

        Ok(Task {
            id: uuid::Uuid::parse_str(&row.id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            agent_id: uuid::Uuid::parse_str(&row.agent_id)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
            task_type: row.task_type,
            data: row.data,
            status,
            priority: row.priority,
            scheduled_at: row.scheduled_at,
            executed_at: row.executed_at,
            retry_count: row.retry_count,
            max_retries: row.max_retries,
            error: row.error,
            created_at: row.created_at.unwrap_or(0),
            updated_at: row.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supabase_config() {
        let config = SupabaseConfig::new("https://xxx.supabase.co", "anon_key");
        assert_eq!(config.url, "https://xxx.supabase.co");
        assert!(!config.use_service_role);

        let config = config.with_service_role();
        assert!(config.use_service_role);
    }
}
