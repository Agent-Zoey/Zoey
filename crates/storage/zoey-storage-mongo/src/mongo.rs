//! MongoDB database adapter
//!
//! Implements core database operations for MongoDB with Atlas Search support for vector search.

use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::{
    bson::{doc, oid::ObjectId, to_bson, Bson, Document},
    options::{ClientOptions, FindOptions, IndexOptions, UpdateOptions},
    Client, Collection, Database, IndexModel,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use zoey_core::observability::types::LLMCostRecord;
use zoey_core::{types::*, Result, ZoeyError};

/// MongoDB database adapter
pub struct MongoAdapter {
    db: Database,
    client: Client,
    embedding_dimension: std::sync::RwLock<usize>,
}

impl MongoAdapter {
    /// Create a new MongoDB adapter
    pub async fn new(connection_string: &str, database_name: &str) -> Result<Self> {
        info!("Connecting to MongoDB database: {}", database_name);

        let client_options = ClientOptions::parse(connection_string)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to parse MongoDB URI: {}", e)))?;

        let client = Client::with_options(client_options)
            .map_err(|e| ZoeyError::database(format!("Failed to create MongoDB client: {}", e)))?;

        let db = client.database(database_name);

        // Ping the database to verify connection
        db.run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to connect to MongoDB: {}", e)))?;

        info!("Successfully connected to MongoDB");

        Ok(Self {
            db,
            client,
            embedding_dimension: std::sync::RwLock::new(1536), // Default OpenAI embedding dimension
        })
    }

    /// Get the database instance
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Get the client instance
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get a collection by name
    fn collection<T>(&self, name: &str) -> Collection<T>
    where
        T: Send + Sync,
    {
        self.db.collection(name)
    }

    /// Initialize database schema (collections and indexes)
    async fn init_schema(&self) -> Result<()> {
        debug!("Initializing MongoDB schema...");

        // Create collections and indexes
        self.create_agents_indexes().await?;
        self.create_entities_indexes().await?;
        self.create_worlds_indexes().await?;
        self.create_rooms_indexes().await?;
        self.create_memories_indexes().await?;
        self.create_participants_indexes().await?;
        self.create_relationships_indexes().await?;
        self.create_components_indexes().await?;
        self.create_tasks_indexes().await?;
        self.create_logs_indexes().await?;
        self.create_llm_costs_indexes().await?;

        info!("MongoDB schema initialized successfully");
        Ok(())
    }

    async fn create_agents_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("agents");
        let indexes = vec![IndexModel::builder()
            .keys(doc! { "name": 1 })
            .options(IndexOptions::builder().build())
            .build()];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_entities_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("entities");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "agent_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "username": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_worlds_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("worlds");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "agent_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "server_id": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_rooms_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("rooms");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "world_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "agent_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "channel_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "source": 1, "server_id": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_memories_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("memories");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "agent_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "room_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "entity_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "created_at": -1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "agent_id": 1, "room_id": 1, "created_at": -1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "agent_id": 1, "unique_flag": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_participants_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("participants");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "entity_id": 1, "room_id": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
            IndexModel::builder()
                .keys(doc! { "room_id": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_relationships_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("relationships");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "entity_id_a": 1, "entity_id_b": 1, "type": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
            IndexModel::builder()
                .keys(doc! { "agent_id": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_components_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("components");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "entity_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "world_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "type": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "entity_id": 1, "type": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_tasks_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("tasks");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "agent_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "status": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "status": 1, "scheduled_at": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "agent_id": 1, "status": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_logs_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("logs");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "entity_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "room_id": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "type": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "created_at": -1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "entity_id": 1, "created_at": -1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    async fn create_llm_costs_indexes(&self) -> Result<()> {
        let collection = self.collection::<Document>("llm_costs");
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "agent_id": 1, "timestamp": -1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "provider": 1, "model": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "timestamp": -1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "conversation_id": 1 })
                .build(),
        ];
        collection.create_indexes(indexes).await.ok();
        Ok(())
    }

    /// Convert UUID to BSON string
    fn uuid_to_bson(id: uuid::Uuid) -> Bson {
        Bson::String(id.to_string())
    }

    /// Parse UUID from BSON
    fn bson_to_uuid(bson: &Bson) -> Result<uuid::Uuid> {
        match bson {
            Bson::String(s) => uuid::Uuid::parse_str(s)
                .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e))),
            _ => Err(ZoeyError::database("Expected string for UUID")),
        }
    }
}

#[async_trait]
impl IDatabaseAdapter for MongoAdapter {
    fn db(&self) -> &dyn std::any::Any {
        &self.db
    }

    async fn initialize(&mut self, _config: Option<serde_json::Value>) -> Result<()> {
        self.init_schema().await
    }

    async fn is_ready(&self) -> Result<bool> {
        match self.db.run_command(doc! { "ping": 1 }).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn close(&mut self) -> Result<()> {
        // MongoDB client handles connection cleanup automatically
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

        for plugin in plugins {
            if let Some(schema) = plugin.schema {
                if options.verbose {
                    info!("Applying schema for plugin '{}' (MongoDB)", plugin.name);
                }

                // In MongoDB, we just need to create indexes based on the schema
                let schema_obj = schema.as_object().ok_or_else(|| {
                    ZoeyError::validation(format!(
                        "Invalid schema for plugin '{}': expected JSON object",
                        plugin.name
                    ))
                })?;

                for (collection_name, _table_def) in schema_obj.iter() {
                    if !options.dry_run {
                        // Collection is created automatically when first document is inserted
                        // Just ensure the collection exists
                        self.db.create_collection(collection_name).await.ok();
                    }
                }

                if options.verbose {
                    info!("✓ Schema applied for plugin '{}' (MongoDB)", plugin.name);
                }
            }
        }
        Ok(())
    }

    // Agent operations
    async fn get_agent(&self, agent_id: UUID) -> Result<Option<Agent>> {
        let collection = self.collection::<Document>("agents");
        let filter = doc! { "_id": agent_id.to_string() };

        let result = collection
            .find_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get agent: {}", e)))?;

        match result {
            Some(doc) => {
                let agent = Agent {
                    id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
                    name: doc.get_str("name").unwrap_or("").to_string(),
                    character: mongodb::bson::from_bson(
                        doc.get("character").cloned().unwrap_or(Bson::Null),
                    )
                    .unwrap_or(serde_json::Value::Null),
                    created_at: doc.get_i64("created_at").ok(),
                    updated_at: doc.get_i64("updated_at").ok(),
                };
                Ok(Some(agent))
            }
            None => Ok(None),
        }
    }

    async fn get_agents(&self) -> Result<Vec<Agent>> {
        let collection = self.collection::<Document>("agents");
        let mut cursor = collection
            .find(doc! {})
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get agents: {}", e)))?;

        let mut agents = Vec::new();
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to iterate agents: {}", e)))?
        {
            let agent = Agent {
                id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
                name: doc.get_str("name").unwrap_or("").to_string(),
                character: mongodb::bson::from_bson(
                    doc.get("character").cloned().unwrap_or(Bson::Null),
                )
                .unwrap_or(serde_json::Value::Null),
                created_at: doc.get_i64("created_at").ok(),
                updated_at: doc.get_i64("updated_at").ok(),
            };
            agents.push(agent);
        }

        Ok(agents)
    }

    async fn create_agent(&self, agent: &Agent) -> Result<bool> {
        let collection = self.collection::<Document>("agents");

        let doc = doc! {
            "_id": agent.id.to_string(),
            "name": &agent.name,
            "character": to_bson(&agent.character).unwrap_or(Bson::Null),
            "created_at": agent.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()),
            "updated_at": agent.updated_at,
        };

        collection
            .insert_one(doc)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create agent: {}", e)))?;

        Ok(true)
    }

    async fn update_agent(&self, agent_id: UUID, agent: &Agent) -> Result<bool> {
        let collection = self.collection::<Document>("agents");

        let filter = doc! { "_id": agent_id.to_string() };
        let update = doc! {
            "$set": {
                "name": &agent.name,
                "character": to_bson(&agent.character).unwrap_or(Bson::Null),
                "updated_at": chrono::Utc::now().timestamp(),
            }
        };

        let result = collection
            .update_one(filter, update)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to update agent: {}", e)))?;

        Ok(result.modified_count > 0)
    }

    async fn delete_agent(&self, agent_id: UUID) -> Result<bool> {
        let collection = self.collection::<Document>("agents");
        let filter = doc! { "_id": agent_id.to_string() };

        let result = collection
            .delete_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to delete agent: {}", e)))?;

        Ok(result.deleted_count > 0)
    }

    async fn ensure_embedding_dimension(&self, dimension: usize) -> Result<()> {
        *self.embedding_dimension.write().unwrap() = dimension;
        Ok(())
    }

    async fn get_entities_by_ids(&self, entity_ids: Vec<UUID>) -> Result<Vec<Entity>> {
        if entity_ids.is_empty() {
            return Ok(vec![]);
        }

        let collection = self.collection::<Document>("entities");
        let ids: Vec<String> = entity_ids.iter().map(|id| id.to_string()).collect();
        let filter = doc! { "_id": { "$in": ids } };

        let mut cursor = collection
            .find(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get entities: {}", e)))?;

        let mut entities = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate entities: {}", e))
        })? {
            let entity = self.doc_to_entity(&doc)?;
            entities.push(entity);
        }

        Ok(entities)
    }

    async fn get_entities_for_room(
        &self,
        room_id: UUID,
        _include_components: bool,
    ) -> Result<Vec<Entity>> {
        let participants_collection = self.collection::<Document>("participants");
        let filter = doc! { "room_id": room_id.to_string() };

        let mut cursor = participants_collection
            .find(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get participants: {}", e)))?;

        let mut entity_ids = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate participants: {}", e))
        })? {
            if let Ok(entity_id) = Self::bson_to_uuid(doc.get("entity_id").unwrap_or(&Bson::Null)) {
                entity_ids.push(entity_id);
            }
        }

        self.get_entities_by_ids(entity_ids).await
    }

    async fn create_entities(&self, entities: Vec<Entity>) -> Result<bool> {
        let collection = self.collection::<Document>("entities");

        for entity in entities {
            let doc = doc! {
                "_id": entity.id.to_string(),
                "agent_id": entity.agent_id.to_string(),
                "name": &entity.name,
                "username": &entity.username,
                "email": &entity.email,
                "avatar_url": &entity.avatar_url,
                "metadata": to_bson(&entity.metadata).unwrap_or(Bson::Document(doc! {})),
                "created_at": entity.created_at,
            };

            let options = UpdateOptions::builder().upsert(true).build();
            collection
                .update_one(
                    doc! { "_id": entity.id.to_string() },
                    doc! { "$set": doc },
                )
                .with_options(options)
                .await
                .map_err(|e| ZoeyError::database(format!("Failed to create entity: {}", e)))?;
        }

        Ok(true)
    }

    async fn update_entity(&self, entity: &Entity) -> Result<()> {
        let collection = self.collection::<Document>("entities");

        let filter = doc! { "_id": entity.id.to_string() };
        let update = doc! {
            "$set": {
                "name": &entity.name,
                "username": &entity.username,
                "email": &entity.email,
                "avatar_url": &entity.avatar_url,
                "metadata": to_bson(&entity.metadata).unwrap_or(Bson::Document(doc! {})),
            }
        };

        collection
            .update_one(filter, update)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to update entity: {}", e)))?;

        Ok(())
    }

    async fn get_entity_by_id(&self, entity_id: UUID) -> Result<Option<Entity>> {
        let collection = self.collection::<Document>("entities");
        let filter = doc! { "_id": entity_id.to_string() };

        let result = collection
            .find_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get entity: {}", e)))?;

        match result {
            Some(doc) => Ok(Some(self.doc_to_entity(&doc)?)),
            None => Ok(None),
        }
    }

    async fn get_component(
        &self,
        entity_id: UUID,
        component_type: &str,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Option<Component>> {
        let collection = self.collection::<Document>("components");

        let mut filter = doc! {
            "entity_id": entity_id.to_string(),
            "type": component_type,
        };

        if let Some(wid) = world_id {
            filter.insert("world_id", wid.to_string());
        }
        if let Some(seid) = source_entity_id {
            filter.insert("source_entity_id", seid.to_string());
        }

        let result = collection
            .find_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get component: {}", e)))?;

        match result {
            Some(doc) => Ok(Some(self.doc_to_component(&doc)?)),
            None => Ok(None),
        }
    }

    async fn get_components(
        &self,
        entity_id: UUID,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Vec<Component>> {
        let collection = self.collection::<Document>("components");

        let mut filter = doc! { "entity_id": entity_id.to_string() };

        if let Some(wid) = world_id {
            filter.insert("world_id", wid.to_string());
        }
        if let Some(seid) = source_entity_id {
            filter.insert("source_entity_id", seid.to_string());
        }

        let mut cursor = collection
            .find(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get components: {}", e)))?;

        let mut components = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate components: {}", e))
        })? {
            components.push(self.doc_to_component(&doc)?);
        }

        Ok(components)
    }

    async fn create_component(&self, component: &Component) -> Result<bool> {
        let collection = self.collection::<Document>("components");

        let doc = doc! {
            "_id": component.id.to_string(),
            "entity_id": component.entity_id.to_string(),
            "world_id": component.world_id.to_string(),
            "source_entity_id": component.source_entity_id.map(|id| id.to_string()),
            "type": &component.component_type,
            "data": to_bson(&component.data).unwrap_or(Bson::Document(doc! {})),
            "created_at": component.created_at,
            "updated_at": component.updated_at,
        };

        collection
            .insert_one(doc)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create component: {}", e)))?;

        Ok(true)
    }

    async fn update_component(&self, component: &Component) -> Result<()> {
        let collection = self.collection::<Document>("components");

        let filter = doc! { "_id": component.id.to_string() };
        let update = doc! {
            "$set": {
                "data": to_bson(&component.data).unwrap_or(Bson::Document(doc! {})),
                "updated_at": chrono::Utc::now().timestamp(),
            }
        };

        collection
            .update_one(filter, update)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to update component: {}", e)))?;

        Ok(())
    }

    async fn delete_component(&self, component_id: UUID) -> Result<()> {
        let collection = self.collection::<Document>("components");
        let filter = doc! { "_id": component_id.to_string() };

        collection
            .delete_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to delete component: {}", e)))?;

        Ok(())
    }

    async fn get_memories(&self, params: MemoryQuery) -> Result<Vec<Memory>> {
        let collection = self.collection::<Document>("memories");

        let mut filter = doc! {};

        if let Some(agent_id) = params.agent_id {
            filter.insert("agent_id", agent_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            filter.insert("room_id", room_id.to_string());
        }
        if let Some(entity_id) = params.entity_id {
            filter.insert("entity_id", entity_id.to_string());
        }
        if let Some(unique) = params.unique {
            filter.insert("unique_flag", unique);
        }

        let mut options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();

        if let Some(count) = params.count {
            options.limit = Some(count as i64);
        }

        let mut cursor = collection
            .find(filter)
            .with_options(options)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get memories: {}", e)))?;

        let mut memories = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate memories: {}", e))
        })? {
            memories.push(self.doc_to_memory(&doc)?);
        }

        Ok(memories)
    }

    async fn create_memory(&self, memory: &Memory, _table_name: &str) -> Result<UUID> {
        let collection = self.collection::<Document>("memories");

        let doc = doc! {
            "_id": memory.id.to_string(),
            "entity_id": memory.entity_id.to_string(),
            "agent_id": memory.agent_id.to_string(),
            "room_id": memory.room_id.to_string(),
            "content": to_bson(&memory.content).unwrap_or(Bson::Document(doc! {})),
            "embedding": memory.embedding.as_ref().map(|e| to_bson(e).unwrap_or(Bson::Null)),
            "metadata": memory.metadata.as_ref().map(|m| to_bson(m).unwrap_or(Bson::Document(doc! {}))),
            "created_at": memory.created_at,
            "unique_flag": memory.unique.unwrap_or(false),
        };

        collection
            .insert_one(doc)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create memory: {}", e)))?;

        Ok(memory.id)
    }

    async fn search_memories_by_embedding(
        &self,
        params: SearchMemoriesParams,
    ) -> Result<Vec<Memory>> {
        // MongoDB Atlas Search with vector search requires Atlas Search index
        // For now, we'll use a basic approach - in production, use Atlas Search
        warn!("Vector search in MongoDB requires Atlas Search index. Using basic query.");

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

    async fn get_cached_embeddings(&self, params: MemoryQuery) -> Result<Vec<Memory>> {
        let collection = self.collection::<Document>("memories");

        let mut filter = doc! {
            "embedding": { "$exists": true, "$ne": null }
        };

        if let Some(agent_id) = params.agent_id {
            filter.insert("agent_id", agent_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            filter.insert("room_id", room_id.to_string());
        }
        if let Some(entity_id) = params.entity_id {
            filter.insert("entity_id", entity_id.to_string());
        }

        let mut options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();

        if let Some(count) = params.count {
            options.limit = Some(count as i64);
        }

        let mut cursor = collection
            .find(filter)
            .with_options(options)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get cached embeddings: {}", e)))?;

        let mut memories = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate memories: {}", e))
        })? {
            memories.push(self.doc_to_memory(&doc)?);
        }

        Ok(memories)
    }

    async fn update_memory(&self, memory: &Memory) -> Result<bool> {
        let collection = self.collection::<Document>("memories");

        let filter = doc! { "_id": memory.id.to_string() };
        let update = doc! {
            "$set": {
                "content": to_bson(&memory.content).unwrap_or(Bson::Document(doc! {})),
                "embedding": memory.embedding.as_ref().map(|e| to_bson(e).unwrap_or(Bson::Null)),
                "metadata": memory.metadata.as_ref().map(|m| to_bson(m).unwrap_or(Bson::Document(doc! {}))),
            }
        };

        let result = collection
            .update_one(filter, update)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to update memory: {}", e)))?;

        Ok(result.modified_count > 0)
    }

    async fn remove_memory(&self, memory_id: UUID, _table_name: &str) -> Result<bool> {
        let collection = self.collection::<Document>("memories");
        let filter = doc! { "_id": memory_id.to_string() };

        let result = collection
            .delete_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to delete memory: {}", e)))?;

        Ok(result.deleted_count > 0)
    }

    async fn remove_all_memories(&self, agent_id: UUID, _table_name: &str) -> Result<bool> {
        let collection = self.collection::<Document>("memories");
        let filter = doc! { "agent_id": agent_id.to_string() };

        let result = collection
            .delete_many(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to delete memories: {}", e)))?;

        Ok(result.deleted_count > 0)
    }

    async fn count_memories(&self, params: MemoryQuery) -> Result<usize> {
        let collection = self.collection::<Document>("memories");

        let mut filter = doc! {};

        if let Some(agent_id) = params.agent_id {
            filter.insert("agent_id", agent_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            filter.insert("room_id", room_id.to_string());
        }
        if let Some(entity_id) = params.entity_id {
            filter.insert("entity_id", entity_id.to_string());
        }
        if let Some(unique) = params.unique {
            filter.insert("unique_flag", unique);
        }

        let count = collection
            .count_documents(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to count memories: {}", e)))?;

        Ok(count as usize)
    }

    async fn get_world(&self, world_id: UUID) -> Result<Option<World>> {
        let collection = self.collection::<Document>("worlds");
        let filter = doc! { "_id": world_id.to_string() };

        let result = collection
            .find_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get world: {}", e)))?;

        match result {
            Some(doc) => Ok(Some(self.doc_to_world(&doc)?)),
            None => Ok(None),
        }
    }

    async fn ensure_world(&self, world: &World) -> Result<()> {
        let collection = self.collection::<Document>("worlds");

        let doc = doc! {
            "_id": world.id.to_string(),
            "name": &world.name,
            "agent_id": world.agent_id.to_string(),
            "server_id": &world.server_id,
            "metadata": to_bson(&world.metadata).unwrap_or(Bson::Document(doc! {})),
            "created_at": world.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()),
        };

        let options = UpdateOptions::builder().upsert(true).build();
        collection
            .update_one(doc! { "_id": world.id.to_string() }, doc! { "$set": doc })
            .with_options(options)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to ensure world: {}", e)))?;

        Ok(())
    }

    async fn get_room(&self, room_id: UUID) -> Result<Option<Room>> {
        let collection = self.collection::<Document>("rooms");
        let filter = doc! { "_id": room_id.to_string() };

        let result = collection
            .find_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get room: {}", e)))?;

        match result {
            Some(doc) => Ok(Some(self.doc_to_room(&doc)?)),
            None => Ok(None),
        }
    }

    async fn create_room(&self, room: &Room) -> Result<UUID> {
        let collection = self.collection::<Document>("rooms");

        let channel_type_str = serde_json::to_string(&room.channel_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        let doc = doc! {
            "_id": room.id.to_string(),
            "agent_id": room.agent_id.map(|id| id.to_string()),
            "name": &room.name,
            "source": &room.source,
            "type": channel_type_str,
            "channel_id": &room.channel_id,
            "server_id": &room.server_id,
            "world_id": room.world_id.to_string(),
            "metadata": to_bson(&room.metadata).unwrap_or(Bson::Document(doc! {})),
            "created_at": room.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()),
        };

        collection
            .insert_one(doc)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create room: {}", e)))?;

        Ok(room.id)
    }

    async fn get_rooms(&self, world_id: UUID) -> Result<Vec<Room>> {
        let collection = self.collection::<Document>("rooms");
        let filter = doc! { "world_id": world_id.to_string() };

        let mut cursor = collection
            .find(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get rooms: {}", e)))?;

        let mut rooms = Vec::new();
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to iterate rooms: {}", e)))?
        {
            rooms.push(self.doc_to_room(&doc)?);
        }

        Ok(rooms)
    }

    async fn get_rooms_for_agent(&self, agent_id: UUID) -> Result<Vec<Room>> {
        let collection = self.collection::<Document>("rooms");
        let filter = doc! { "agent_id": agent_id.to_string() };

        let mut cursor = collection
            .find(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get rooms: {}", e)))?;

        let mut rooms = Vec::new();
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to iterate rooms: {}", e)))?
        {
            rooms.push(self.doc_to_room(&doc)?);
        }

        Ok(rooms)
    }

    async fn add_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let collection = self.collection::<Document>("participants");

        let doc = doc! {
            "entity_id": entity_id.to_string(),
            "room_id": room_id.to_string(),
            "joined_at": chrono::Utc::now().timestamp(),
            "metadata": {},
        };

        let options = UpdateOptions::builder().upsert(true).build();
        let filter = doc! {
            "entity_id": entity_id.to_string(),
            "room_id": room_id.to_string(),
        };

        collection
            .update_one(filter, doc! { "$set": doc })
            .with_options(options)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to add participant: {}", e)))?;

        Ok(true)
    }

    async fn remove_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let collection = self.collection::<Document>("participants");
        let filter = doc! {
            "entity_id": entity_id.to_string(),
            "room_id": room_id.to_string(),
        };

        let result = collection
            .delete_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to remove participant: {}", e)))?;

        Ok(result.deleted_count > 0)
    }

    async fn get_participants(&self, room_id: UUID) -> Result<Vec<Participant>> {
        let collection = self.collection::<Document>("participants");
        let filter = doc! { "room_id": room_id.to_string() };

        let mut cursor = collection
            .find(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get participants: {}", e)))?;

        let mut participants = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| {
            ZoeyError::database(format!("Failed to iterate participants: {}", e))
        })? {
            let participant = Participant {
                entity_id: Self::bson_to_uuid(doc.get("entity_id").unwrap_or(&Bson::Null))?,
                room_id: Self::bson_to_uuid(doc.get("room_id").unwrap_or(&Bson::Null))?,
                joined_at: doc.get_i64("joined_at").ok(),
                metadata: mongodb::bson::from_bson(
                    doc.get("metadata").cloned().unwrap_or(Bson::Document(doc! {})),
                )
                .unwrap_or_default(),
            };
            participants.push(participant);
        }

        Ok(participants)
    }

    async fn create_relationship(&self, relationship: &Relationship) -> Result<bool> {
        let collection = self.collection::<Document>("relationships");

        let doc = doc! {
            "_id": uuid::Uuid::new_v4().to_string(),
            "entity_id_a": relationship.entity_id_a.to_string(),
            "entity_id_b": relationship.entity_id_b.to_string(),
            "type": &relationship.relationship_type,
            "agent_id": relationship.agent_id.to_string(),
            "metadata": to_bson(&relationship.metadata).unwrap_or(Bson::Document(doc! {})),
            "created_at": relationship.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()),
        };

        let options = UpdateOptions::builder().upsert(true).build();
        let filter = doc! {
            "entity_id_a": relationship.entity_id_a.to_string(),
            "entity_id_b": relationship.entity_id_b.to_string(),
            "type": &relationship.relationship_type,
        };

        collection
            .update_one(filter, doc! { "$set": doc })
            .with_options(options)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create relationship: {}", e)))?;

        Ok(true)
    }

    async fn get_relationship(
        &self,
        entity_id_a: UUID,
        entity_id_b: UUID,
    ) -> Result<Option<Relationship>> {
        let collection = self.collection::<Document>("relationships");
        let filter = doc! {
            "$or": [
                { "entity_id_a": entity_id_a.to_string(), "entity_id_b": entity_id_b.to_string() },
                { "entity_id_a": entity_id_b.to_string(), "entity_id_b": entity_id_a.to_string() },
            ]
        };

        let result = collection
            .find_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get relationship: {}", e)))?;

        match result {
            Some(doc) => {
                let relationship = Relationship {
                    entity_id_a: Self::bson_to_uuid(
                        doc.get("entity_id_a").unwrap_or(&Bson::Null),
                    )?,
                    entity_id_b: Self::bson_to_uuid(
                        doc.get("entity_id_b").unwrap_or(&Bson::Null),
                    )?,
                    relationship_type: doc.get_str("type").unwrap_or("").to_string(),
                    agent_id: Self::bson_to_uuid(doc.get("agent_id").unwrap_or(&Bson::Null))?,
                    metadata: mongodb::bson::from_bson(
                        doc.get("metadata").cloned().unwrap_or(Bson::Document(doc! {})),
                    )
                    .unwrap_or_default(),
                    created_at: doc.get_i64("created_at").ok(),
                };
                Ok(Some(relationship))
            }
            None => Ok(None),
        }
    }

    async fn create_task(&self, task: &Task) -> Result<UUID> {
        let collection = self.collection::<Document>("tasks");

        let status_str = match task.status {
            TaskStatus::Pending => "PENDING",
            TaskStatus::Running => "RUNNING",
            TaskStatus::Completed => "COMPLETED",
            TaskStatus::Failed => "FAILED",
            TaskStatus::Cancelled => "CANCELLED",
        };

        let doc = doc! {
            "_id": task.id.to_string(),
            "agent_id": task.agent_id.to_string(),
            "task_type": &task.task_type,
            "data": to_bson(&task.data).unwrap_or(Bson::Document(doc! {})),
            "status": status_str,
            "priority": task.priority,
            "scheduled_at": task.scheduled_at,
            "executed_at": task.executed_at,
            "retry_count": task.retry_count,
            "max_retries": task.max_retries,
            "error": &task.error,
            "created_at": task.created_at,
            "updated_at": task.updated_at,
        };

        collection
            .insert_one(doc)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create task: {}", e)))?;

        Ok(task.id)
    }

    async fn update_task(&self, task: &Task) -> Result<bool> {
        let collection = self.collection::<Document>("tasks");

        let status_str = match task.status {
            TaskStatus::Pending => "PENDING",
            TaskStatus::Running => "RUNNING",
            TaskStatus::Completed => "COMPLETED",
            TaskStatus::Failed => "FAILED",
            TaskStatus::Cancelled => "CANCELLED",
        };

        let filter = doc! { "_id": task.id.to_string() };
        let update = doc! {
            "$set": {
                "data": to_bson(&task.data).unwrap_or(Bson::Document(doc! {})),
                "status": status_str,
                "priority": task.priority,
                "scheduled_at": task.scheduled_at,
                "executed_at": task.executed_at,
                "retry_count": task.retry_count,
                "max_retries": task.max_retries,
                "error": &task.error,
                "updated_at": chrono::Utc::now().timestamp(),
            }
        };

        let result = collection
            .update_one(filter, update)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to update task: {}", e)))?;

        Ok(result.modified_count > 0)
    }

    async fn get_task(&self, task_id: UUID) -> Result<Option<Task>> {
        let collection = self.collection::<Document>("tasks");
        let filter = doc! { "_id": task_id.to_string() };

        let result = collection
            .find_one(filter)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get task: {}", e)))?;

        match result {
            Some(doc) => Ok(Some(self.doc_to_task(&doc)?)),
            None => Ok(None),
        }
    }

    async fn get_pending_tasks(&self, agent_id: UUID) -> Result<Vec<Task>> {
        let collection = self.collection::<Document>("tasks");
        let filter = doc! {
            "agent_id": agent_id.to_string(),
            "status": "PENDING",
        };

        let options = FindOptions::builder()
            .sort(doc! { "scheduled_at": 1, "created_at": 1 })
            .build();

        let mut cursor = collection
            .find(filter)
            .with_options(options)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get pending tasks: {}", e)))?;

        let mut tasks = Vec::new();
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to iterate tasks: {}", e)))?
        {
            tasks.push(self.doc_to_task(&doc)?);
        }

        Ok(tasks)
    }

    async fn log(&self, log: &Log) -> Result<()> {
        let collection = self.collection::<Document>("logs");

        let id = log.id.unwrap_or_else(uuid::Uuid::new_v4);

        let doc = doc! {
            "_id": id.to_string(),
            "entity_id": log.entity_id.to_string(),
            "room_id": log.room_id.map(|id| id.to_string()),
            "body": to_bson(&log.body).unwrap_or(Bson::Document(doc! {})),
            "type": &log.log_type,
            "created_at": log.created_at,
        };

        collection
            .insert_one(doc)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to create log: {}", e)))?;

        Ok(())
    }

    async fn get_logs(&self, params: LogQuery) -> Result<Vec<Log>> {
        let collection = self.collection::<Document>("logs");

        let mut filter = doc! {};

        if let Some(entity_id) = params.entity_id {
            filter.insert("entity_id", entity_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            filter.insert("room_id", room_id.to_string());
        }
        if let Some(log_type) = params.log_type {
            filter.insert("type", log_type);
        }

        let mut options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();

        if let Some(limit) = params.limit {
            options.limit = Some(limit as i64);
        }
        if let Some(offset) = params.offset {
            options.skip = Some(offset as u64);
        }

        let mut cursor = collection
            .find(filter)
            .with_options(options)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to get logs: {}", e)))?;

        let mut logs = Vec::new();
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to iterate logs: {}", e)))?
        {
            let log = Log {
                id: Some(Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?),
                entity_id: Self::bson_to_uuid(doc.get("entity_id").unwrap_or(&Bson::Null))?,
                room_id: doc
                    .get("room_id")
                    .and_then(|b| Self::bson_to_uuid(b).ok()),
                body: mongodb::bson::from_bson(
                    doc.get("body").cloned().unwrap_or(Bson::Document(doc! {})),
                )
                .unwrap_or(serde_json::Value::Null),
                log_type: doc.get_str("type").unwrap_or("").to_string(),
                created_at: doc.get_i64("created_at").unwrap_or(0),
            };
            logs.push(log);
        }

        Ok(logs)
    }

    async fn get_agent_run_summaries(
        &self,
        params: RunSummaryQuery,
    ) -> Result<AgentRunSummaryResult> {
        let collection = self.collection::<Document>("llm_costs");

        let mut filter = doc! {};

        if let Some(agent_id) = params.agent_id {
            filter.insert("agent_id", agent_id.to_string());
        }
        if let Some(status) = params.status {
            filter.insert("success", matches!(status, RunStatus::Completed));
        }

        let total = collection
            .count_documents(filter.clone())
            .await
            .unwrap_or(0);

        let mut options = FindOptions::builder()
            .sort(doc! { "timestamp": -1 })
            .build();

        if let Some(limit) = params.limit {
            options.limit = Some(limit as i64);
        }
        if let Some(offset) = params.offset {
            options.skip = Some(offset as u64);
        }

        let mut cursor = collection.find(filter).with_options(options).await
            .map_err(|e| ZoeyError::database(format!("Failed to query run summaries: {}", e)))?;

        let mut runs = Vec::new();
        while let Some(doc) = cursor.try_next().await.ok().flatten() {
            let id_str = doc.get_str("_id").unwrap_or("");
            let success = doc.get_bool("success").unwrap_or(false);
            let timestamp = doc.get_i64("timestamp").unwrap_or(0);

            let status = if success {
                RunStatus::Completed
            } else {
                RunStatus::Error
            };

            runs.push(AgentRunSummary {
                run_id: id_str.to_string(),
                status,
                started_at: Some(timestamp),
                ended_at: Some(timestamp),
                duration_ms: None,
                message_id: None,
                room_id: None,
                entity_id: None,
                metadata: None,
                counts: None,
            });
        }

        Ok(AgentRunSummaryResult {
            runs,
            total: total as usize,
            has_more: params
                .limit
                .map(|l| (l as u64) + (params.offset.unwrap_or(0) as u64) < total)
                .unwrap_or(false),
        })
    }

    async fn persist_llm_cost(&self, record: LLMCostRecord) -> Result<()> {
        let collection = self.collection::<Document>("llm_costs");

        let doc = doc! {
            "_id": record.id.to_string(),
            "timestamp": record.timestamp.timestamp(),
            "agent_id": record.agent_id.to_string(),
            "user_id": record.user_id,
            "conversation_id": record.conversation_id.map(|id| id.to_string()),
            "action_name": record.action_name,
            "evaluator_name": record.evaluator_name,
            "provider": record.provider,
            "model": record.model,
            "temperature": record.temperature as f64,
            "prompt_tokens": record.prompt_tokens as i64,
            "completion_tokens": record.completion_tokens as i64,
            "total_tokens": record.total_tokens as i64,
            "cached_tokens": record.cached_tokens.map(|t| t as i64),
            "input_cost_usd": record.input_cost_usd as f64,
            "output_cost_usd": record.output_cost_usd as f64,
            "total_cost_usd": record.total_cost_usd as f64,
            "latency_ms": record.latency_ms as i64,
            "ttft_ms": record.ttft_ms.map(|t| t as i64),
            "success": record.success,
            "error": record.error,
            "prompt_hash": record.prompt_hash,
            "prompt_preview": record.prompt_preview,
        };

        collection
            .insert_one(doc)
            .await
            .map_err(|e| ZoeyError::database(format!("Failed to persist LLM cost: {}", e)))?;

        Ok(())
    }
}

// Helper methods for document conversion
impl MongoAdapter {
    fn doc_to_entity(&self, doc: &Document) -> Result<Entity> {
        Ok(Entity {
            id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
            agent_id: Self::bson_to_uuid(doc.get("agent_id").unwrap_or(&Bson::Null))?,
            name: doc.get_str("name").ok().map(|s| s.to_string()),
            username: doc.get_str("username").ok().map(|s| s.to_string()),
            email: doc.get_str("email").ok().map(|s| s.to_string()),
            avatar_url: doc.get_str("avatar_url").ok().map(|s| s.to_string()),
            metadata: mongodb::bson::from_bson(
                doc.get("metadata").cloned().unwrap_or(Bson::Document(doc! {})),
            )
            .unwrap_or_default(),
            created_at: Some(doc.get_i64("created_at").unwrap_or(0)),
        })
    }

    fn doc_to_component(&self, doc: &Document) -> Result<Component> {
        Ok(Component {
            id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
            entity_id: Self::bson_to_uuid(doc.get("entity_id").unwrap_or(&Bson::Null))?,
            world_id: Self::bson_to_uuid(doc.get("world_id").unwrap_or(&Bson::Null))?,
            source_entity_id: doc
                .get("source_entity_id")
                .and_then(|b| Self::bson_to_uuid(b).ok()),
            component_type: doc.get_str("type").unwrap_or("").to_string(),
            data: mongodb::bson::from_bson(
                doc.get("data").cloned().unwrap_or(Bson::Document(doc! {})),
            )
            .unwrap_or(serde_json::Value::Null),
            created_at: Some(doc.get_i64("created_at").unwrap_or(0)),
            updated_at: doc.get_i64("updated_at").ok(),
        })
    }

    fn doc_to_memory(&self, doc: &Document) -> Result<Memory> {
        Ok(Memory {
            id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
            entity_id: Self::bson_to_uuid(doc.get("entity_id").unwrap_or(&Bson::Null))?,
            agent_id: Self::bson_to_uuid(doc.get("agent_id").unwrap_or(&Bson::Null))?,
            room_id: Self::bson_to_uuid(doc.get("room_id").unwrap_or(&Bson::Null))?,
            content: mongodb::bson::from_bson(
                doc.get("content").cloned().unwrap_or(Bson::Document(doc! {})),
            )
            .unwrap_or_default(),
            embedding: doc
                .get("embedding")
                .and_then(|b| mongodb::bson::from_bson::<Vec<f32>>(b.clone()).ok()),
            metadata: doc
                .get("metadata")
                .and_then(|b| mongodb::bson::from_bson(b.clone()).ok()),
            created_at: doc.get_i64("created_at").unwrap_or(0),
            unique: doc.get_bool("unique_flag").ok(),
            similarity: None,
        })
    }

    fn doc_to_world(&self, doc: &Document) -> Result<World> {
        Ok(World {
            id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
            name: doc.get_str("name").unwrap_or("").to_string(),
            agent_id: Self::bson_to_uuid(doc.get("agent_id").unwrap_or(&Bson::Null))?,
            server_id: doc.get_str("server_id").ok().map(|s| s.to_string()),
            metadata: mongodb::bson::from_bson(
                doc.get("metadata").cloned().unwrap_or(Bson::Document(doc! {})),
            )
            .unwrap_or_default(),
            created_at: doc.get_i64("created_at").ok(),
        })
    }

    fn doc_to_room(&self, doc: &Document) -> Result<Room> {
        let type_str = doc.get_str("type").unwrap_or("UNKNOWN");
        let channel_type = match type_str {
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
            id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
            agent_id: doc
                .get("agent_id")
                .and_then(|b| Self::bson_to_uuid(b).ok()),
            name: doc.get_str("name").unwrap_or("").to_string(),
            source: doc.get_str("source").unwrap_or("").to_string(),
            channel_type,
            channel_id: doc.get_str("channel_id").ok().map(|s| s.to_string()),
            server_id: doc.get_str("server_id").ok().map(|s| s.to_string()),
            world_id: Self::bson_to_uuid(doc.get("world_id").unwrap_or(&Bson::Null))?,
            metadata: mongodb::bson::from_bson(
                doc.get("metadata").cloned().unwrap_or(Bson::Document(doc! {})),
            )
            .unwrap_or_default(),
            created_at: doc.get_i64("created_at").ok(),
        })
    }

    fn doc_to_task(&self, doc: &Document) -> Result<Task> {
        let status_str = doc.get_str("status").unwrap_or("PENDING");
        let status = match status_str {
            "PENDING" => TaskStatus::Pending,
            "RUNNING" => TaskStatus::Running,
            "COMPLETED" => TaskStatus::Completed,
            "FAILED" => TaskStatus::Failed,
            "CANCELLED" => TaskStatus::Cancelled,
            _ => TaskStatus::Pending,
        };

        Ok(Task {
            id: Self::bson_to_uuid(doc.get("_id").unwrap_or(&Bson::Null))?,
            agent_id: Self::bson_to_uuid(doc.get("agent_id").unwrap_or(&Bson::Null))?,
            task_type: doc.get_str("task_type").unwrap_or("").to_string(),
            data: mongodb::bson::from_bson(
                doc.get("data").cloned().unwrap_or(Bson::Document(doc! {})),
            )
            .unwrap_or(serde_json::Value::Null),
            status,
            priority: doc.get_i32("priority").unwrap_or(0),
            scheduled_at: doc.get_i64("scheduled_at").ok(),
            executed_at: doc.get_i64("executed_at").ok(),
            retry_count: doc.get_i32("retry_count").unwrap_or(0),
            max_retries: doc.get_i32("max_retries").unwrap_or(3),
            error: doc.get_str("error").ok().map(|s| s.to_string()),
            created_at: doc.get_i64("created_at").unwrap_or(0),
            updated_at: doc.get_i64("updated_at").ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mongo_adapter_creation() {
        assert!(true);
    }
}
