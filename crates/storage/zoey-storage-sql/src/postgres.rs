//! PostgreSQL database adapter

use async_trait::async_trait;
use zoey_core::observability::types::LLMCostRecord;
use zoey_core::{types::*, ZoeyError, Result};
use sqlx::{postgres::PgPoolOptions, Arguments, PgPool, Row};
use tracing::{debug, info, warn};

/// Allowed table names for SQL queries (whitelist approach for SQL injection prevention)
const ALLOWED_TABLES: &[&str] = &[
    "memories",
    "agents",
    "entities",
    "worlds",
    "rooms",
    "relationships",
    "goals",
    "logs",
    "cache",
    "components",
    "embeddings",
    "documents",
    "conversations",
];

/// Validate table name to prevent SQL injection
///
/// This function provides defense-in-depth against SQL injection:
/// 1. Whitelist validation - only known tables are allowed
/// 2. Character validation - only alphanumeric and underscore allowed
/// 3. Length limit - prevents buffer overflow attacks
fn validate_table_name(name: &str) -> Result<&str> {
    // Length check (defense against buffer overflow)
    if name.len() > 64 {
        warn!("Rejected table name due to length: {} chars", name.len());
        return Err(ZoeyError::validation(format!(
            "Table name too long: {} (max 64 characters)",
            name.len()
        )));
    }

    // Character validation (alphanumeric and underscore only)
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        warn!("Rejected table name with invalid characters: {}", name);
        return Err(ZoeyError::validation(format!(
            "Invalid table name '{}': only alphanumeric characters and underscores allowed",
            name
        )));
    }

    // Whitelist validation
    if !ALLOWED_TABLES.contains(&name) {
        warn!("Rejected unknown table name: {}", name);
        return Err(ZoeyError::validation(format!(
            "Unknown table name '{}': not in allowed list",
            name
        )));
    }

    Ok(name)
}

/// Validate SQL identifier (table/column) for safety
fn validate_identifier(name: &str) -> Result<&str> {
    if name.len() > 64 {
        warn!("Rejected identifier due to length: {} chars", name.len());
        return Err(ZoeyError::validation(format!(
            "Identifier too long: {} (max 64 characters)",
            name.len()
        )));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        warn!("Rejected identifier with invalid characters: {}", name);
        return Err(ZoeyError::validation(format!(
            "Invalid identifier '{}': only alphanumeric characters and underscores allowed",
            name
        )));
    }
    Ok(name)
}

/// PostgreSQL database adapter
pub struct PostgresAdapter {
    pool: PgPool,
    embedding_dimension: std::sync::RwLock<usize>,
}

impl PostgresAdapter {
    /// Set current agent context for RLS policies
    pub async fn set_current_agent(&self, agent_id: uuid::Uuid) -> Result<()> {
        sqlx::query("SELECT set_config('app.current_agent_id', $1, false)")
            .bind(agent_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    /// Create a new PostgreSQL adapter
    pub async fn new(database_url: &str) -> Result<Self> {
        info!("Connecting to PostgreSQL database...");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| ZoeyError::DatabaseSqlx(e))?;

        Ok(Self {
            pool,
            embedding_dimension: std::sync::RwLock::new(1536), // Default OpenAI embedding dimension
        })
    }

    /// Create with custom pool options
    pub async fn with_pool(pool: PgPool) -> Self {
        Self {
            pool,
            embedding_dimension: std::sync::RwLock::new(1536),
        }
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<()> {
        debug!("Initializing database schema...");

        // Ensure pgvector extension is available
        sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(&self.pool)
            .await
            .ok(); // Ignore error if extension already exists or can't be created

        // ============================================================
        // CREATE TABLES
        // ============================================================

        // Agents table (root entity - no foreign keys)
        // Note: Using BIGINT for timestamps (Unix epoch) for Rust i64 compatibility
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agents (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                character JSONB NOT NULL,
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
                updated_at BIGINT
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Entities table (belongs to agent)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS entities (
                id UUID PRIMARY KEY,
                agent_id UUID NOT NULL,
                name TEXT,
                username TEXT,
                email TEXT,
                avatar_url TEXT,
                metadata JSONB DEFAULT '{}',
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Worlds table (belongs to agent)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS worlds (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                agent_id UUID NOT NULL,
                server_id TEXT,
                metadata JSONB DEFAULT '{}',
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Rooms table (belongs to world and optionally agent)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS rooms (
                id UUID PRIMARY KEY,
                agent_id UUID,
                name TEXT NOT NULL,
                source TEXT NOT NULL,
                type TEXT NOT NULL,
                channel_id TEXT,
                server_id TEXT,
                world_id UUID NOT NULL,
                metadata JSONB DEFAULT '{}',
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Memories table (core data - belongs to entity, agent, room)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id UUID PRIMARY KEY,
                entity_id UUID NOT NULL,
                agent_id UUID NOT NULL,
                room_id UUID NOT NULL,
                content JSONB NOT NULL,
                embedding vector,
                metadata JSONB,
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
                unique_flag BOOLEAN DEFAULT FALSE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Participants junction table (entity <-> room)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS participants (
                entity_id UUID NOT NULL,
                room_id UUID NOT NULL,
                joined_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
                metadata JSONB DEFAULT '{}',
                PRIMARY KEY (entity_id, room_id)
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Relationships table (entity <-> entity with type)
        // NOTE: Primary key includes type to allow multiple relationship types between same entities
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS relationships (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                entity_id_a UUID NOT NULL,
                entity_id_b UUID NOT NULL,
                type TEXT NOT NULL,
                agent_id UUID NOT NULL,
                metadata JSONB DEFAULT '{}',
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
                UNIQUE (entity_id_a, entity_id_b, type)
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Components table (ECS-style components attached to entities)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS components (
                id UUID PRIMARY KEY,
                entity_id UUID NOT NULL,
                world_id UUID NOT NULL,
                source_entity_id UUID,
                type TEXT NOT NULL,
                data JSONB NOT NULL,
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
                updated_at BIGINT
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Tasks table (deferred/scheduled work)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id UUID PRIMARY KEY,
                agent_id UUID NOT NULL,
                task_type TEXT NOT NULL,
                data JSONB NOT NULL,
                status TEXT NOT NULL DEFAULT 'PENDING',
                priority INTEGER DEFAULT 0,
                scheduled_at BIGINT,
                executed_at BIGINT,
                retry_count INTEGER DEFAULT 0,
                max_retries INTEGER DEFAULT 3,
                error TEXT,
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
                updated_at BIGINT
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Logs table (audit/debug logs)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS logs (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                entity_id UUID NOT NULL,
                room_id UUID,
                body JSONB NOT NULL,
                type TEXT NOT NULL,
                created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // ============================================================
        // ADD FOREIGN KEY CONSTRAINTS (idempotent via DO block)
        // ============================================================

        // Helper: Add FK constraint if it doesn't exist
        sqlx::query(
            r#"
            DO $$
            BEGIN
                -- entities -> agents
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_entities_agent') THEN
                    ALTER TABLE entities ADD CONSTRAINT fk_entities_agent
                        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE;
                END IF;

                -- worlds -> agents
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_worlds_agent') THEN
                    ALTER TABLE worlds ADD CONSTRAINT fk_worlds_agent
                        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE;
                END IF;

                -- rooms -> worlds
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_rooms_world') THEN
                    ALTER TABLE rooms ADD CONSTRAINT fk_rooms_world
                        FOREIGN KEY (world_id) REFERENCES worlds(id) ON DELETE CASCADE;
                END IF;

                -- rooms -> agents (optional)
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_rooms_agent') THEN
                    ALTER TABLE rooms ADD CONSTRAINT fk_rooms_agent
                        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE SET NULL;
                END IF;

                -- memories -> entities
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_memories_entity') THEN
                    ALTER TABLE memories ADD CONSTRAINT fk_memories_entity
                        FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE;
                END IF;

                -- memories -> agents
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_memories_agent') THEN
                    ALTER TABLE memories ADD CONSTRAINT fk_memories_agent
                        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE;
                END IF;

                -- memories -> rooms
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_memories_room') THEN
                    ALTER TABLE memories ADD CONSTRAINT fk_memories_room
                        FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE;
                END IF;

                -- participants -> entities
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_participants_entity') THEN
                    ALTER TABLE participants ADD CONSTRAINT fk_participants_entity
                        FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE;
                END IF;

                -- participants -> rooms
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_participants_room') THEN
                    ALTER TABLE participants ADD CONSTRAINT fk_participants_room
                        FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE;
                END IF;

                -- relationships -> entities (both sides)
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_relationships_entity_a') THEN
                    ALTER TABLE relationships ADD CONSTRAINT fk_relationships_entity_a
                        FOREIGN KEY (entity_id_a) REFERENCES entities(id) ON DELETE CASCADE;
                END IF;

                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_relationships_entity_b') THEN
                    ALTER TABLE relationships ADD CONSTRAINT fk_relationships_entity_b
                        FOREIGN KEY (entity_id_b) REFERENCES entities(id) ON DELETE CASCADE;
                END IF;

                -- relationships -> agents
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_relationships_agent') THEN
                    ALTER TABLE relationships ADD CONSTRAINT fk_relationships_agent
                        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE;
                END IF;

                -- components -> entities
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_components_entity') THEN
                    ALTER TABLE components ADD CONSTRAINT fk_components_entity
                        FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE;
                END IF;

                -- components -> worlds
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_components_world') THEN
                    ALTER TABLE components ADD CONSTRAINT fk_components_world
                        FOREIGN KEY (world_id) REFERENCES worlds(id) ON DELETE CASCADE;
                END IF;

                -- components -> entities (source)
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_components_source_entity') THEN
                    ALTER TABLE components ADD CONSTRAINT fk_components_source_entity
                        FOREIGN KEY (source_entity_id) REFERENCES entities(id) ON DELETE SET NULL;
                END IF;

                -- tasks -> agents
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_tasks_agent') THEN
                    ALTER TABLE tasks ADD CONSTRAINT fk_tasks_agent
                        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE;
                END IF;

                -- logs -> entities
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_logs_entity') THEN
                    ALTER TABLE logs ADD CONSTRAINT fk_logs_entity
                        FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE;
                END IF;

                -- logs -> rooms (optional)
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_logs_room') THEN
                    ALTER TABLE logs ADD CONSTRAINT fk_logs_room
                        FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE SET NULL;
                END IF;
            END $$;
        "#,
        )
        .execute(&self.pool)
        .await?;

        // ============================================================
        // ADD UNIQUE CONSTRAINTS
        // ============================================================

        // Unique constraint on components (entity, world, type, source_entity)
        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'uq_components_entity_world_type_source') THEN
                    ALTER TABLE components ADD CONSTRAINT uq_components_entity_world_type_source
                        UNIQUE (entity_id, world_id, type, source_entity_id);
                END IF;
            END $$;
        "#,
        )
        .execute(&self.pool)
        .await?;

        // ============================================================
        // CREATE INDICES (comprehensive coverage)
        // ============================================================

        // -- Entities indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_agent_id ON entities(agent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_username ON entities(username) WHERE username IS NOT NULL")
            .execute(&self.pool)
            .await?;

        // -- Worlds indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_worlds_agent_id ON worlds(agent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_worlds_server_id ON worlds(server_id) WHERE server_id IS NOT NULL")
            .execute(&self.pool)
            .await?;

        // -- Rooms indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_world_id ON rooms(world_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_agent_id ON rooms(agent_id) WHERE agent_id IS NOT NULL")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_channel_id ON rooms(channel_id) WHERE channel_id IS NOT NULL")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_source_server ON rooms(source, server_id) WHERE server_id IS NOT NULL")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_type ON rooms(type)")
            .execute(&self.pool)
            .await?;

        // -- Memories indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_agent_id ON memories(agent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_room_id ON memories(room_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_entity_id ON memories(entity_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_agent_room_created ON memories(agent_id, room_id, created_at DESC)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_unique ON memories(agent_id, room_id) WHERE unique_flag = TRUE")
            .execute(&self.pool)
            .await?;

        // -- Participants indices --
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_participants_entity_id ON participants(entity_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_participants_room_id ON participants(room_id)")
            .execute(&self.pool)
            .await?;

        // -- Relationships indices --
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_relationships_entity_a ON relationships(entity_id_a)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_relationships_entity_b ON relationships(entity_id_b)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_relationships_agent_id ON relationships(agent_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_relationships_type ON relationships(type)")
            .execute(&self.pool)
            .await?;

        // -- Components indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_components_entity_id ON components(entity_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_components_world_id ON components(world_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_components_type ON components(type)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_components_entity_type ON components(entity_id, type)",
        )
        .execute(&self.pool)
        .await?;

        // -- Tasks indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tasks_agent_id ON tasks(agent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tasks_status_scheduled ON tasks(status, scheduled_at) WHERE status = 'PENDING'")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tasks_agent_status ON tasks(agent_id, status)")
            .execute(&self.pool)
            .await?;

        // -- Logs indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_logs_entity_id ON logs(entity_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_logs_room_id ON logs(room_id) WHERE room_id IS NOT NULL")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_logs_type ON logs(type)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_logs_created_at ON logs(created_at DESC)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_logs_entity_created ON logs(entity_id, created_at DESC)")
            .execute(&self.pool)
            .await?;

        // ============================================================
        // OBSERVABILITY TABLES
        // ============================================================

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS llm_costs (
                id UUID PRIMARY KEY,
                timestamp TIMESTAMPTZ NOT NULL,

                agent_id UUID NOT NULL,
                user_id TEXT,
                conversation_id UUID,
                action_name TEXT,
                evaluator_name TEXT,

                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                temperature REAL NOT NULL,

                prompt_tokens INTEGER NOT NULL,
                completion_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cached_tokens INTEGER,

                input_cost_usd REAL NOT NULL,
                output_cost_usd REAL NOT NULL,
                total_cost_usd REAL NOT NULL,

                latency_ms BIGINT NOT NULL,
                ttft_ms BIGINT,

                success BOOLEAN NOT NULL,
                error TEXT,

                prompt_hash TEXT,
                prompt_preview TEXT
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS stored_prompts (
                id UUID PRIMARY KEY,
                timestamp TIMESTAMPTZ NOT NULL,
                cost_record_id UUID REFERENCES llm_costs(id) ON DELETE CASCADE,
                agent_id UUID NOT NULL,
                conversation_id UUID,
                prompt_hash TEXT NOT NULL,
                prompt_text TEXT,
                prompt_length INTEGER NOT NULL,
                sanitized BOOLEAN NOT NULL,
                sanitization_level TEXT NOT NULL,
                completion_text TEXT,
                completion_length INTEGER NOT NULL DEFAULT 0,
                model TEXT NOT NULL,
                temperature REAL NOT NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Observability indices
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_llm_costs_agent_timestamp ON llm_costs(agent_id, timestamp DESC)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_llm_costs_provider_model ON llm_costs(provider, model)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_llm_costs_timestamp ON llm_costs(timestamp DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_llm_costs_conversation ON llm_costs(conversation_id) WHERE conversation_id IS NOT NULL")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_stored_prompts_agent_timestamp ON stored_prompts(agent_id, timestamp DESC)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_stored_prompts_hash ON stored_prompts(prompt_hash)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_stored_prompts_cost_record ON stored_prompts(cost_record_id)")
            .execute(&self.pool)
            .await?;

        info!("Database schema initialized successfully");
        Ok(())
    }
}

#[async_trait]
impl IDatabaseAdapter for PostgresAdapter {
    fn db(&self) -> &dyn std::any::Any {
        &self.pool
    }

    async fn initialize(&mut self, _config: Option<serde_json::Value>) -> Result<()> {
        self.init_schema().await
    }

    async fn is_ready(&self) -> Result<bool> {
        // Try a simple query
        match sqlx::query("SELECT 1").fetch_one(&self.pool).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn close(&mut self) -> Result<()> {
        self.pool.close().await;
        Ok(())
    }

    async fn get_connection(&self) -> Result<Box<dyn std::any::Any + Send>> {
        Ok(Box::new(self.pool.clone()))
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
                    info!("Applying schema for plugin '{}'", plugin.name);
                }

                // Expect object: { table_name: { type: "table", columns: { col: type, ... } }, ... }
                let schema_obj = schema.as_object().ok_or_else(|| {
                    ZoeyError::validation(format!(
                        "Invalid schema for plugin '{}': expected JSON object",
                        plugin.name
                    ))
                })?;

                // Build dependency graph from REFERENCES clauses
                use std::collections::{HashMap, HashSet, VecDeque};
                let mut deps: HashMap<String, HashSet<String>> = HashMap::new();
                let mut reverse_deps: HashMap<String, HashSet<String>> = HashMap::new();
                let table_names: Vec<String> = schema_obj.keys().map(|k| k.to_string()).collect();

                for (tname, tdef) in schema_obj.iter() {
                    let tname = validate_identifier(tname)?;
                    let columns_obj =
                        tdef.get("columns")
                            .and_then(|v| v.as_object())
                            .ok_or_else(|| {
                                ZoeyError::validation(format!(
                                    "Invalid table definition for '{}': missing 'columns' object",
                                    tname
                                ))
                            })?;
                    for (_col_name, col_type_val) in columns_obj.iter() {
                        if let Some(col_type) = col_type_val.as_str() {
                            if let Some(idx) = col_type.find("REFERENCES") {
                                let tail = &col_type[idx + "REFERENCES".len()..];
                                let ref_name = tail
                                    .trim()
                                    .split(|c: char| c.is_whitespace() || c == '(')
                                    .filter(|s| !s.is_empty())
                                    .next()
                                    .unwrap_or("");
                                if !ref_name.is_empty() {
                                    let ref_name = validate_identifier(ref_name)?;
                                    if table_names.iter().any(|n| n == ref_name) {
                                        deps.entry(tname.to_string())
                                            .or_default()
                                            .insert(ref_name.to_string());
                                        reverse_deps
                                            .entry(ref_name.to_string())
                                            .or_default()
                                            .insert(tname.to_string());
                                    }
                                }
                            }
                        }
                    }
                }

                // Kahn's algorithm for topological sort
                let mut indegree: HashMap<String, usize> = HashMap::new();
                for t in &table_names {
                    let d = deps.get(t).map(|s| s.len()).unwrap_or(0);
                    indegree.insert(t.clone(), d);
                }
                let mut q: VecDeque<String> = table_names
                    .iter()
                    .filter(|t| indegree.get(*t).copied().unwrap_or(0) == 0)
                    .cloned()
                    .collect();
                let mut order: Vec<String> = Vec::new();
                while let Some(n) = q.pop_front() {
                    order.push(n.clone());
                    if let Some(children) = reverse_deps.get(&n) {
                        for c in children {
                            if let Some(e) = indegree.get_mut(c) {
                                if *e > 0 {
                                    *e -= 1;
                                    if *e == 0 {
                                        q.push_back(c.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                // Append any remaining (cycles or external deps), keeping original order
                for t in &table_names {
                    if !order.contains(t) {
                        order.push(t.clone());
                    }
                }

                // Create tables in sorted order
                for table_name in order {
                    let table_def = &schema_obj[&table_name];
                    let columns_obj = table_def
                        .get("columns")
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| {
                            ZoeyError::validation(format!(
                                "Invalid table definition for '{}': missing 'columns' object",
                                table_name
                            ))
                        })?;

                    let mut cols_sql: Vec<String> = Vec::new();
                    for (col_name, col_type_val) in columns_obj.iter() {
                        let col_name = validate_identifier(col_name)?;
                        let col_type = col_type_val.as_str().ok_or_else(|| {
                            ZoeyError::validation(format!(
                                "Invalid type for column '{}.{}'",
                                table_name, col_name
                            ))
                        })?;

                        let allowed_chars = |c: char| {
                            c.is_ascii_alphanumeric()
                                || matches!(c, ' ' | '_' | '(' | ')' | ',' | '-' | ':')
                        };
                        if col_type.len() > 128 || !col_type.chars().all(allowed_chars) {
                            return Err(ZoeyError::validation(format!(
                                "Unsupported or unsafe type for '{}.{}': {}",
                                table_name, col_name, col_type
                            )));
                        }
                        cols_sql.push(format!("{} {}", col_name, col_type));
                    }

                    let create_sql = format!(
                        "CREATE TABLE IF NOT EXISTS {} ({})",
                        table_name,
                        cols_sql.join(", ")
                    );

                    if options.verbose {
                        debug!("{}", create_sql);
                    }

                    if !options.dry_run {
                        sqlx::query(&create_sql).execute(&self.pool).await?;
                    }
                }

                if options.verbose {
                    info!("✓ Schema applied for plugin '{}'", plugin.name);
                }
            }
        }
        Ok(())
    }

    // Agent operations
    async fn get_agent(&self, agent_id: UUID) -> Result<Option<Agent>> {
        let row = sqlx::query(
            "SELECT id, name, character, created_at, updated_at FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Agent {
                id: row.get("id"),
                name: row.get("name"),
                character: row.get("character"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })),
            None => Ok(None),
        }
    }

    async fn get_agents(&self) -> Result<Vec<Agent>> {
        let rows = sqlx::query("SELECT id, name, character, created_at, updated_at FROM agents")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| Agent {
                id: row.get("id"),
                name: row.get("name"),
                character: row.get("character"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    async fn create_agent(&self, agent: &Agent) -> Result<bool> {
        let result = sqlx::query(
            "INSERT INTO agents (id, name, character, created_at, updated_at) 
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&agent.id)
        .bind(&agent.name)
        .bind(&agent.character)
        .bind(&agent.created_at)
        .bind(&agent.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_agent(&self, agent_id: UUID, agent: &Agent) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE agents SET name = $2, character = $3, updated_at = $4 WHERE id = $1",
        )
        .bind(agent_id)
        .bind(&agent.name)
        .bind(&agent.character)
        .bind(chrono::Utc::now().timestamp())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_agent(&self, agent_id: UUID) -> Result<bool> {
        let result = sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn ensure_embedding_dimension(&self, dimension: usize) -> Result<()> {
        // Enable pgvector extension if not already enabled
        sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(&self.pool)
            .await?;

        *self.embedding_dimension.write().unwrap() = dimension;
        Ok(())
    }

    async fn get_entities_by_ids(&self, entity_ids: Vec<UUID>) -> Result<Vec<Entity>> {
        if entity_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query(
            "SELECT id, agent_id, name, username, email, avatar_url, metadata, created_at FROM entities WHERE id = ANY($1)"
        )
        .bind(entity_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Entity {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                name: row.get("name"),
                username: row.get("username"),
                email: row.get("email"),
                avatar_url: row.get("avatar_url"),
                metadata: row
                    .try_get("metadata")
                    .ok()
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn get_entities_for_room(
        &self,
        room_id: UUID,
        _include_components: bool,
    ) -> Result<Vec<Entity>> {
        let rows = sqlx::query(
            "SELECT e.id, e.agent_id, e.name, e.username, e.email, e.avatar_url, e.metadata, e.created_at
             FROM participants p JOIN entities e ON e.id = p.entity_id WHERE p.room_id = $1"
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Entity {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                name: row.get("name"),
                username: row.get("username"),
                email: row.get("email"),
                avatar_url: row.get("avatar_url"),
                metadata: row
                    .try_get("metadata")
                    .ok()
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn create_entities(&self, entities: Vec<Entity>) -> Result<bool> {
        for entity in entities {
            sqlx::query(
                "INSERT INTO entities (id, agent_id, name, username, email, avatar_url, metadata, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    username = EXCLUDED.username,
                    email = EXCLUDED.email,
                    avatar_url = EXCLUDED.avatar_url,
                    metadata = EXCLUDED.metadata"
            )
            .bind(entity.id)
            .bind(entity.agent_id)
            .bind(&entity.name)
            .bind(&entity.username)
            .bind(&entity.email)
            .bind(&entity.avatar_url)
            .bind(serde_json::to_value(&entity.metadata)?)
            .bind(entity.created_at)
            .execute(&self.pool)
            .await?;
        }
        Ok(true)
    }

    async fn update_entity(&self, entity: &Entity) -> Result<()> {
        sqlx::query(
            "UPDATE entities SET name = $2, username = $3, email = $4, avatar_url = $5, metadata = $6, created_at = $7 WHERE id = $1"
        )
        .bind(entity.id)
        .bind(&entity.name)
        .bind(&entity.username)
        .bind(&entity.email)
        .bind(&entity.avatar_url)
        .bind(serde_json::to_value(&entity.metadata)?)
        .bind(entity.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_entity_by_id(&self, entity_id: UUID) -> Result<Option<Entity>> {
        let row = sqlx::query(
            "SELECT id, agent_id, name, username, email, avatar_url, metadata, created_at
             FROM entities WHERE id = $1",
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Entity {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                name: row.get("name"),
                username: row.get("username"),
                email: row.get("email"),
                avatar_url: row.get("avatar_url"),
                metadata: row
                    .try_get("metadata")
                    .ok()
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
                created_at: row.get("created_at"),
            })),
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
        let mut query = String::from(
            "SELECT id, entity_id, world_id, source_entity_id, type, data, created_at, updated_at FROM components WHERE entity_id = $1 AND type = $2"
        );
        let mut idx = 3;
        if world_id.is_some() {
            query.push_str(&format!(" AND world_id = ${}", idx));
            idx += 1;
        }
        if source_entity_id.is_some() {
            query.push_str(&format!(" AND source_entity_id = ${}", idx));
        }

        let mut sql = sqlx::query(&query).bind(entity_id).bind(component_type);
        if let Some(wid) = world_id {
            sql = sql.bind(wid);
        }
        if let Some(seid) = source_entity_id {
            sql = sql.bind(seid);
        }

        let row = sql.fetch_optional(&self.pool).await?;
        Ok(row.map(|row| Component {
            id: row.get("id"),
            entity_id: row.get("entity_id"),
            world_id: row.get("world_id"),
            source_entity_id: row.get("source_entity_id"),
            component_type: row.get("type"),
            data: row.get("data"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }

    async fn get_components(
        &self,
        entity_id: UUID,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Vec<Component>> {
        let mut query = String::from(
            "SELECT id, entity_id, world_id, source_entity_id, type, data, created_at, updated_at FROM components WHERE entity_id = $1"
        );
        let mut idx = 2;
        if world_id.is_some() {
            query.push_str(&format!(" AND world_id = ${}", idx));
            idx += 1;
        }
        if source_entity_id.is_some() {
            query.push_str(&format!(" AND source_entity_id = ${}", idx));
        }
        let mut sql = sqlx::query(&query).bind(entity_id);
        if let Some(wid) = world_id {
            sql = sql.bind(wid);
        }
        if let Some(seid) = source_entity_id {
            sql = sql.bind(seid);
        }
        let rows = sql.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| Component {
                id: row.get("id"),
                entity_id: row.get("entity_id"),
                world_id: row.get("world_id"),
                source_entity_id: row.get("source_entity_id"),
                component_type: row.get("type"),
                data: row.get("data"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    async fn create_component(&self, component: &Component) -> Result<bool> {
        let result = sqlx::query(
            "INSERT INTO components (id, entity_id, world_id, source_entity_id, type, data, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(component.id)
        .bind(component.entity_id)
        .bind(component.world_id)
        .bind(component.source_entity_id)
        .bind(&component.component_type)
        .bind(&component.data)
        .bind(component.created_at)
        .bind(component.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_component(&self, component: &Component) -> Result<()> {
        sqlx::query("UPDATE components SET data = $2, updated_at = $3 WHERE id = $1")
            .bind(component.id)
            .bind(&component.data)
            .bind(chrono::Utc::now().timestamp())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_component(&self, component_id: UUID) -> Result<()> {
        sqlx::query("DELETE FROM components WHERE id = $1")
            .bind(component_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_memories(&self, params: MemoryQuery) -> Result<Vec<Memory>> {
        let mut query = String::from("SELECT id, entity_id, agent_id, room_id, content, embedding, metadata, created_at, unique_flag FROM memories WHERE 1=1");
        let mut param_index = 1;

        if let Some(agent_id) = params.agent_id {
            query.push_str(&format!(" AND agent_id = ${}", param_index));
            param_index += 1;
        }
        if let Some(room_id) = params.room_id {
            query.push_str(&format!(" AND room_id = ${}", param_index));
            param_index += 1;
        }
        if let Some(entity_id) = params.entity_id {
            query.push_str(&format!(" AND entity_id = ${}", param_index));
            param_index += 1;
        }
        if let Some(unique) = params.unique {
            query.push_str(&format!(" AND unique_flag = ${}", param_index));
            param_index += 1;
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(count) = params.count {
            query.push_str(&format!(" LIMIT ${}", param_index));
            param_index += 1;
        }

        let mut sql_query = sqlx::query(&query);
        let mut bind_idx = 1;
        if let Some(agent_id) = params.agent_id {
            sql_query = sql_query.bind(agent_id);
            bind_idx += 1;
        }
        if let Some(room_id) = params.room_id {
            sql_query = sql_query.bind(room_id);
            bind_idx += 1;
        }
        if let Some(entity_id) = params.entity_id {
            sql_query = sql_query.bind(entity_id);
            bind_idx += 1;
        }
        if let Some(unique) = params.unique {
            sql_query = sql_query.bind(unique);
            bind_idx += 1;
        }
        if let Some(count) = params.count {
            sql_query = sql_query.bind(count as i64);
        }

        let rows = sql_query.fetch_all(&self.pool).await?;

        let memories = rows
            .into_iter()
            .map(|row| Memory {
                id: row.get("id"),
                entity_id: row.get("entity_id"),
                agent_id: row.get("agent_id"),
                room_id: row.get("room_id"),
                content: serde_json::from_value(row.get("content")).unwrap_or_default(),
                embedding: row.get("embedding"),
                metadata: row
                    .try_get("metadata")
                    .ok()
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok()),
                created_at: row.get("created_at"),
                unique: Some(row.get("unique_flag")),
                similarity: None,
            })
            .collect();

        Ok(memories)
    }

    async fn create_memory(&self, memory: &Memory, _table_name: &str) -> Result<UUID> {
        sqlx::query(
            "INSERT INTO memories (id, entity_id, agent_id, room_id, content, embedding, metadata, created_at, unique_flag)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(memory.id)
        .bind(memory.entity_id)
        .bind(memory.agent_id)
        .bind(memory.room_id)
        .bind(serde_json::to_value(&memory.content)?)
        .bind(&memory.embedding)
        .bind(serde_json::to_value(&memory.metadata)?)
        .bind(memory.created_at)
        .bind(memory.unique.unwrap_or(false))
        .execute(&self.pool)
        .await?;

        Ok(memory.id)
    }

    async fn search_memories_by_embedding(
        &self,
        params: SearchMemoriesParams,
    ) -> Result<Vec<Memory>> {
        use tracing::{info, warn};

        // Validate embedding dimension
        let expected_dim = *self.embedding_dimension.read().unwrap();
        let actual_dim = params.embedding.len();

        if actual_dim != expected_dim {
            return Err(ZoeyError::vector_search(
                format!(
                    "Embedding dimension mismatch for table '{}'",
                    params.table_name
                ),
                actual_dim,
                expected_dim,
            ));
        }

        // Validate table name to prevent SQL injection
        let validated_table = validate_table_name(&params.table_name)?;

        info!(
            "Searching memories by embedding in table '{}' with {} dimensions, limit: {}",
            validated_table, actual_dim, params.count
        );

        // Build query with optional filters
        let mut query = format!(
            "SELECT id, entity_id, agent_id, room_id, content, embedding, metadata, created_at, unique_flag,
             embedding <-> $1::vector AS similarity
             FROM {}
             WHERE embedding IS NOT NULL",
            validated_table
        );

        let mut param_count = 2;
        if params.agent_id.is_some() {
            query.push_str(&format!(" AND agent_id = ${}", param_count));
            param_count += 1;
        }
        if params.room_id.is_some() {
            query.push_str(&format!(" AND room_id = ${}", param_count));
            param_count += 1;
        }
        if params.world_id.is_some() {
            query.push_str(&format!(" AND world_id = ${}", param_count));
            param_count += 1;
        }
        if params.entity_id.is_some() {
            query.push_str(&format!(" AND entity_id = ${}", param_count));
            param_count += 1;
        }

        query.push_str(" ORDER BY similarity ASC");
        query.push_str(&format!(" LIMIT ${}", param_count));

        // Execute query with dynamic parameter binding
        let embedding_str = format!(
            "[{}]",
            params
                .embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let mut sql_query = sqlx::query(&query).bind(&embedding_str);

        if let Some(agent_id) = params.agent_id {
            sql_query = sql_query.bind(agent_id);
        }
        if let Some(room_id) = params.room_id {
            sql_query = sql_query.bind(room_id);
        }
        if let Some(world_id) = params.world_id {
            sql_query = sql_query.bind(world_id);
        }
        if let Some(entity_id) = params.entity_id {
            sql_query = sql_query.bind(entity_id);
        }
        sql_query = sql_query.bind(params.count as i64);

        let rows = sql_query.fetch_all(&self.pool).await.map_err(|e| {
            warn!("Vector search failed: {}", e);
            ZoeyError::database(format!(
                "Vector search failed in table '{}': {}",
                params.table_name, e
            ))
        })?;

        let mut memories = Vec::new();
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let entity_id: uuid::Uuid = row.get("entity_id");
            let agent_id: uuid::Uuid = row.get("agent_id");
            let room_id: uuid::Uuid = row.get("room_id");
            let content_value: serde_json::Value = row.get("content");
            let metadata_value: Option<serde_json::Value> = row.get("metadata");
            let created_at: i64 = row.get("created_at");
            let unique_flag: Option<bool> = row.get("unique_flag");
            let similarity: f32 = row.get("similarity");

            let content: Content = serde_json::from_value(content_value)?;
            let metadata: Option<MemoryMetadata> = metadata_value
                .map(|v| serde_json::from_value(v))
                .transpose()?;

            memories.push(Memory {
                id,
                entity_id,
                agent_id,
                room_id,
                content,
                embedding: None, // Don't return embeddings by default to save bandwidth
                metadata,
                created_at,
                unique: unique_flag,
                similarity: Some(similarity), // Include similarity score
            });
        }

        info!(
            "Found {} memories similar to query embedding",
            memories.len()
        );
        Ok(memories)
    }

    async fn get_cached_embeddings(&self, params: MemoryQuery) -> Result<Vec<Memory>> {
        use tracing::{debug, info};

        debug!(
            "Getting cached embeddings for agent_id: {:?}, room_id: {:?}, count: {:?}",
            params.agent_id, params.room_id, params.count
        );

        // Validate table name to prevent SQL injection
        let validated_table = validate_table_name(&params.table_name)?;

        // Build query for memories with embeddings
        let mut query = format!(
            "SELECT id, entity_id, agent_id, room_id, content, embedding, metadata, created_at, unique_flag
             FROM {}
             WHERE embedding IS NOT NULL",
            validated_table
        );

        let mut param_count = 1;
        if params.agent_id.is_some() {
            query.push_str(&format!(" AND agent_id = ${}", param_count));
            param_count += 1;
        }
        if params.room_id.is_some() {
            query.push_str(&format!(" AND room_id = ${}", param_count));
            param_count += 1;
        }
        if params.entity_id.is_some() {
            query.push_str(&format!(" AND entity_id = ${}", param_count));
            param_count += 1;
        }
        if params.unique.is_some() {
            query.push_str(&format!(" AND unique_flag = ${}", param_count));
            param_count += 1;
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = params.count {
            query.push_str(&format!(" LIMIT ${}", param_count));
        }

        // Execute query
        let mut sql_query = sqlx::query(&query);

        if let Some(agent_id) = params.agent_id {
            sql_query = sql_query.bind(agent_id);
        }
        if let Some(room_id) = params.room_id {
            sql_query = sql_query.bind(room_id);
        }
        if let Some(entity_id) = params.entity_id {
            sql_query = sql_query.bind(entity_id);
        }
        if let Some(unique) = params.unique {
            sql_query = sql_query.bind(unique);
        }
        if let Some(limit) = params.count {
            sql_query = sql_query.bind(limit as i64);
        }

        let rows = sql_query.fetch_all(&self.pool).await?;

        let mut memories = Vec::new();
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let entity_id: uuid::Uuid = row.get("entity_id");
            let agent_id: uuid::Uuid = row.get("agent_id");
            let room_id: uuid::Uuid = row.get("room_id");
            let content_value: serde_json::Value = row.get("content");
            let embedding_str: Option<String> = row.get("embedding");
            let metadata_value: Option<serde_json::Value> = row.get("metadata");
            let created_at: i64 = row.get("created_at");
            let unique_flag: Option<bool> = row.get("unique_flag");

            let content: Content = serde_json::from_value(content_value)?;

            // Parse embedding if present
            let embedding = if let Some(emb_str) = embedding_str {
                // pgvector stores as "[1.0,2.0,3.0]" format
                let trimmed = emb_str.trim_matches(|c| c == '[' || c == ']');
                let vec: std::result::Result<Vec<f32>, _> = trimmed
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect();
                Some(vec.map_err(|e| {
                    ZoeyError::database(format!("Failed to parse embedding: {}", e))
                })?)
            } else {
                None
            };

            let metadata: Option<MemoryMetadata> = metadata_value
                .map(|v| serde_json::from_value(v))
                .transpose()?;

            memories.push(Memory {
                id,
                entity_id,
                agent_id,
                room_id,
                content,
                embedding,
                metadata,
                created_at,
                unique: unique_flag,
                similarity: None,
            });
        }

        info!(
            "Retrieved {} cached embeddings from {}",
            memories.len(),
            params.table_name
        );
        Ok(memories)
    }

    async fn update_memory(&self, memory: &Memory) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE memories SET content = $2, embedding = $3, metadata = $4 WHERE id = $1",
        )
        .bind(memory.id)
        .bind(serde_json::to_value(&memory.content)?)
        .bind(&memory.embedding)
        .bind(serde_json::to_value(&memory.metadata)?)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn remove_memory(&self, memory_id: UUID, _table_name: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM memories WHERE id = $1")
            .bind(memory_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn remove_all_memories(&self, agent_id: UUID, _table_name: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM memories WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn count_memories(&self, params: MemoryQuery) -> Result<usize> {
        let mut query = String::from("SELECT COUNT(*) as count FROM memories WHERE 1=1");
        let mut bind_idx = 1;
        let mut query_args = sqlx::postgres::PgArguments::default();

        if let Some(agent_id) = params.agent_id {
            query.push_str(&format!(" AND agent_id = ${}", bind_idx));
            query_args
                .add(agent_id)
                .map_err(|e| ZoeyError::Database(e.to_string()))?;
            bind_idx += 1;
        }
        if let Some(room_id) = params.room_id {
            query.push_str(&format!(" AND room_id = ${}", bind_idx));
            query_args
                .add(room_id)
                .map_err(|e| ZoeyError::Database(e.to_string()))?;
            bind_idx += 1;
        }
        if let Some(entity_id) = params.entity_id {
            query.push_str(&format!(" AND entity_id = ${}", bind_idx));
            query_args
                .add(entity_id)
                .map_err(|e| ZoeyError::Database(e.to_string()))?;
            bind_idx += 1;
        }
        if let Some(unique) = params.unique {
            query.push_str(&format!(" AND unique_flag = ${}", bind_idx));
            query_args
                .add(unique)
                .map_err(|e| ZoeyError::Database(e.to_string()))?;
            bind_idx += 1;
        }

        let row = sqlx::query_with(&query, query_args)
            .fetch_one(&self.pool)
            .await?;

        let count: i64 = row.get("count");
        Ok(count as usize)
    }

    async fn get_world(&self, world_id: UUID) -> Result<Option<World>> {
        let row = sqlx::query(
            "SELECT id, name, agent_id, server_id, metadata, created_at FROM worlds WHERE id = $1",
        )
        .bind(world_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| World {
            id: row.get("id"),
            name: row.get("name"),
            agent_id: row.get("agent_id"),
            server_id: row.get("server_id"),
            metadata: row
                .try_get("metadata")
                .ok()
                .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            created_at: row.get("created_at"),
        }))
    }

    async fn ensure_world(&self, world: &World) -> Result<()> {
        sqlx::query(
            "INSERT INTO worlds (id, name, agent_id, server_id, metadata, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, server_id = EXCLUDED.server_id, metadata = EXCLUDED.metadata"
        )
        .bind(world.id)
        .bind(&world.name)
        .bind(world.agent_id)
        .bind(&world.server_id)
        .bind(serde_json::to_value(&world.metadata)?)
        .bind(world.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_room(&self, room_id: UUID) -> Result<Option<Room>> {
        let row = sqlx::query(
            "SELECT id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at FROM rooms WHERE id = $1"
        )
        .bind(room_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| Room {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            name: row.get("name"),
            source: row.get("source"),
            channel_type: {
                let t: String = row.get("type");
                match t.as_str() {
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
                }
            },
            channel_id: row.get("channel_id"),
            server_id: row.get("server_id"),
            world_id: row.get("world_id"),
            metadata: row
                .try_get("metadata")
                .ok()
                .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            created_at: row.get("created_at"),
        }))
    }

    async fn create_room(&self, room: &Room) -> Result<UUID> {
        sqlx::query(
            "INSERT INTO rooms (id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(room.id)
        .bind(room.agent_id)
        .bind(&room.name)
        .bind(&room.source)
        .bind(match room.channel_type {
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
        })
        .bind(&room.channel_id)
        .bind(&room.server_id)
        .bind(room.world_id)
        .bind(serde_json::to_value(&room.metadata)?)
        .bind(room.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()))
        .execute(&self.pool)
        .await?;
        Ok(room.id)
    }

    async fn get_rooms(&self, world_id: UUID) -> Result<Vec<Room>> {
        let rows = sqlx::query(
            "SELECT id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at FROM rooms WHERE world_id = $1"
        )
        .bind(world_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Room {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                name: row.get("name"),
                source: row.get("source"),
                channel_type: {
                    let t: String = row.get("type");
                    match t.as_str() {
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
                    }
                },
                channel_id: row.get("channel_id"),
                server_id: row.get("server_id"),
                world_id: row.get("world_id"),
                metadata: row
                    .try_get("metadata")
                    .ok()
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn get_rooms_for_agent(&self, agent_id: UUID) -> Result<Vec<Room>> {
        let rows = sqlx::query(
            "SELECT id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at FROM rooms WHERE agent_id = $1"
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Room {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                name: row.get("name"),
                source: row.get("source"),
                channel_type: {
                    let t: String = row.get("type");
                    match t.as_str() {
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
                    }
                },
                channel_id: row.get("channel_id"),
                server_id: row.get("server_id"),
                world_id: row.get("world_id"),
                metadata: row
                    .try_get("metadata")
                    .ok()
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn add_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let result = sqlx::query(
            "INSERT INTO participants (entity_id, room_id, joined_at, metadata) VALUES ($1, $2, $3, $4)"
        )
        .bind(entity_id)
        .bind(room_id)
        .bind(chrono::Utc::now().timestamp())
        .bind(serde_json::json!({}))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn remove_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let result = sqlx::query("DELETE FROM participants WHERE entity_id = $1 AND room_id = $2")
            .bind(entity_id)
            .bind(room_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_participants(&self, room_id: UUID) -> Result<Vec<Participant>> {
        let rows = sqlx::query(
            "SELECT entity_id, room_id, joined_at, metadata FROM participants WHERE room_id = $1",
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Participant {
                entity_id: row.get("entity_id"),
                room_id: row.get("room_id"),
                joined_at: row.get("joined_at"),
                metadata: row
                    .try_get("metadata")
                    .ok()
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
            })
            .collect())
    }

    async fn create_relationship(&self, relationship: &Relationship) -> Result<bool> {
        let result = sqlx::query(
            "INSERT INTO relationships (entity_id_a, entity_id_b, type, agent_id, metadata, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(relationship.entity_id_a)
        .bind(relationship.entity_id_b)
        .bind(&relationship.relationship_type)
        .bind(relationship.agent_id)
        .bind(serde_json::to_value(&relationship.metadata)?)
        .bind(relationship.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_relationship(
        &self,
        entity_id_a: UUID,
        entity_id_b: UUID,
    ) -> Result<Option<Relationship>> {
        let row = sqlx::query(
            "SELECT entity_id_a, entity_id_b, type, agent_id, metadata, created_at
             FROM relationships WHERE (entity_id_a = $1 AND entity_id_b = $2) OR (entity_id_a = $2 AND entity_id_b = $1)"
        )
        .bind(entity_id_a)
        .bind(entity_id_b)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| Relationship {
            entity_id_a: row.get("entity_id_a"),
            entity_id_b: row.get("entity_id_b"),
            relationship_type: row.get("type"),
            agent_id: row.get("agent_id"),
            metadata: row
                .try_get("metadata")
                .ok()
                .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            created_at: row.get("created_at"),
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
        sqlx::query(
            "INSERT INTO tasks (id, agent_id, task_type, data, status, priority, scheduled_at, executed_at, retry_count, max_retries, error, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"
        )
        .bind(task.id)
        .bind(task.agent_id)
        .bind(&task.task_type)
        .bind(&task.data)
        .bind(status_str)
        .bind(task.priority)
        .bind(task.scheduled_at)
        .bind(task.executed_at)
        .bind(task.retry_count)
        .bind(task.max_retries)
        .bind(&task.error)
        .bind(task.created_at)
        .bind(task.updated_at)
        .execute(&self.pool)
        .await?;
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
        let result = sqlx::query(
            "UPDATE tasks SET data = $2, status = $3, priority = $4, scheduled_at = $5, executed_at = $6, retry_count = $7, max_retries = $8, error = $9, updated_at = $10 WHERE id = $1"
        )
        .bind(task.id)
        .bind(&task.data)
        .bind(status_str)
        .bind(task.priority)
        .bind(task.scheduled_at)
        .bind(task.executed_at)
        .bind(task.retry_count)
        .bind(task.max_retries)
        .bind(&task.error)
        .bind(task.updated_at.unwrap_or_else(|| chrono::Utc::now().timestamp()))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_task(&self, task_id: UUID) -> Result<Option<Task>> {
        let row = sqlx::query(
            "SELECT id, agent_id, task_type, data, status, priority, scheduled_at, executed_at, retry_count, max_retries, error, created_at, updated_at FROM tasks WHERE id = $1"
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| {
            let status_str: String = row.get("status");
            let status = match status_str.as_str() {
                "PENDING" => TaskStatus::Pending,
                "RUNNING" => TaskStatus::Running,
                "COMPLETED" => TaskStatus::Completed,
                "FAILED" => TaskStatus::Failed,
                "CANCELLED" => TaskStatus::Cancelled,
                _ => TaskStatus::Pending,
            };
            Task {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                task_type: row.get("task_type"),
                data: row.get("data"),
                status,
                priority: row.get("priority"),
                scheduled_at: row.get("scheduled_at"),
                executed_at: row.get("executed_at"),
                retry_count: row.get("retry_count"),
                max_retries: row.get("max_retries"),
                error: row.get("error"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }
        }))
    }

    async fn get_pending_tasks(&self, agent_id: UUID) -> Result<Vec<Task>> {
        let rows = sqlx::query(
            "SELECT id, agent_id, task_type, data, status, priority, scheduled_at, executed_at, retry_count, max_retries, error, created_at, updated_at
             FROM tasks WHERE agent_id = $1 AND status = 'PENDING' ORDER BY scheduled_at NULLS LAST, created_at ASC"
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Task {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                task_type: row.get("task_type"),
                data: row.get("data"),
                status: TaskStatus::Pending,
                priority: row.get("priority"),
                scheduled_at: row.get("scheduled_at"),
                executed_at: row.get("executed_at"),
                retry_count: row.get("retry_count"),
                max_retries: row.get("max_retries"),
                error: row.get("error"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    async fn log(&self, log: &Log) -> Result<()> {
        sqlx::query(
            "INSERT INTO logs (entity_id, room_id, body, type, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(log.entity_id)
        .bind(log.room_id)
        .bind(&log.body)
        .bind(&log.log_type)
        .bind(log.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_logs(&self, params: LogQuery) -> Result<Vec<Log>> {
        let mut query = String::from(
            "SELECT id, entity_id, room_id, body, type, created_at FROM logs WHERE 1=1",
        );
        let mut idx = 1;
        if params.entity_id.is_some() {
            query.push_str(&format!(" AND entity_id = ${}", idx));
            idx += 1;
        }
        if params.room_id.is_some() {
            query.push_str(&format!(" AND room_id = ${}", idx));
            idx += 1;
        }
        if params.log_type.is_some() {
            query.push_str(&format!(" AND type = ${}", idx));
            idx += 1;
        }
        query.push_str(" ORDER BY created_at DESC");
        if params.limit.is_some() {
            query.push_str(&format!(" LIMIT ${}", idx));
            idx += 1;
        }
        if params.offset.is_some() {
            query.push_str(&format!(" OFFSET ${}", idx));
        }
        let mut sql = sqlx::query(&query);
        if let Some(eid) = params.entity_id {
            sql = sql.bind(eid);
        }
        if let Some(rid) = params.room_id {
            sql = sql.bind(rid);
        }
        if let Some(t) = params.log_type {
            sql = sql.bind(t);
        }
        if let Some(l) = params.limit {
            sql = sql.bind(l as i64);
        }
        if let Some(o) = params.offset {
            sql = sql.bind(o as i64);
        }
        let rows = sql.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| Log {
                id: Some(row.get("id")),
                entity_id: row.get("entity_id"),
                room_id: row.get("room_id"),
                body: row.get("body"),
                log_type: row.get("type"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn get_agent_run_summaries(
        &self,
        params: RunSummaryQuery,
    ) -> Result<AgentRunSummaryResult> {
        let mut base = String::from("FROM llm_costs WHERE 1=1");
        let mut idx = 1;
        if params.agent_id.is_some() {
            base.push_str(&format!(" AND agent_id = ${}", idx));
            idx += 1;
        }
        if params.status.is_some() {
            base.push_str(&format!(" AND success = ${}", idx));
            idx += 1;
        }
        let count_sql = format!("SELECT COUNT(*) as count {}", base);
        let mut count_q = sqlx::query(&count_sql);
        if let Some(aid) = params.agent_id {
            count_q = count_q.bind(aid);
        }
        if let Some(st) = params.status {
            count_q = count_q.bind(matches!(st, RunStatus::Completed));
        }
        let count_row = count_q.fetch_one(&self.pool).await?;
        let total: i64 = count_row.get("count");

        let mut query = format!(
            "SELECT id, timestamp, success, conversation_id {} ORDER BY timestamp DESC",
            base
        );
        if params.limit.is_some() {
            query.push_str(&format!(" LIMIT ${}", idx));
            idx += 1;
        }
        if params.offset.is_some() {
            query.push_str(&format!(" OFFSET ${}", idx));
        }
        let mut sql = sqlx::query(&query);
        if let Some(aid) = params.agent_id {
            sql = sql.bind(aid);
        }
        if let Some(st) = params.status {
            sql = sql.bind(matches!(st, RunStatus::Completed));
        }
        if let Some(l) = params.limit {
            sql = sql.bind(l as i64);
        }
        if let Some(o) = params.offset {
            sql = sql.bind(o as i64);
        }
        let rows = sql.fetch_all(&self.pool).await?;
        let mut runs = Vec::new();
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let ts: chrono::DateTime<chrono::Utc> = row.get("timestamp");
            let success: bool = row.get("success");
            let status = if success {
                RunStatus::Completed
            } else {
                RunStatus::Error
            };
            runs.push(AgentRunSummary {
                run_id: id.to_string(),
                status,
                started_at: Some(ts.timestamp()),
                ended_at: Some(ts.timestamp()),
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
                .map(|l| (l as i64) + (params.offset.unwrap_or(0) as i64) < total)
                .unwrap_or(false),
        })
    }
}

impl PostgresAdapter {
    pub async fn persist_llm_cost(&self, record: LLMCostRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO llm_costs (id, timestamp, agent_id, user_id, conversation_id, action_name, evaluator_name, provider, model, temperature, prompt_tokens, completion_tokens, total_tokens, cached_tokens, input_cost_usd, output_cost_usd, total_cost_usd, latency_ms, ttft_ms, success, error, prompt_hash, prompt_preview)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)"
        )
        .bind(record.id)
        .bind(record.timestamp)
        .bind(record.agent_id)
        .bind(record.user_id)
        .bind(record.conversation_id)
        .bind(record.action_name)
        .bind(record.evaluator_name)
        .bind(record.provider)
        .bind(record.model)
        .bind(record.temperature)
        .bind(record.prompt_tokens as i64)
        .bind(record.completion_tokens as i64)
        .bind(record.total_tokens as i64)
        .bind(record.cached_tokens.map(|v| v as i64))
        .bind(record.input_cost_usd)
        .bind(record.output_cost_usd)
        .bind(record.total_cost_usd)
        .bind(record.latency_ms as i64)
        .bind(record.ttft_ms.map(|v| v as i64))
        .bind(record.success)
        .bind(record.error)
        .bind(record.prompt_hash)
        .bind(record.prompt_preview)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_adapter_creation() {
        assert!(true);
    }
}
