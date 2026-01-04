//! SQLite database adapter
//!
//! Implements core database operations for SQLite.
//! Note: Vector search is not supported - for embedding-based search, use PostgreSQL with pgvector.

use async_trait::async_trait;
use zoey_core::observability::types::LLMCostRecord;
use zoey_core::{types::*, ZoeyError, Result};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use tracing::{debug, info};

/// Helper macro for not implemented methods (vector search only)
macro_rules! not_implemented {
    ($method:expr) => {
        Err(ZoeyError::database(format!(
            "SQLite: {} not implemented. Use PostgreSQL for full functionality",
            $method
        )))
    };
}

/// SQLite database adapter
pub struct SqliteAdapter {
    pool: SqlitePool,
}

impl SqliteAdapter {
    /// Create a new SQLite adapter
    pub async fn new(database_path: &str) -> Result<Self> {
        info!("Opening SQLite database at: {}", database_path);

        let opts = SqliteConnectOptions::from_str(database_path)
            .map_err(|e| ZoeyError::database(format!("Invalid SQLite URL: {}", e)))?
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(opts)
            .await
            .map_err(|e| ZoeyError::DatabaseSqlx(e))?;

        Ok(Self { pool })
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<()> {
        debug!("Initializing SQLite schema...");

        // Enable foreign keys
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&self.pool)
            .await?;

        // ============================================================
        // CREATE TABLES
        // ============================================================

        // Agents table (root entity)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                character TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Entities table (belongs to agent)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS entities (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                name TEXT,
                username TEXT,
                email TEXT,
                avatar_url TEXT,
                metadata TEXT DEFAULT '{}',
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Worlds table (belongs to agent)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS worlds (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                server_id TEXT,
                metadata TEXT DEFAULT '{}',
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Rooms table (belongs to world)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS rooms (
                id TEXT PRIMARY KEY,
                agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
                name TEXT NOT NULL,
                source TEXT NOT NULL,
                type TEXT NOT NULL,
                channel_id TEXT,
                server_id TEXT,
                world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
                metadata TEXT DEFAULT '{}',
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Memories table (core data)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                room_id TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
                content TEXT NOT NULL,
                metadata TEXT,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                unique_flag INTEGER DEFAULT 0
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Participants junction table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS participants (
                entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                room_id TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
                joined_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                metadata TEXT DEFAULT '{}',
                PRIMARY KEY (entity_id, room_id)
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Relationships table (includes type in unique constraint)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS relationships (
                id TEXT PRIMARY KEY,
                entity_id_a TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                entity_id_b TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                type TEXT NOT NULL,
                agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                metadata TEXT DEFAULT '{}',
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                UNIQUE (entity_id_a, entity_id_b, type)
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Components table (ECS-style)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS components (
                id TEXT PRIMARY KEY,
                entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
                source_entity_id TEXT REFERENCES entities(id) ON DELETE SET NULL,
                type TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER,
                UNIQUE (entity_id, world_id, type, source_entity_id)
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Tasks table (scheduled work)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                task_type TEXT NOT NULL,
                data TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'PENDING',
                priority INTEGER DEFAULT 0,
                scheduled_at INTEGER,
                executed_at INTEGER,
                retry_count INTEGER DEFAULT 0,
                max_retries INTEGER DEFAULT 3,
                error TEXT,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Logs table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS logs (
                id TEXT PRIMARY KEY,
                entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                room_id TEXT REFERENCES rooms(id) ON DELETE SET NULL,
                body TEXT NOT NULL,
                type TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // ============================================================
        // CREATE INDICES
        // ============================================================

        // -- Entities indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_agent_id ON entities(agent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_username ON entities(username)")
            .execute(&self.pool)
            .await?;

        // -- Worlds indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_worlds_agent_id ON worlds(agent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_worlds_server_id ON worlds(server_id)")
            .execute(&self.pool)
            .await?;

        // -- Rooms indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_world_id ON rooms(world_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_agent_id ON rooms(agent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_channel_id ON rooms(channel_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_rooms_source_server ON rooms(source, server_id)",
        )
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

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tasks_status_scheduled ON tasks(status, scheduled_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tasks_agent_status ON tasks(agent_id, status)")
            .execute(&self.pool)
            .await?;

        // -- Logs indices --
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_logs_entity_id ON logs(entity_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_logs_room_id ON logs(room_id)")
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
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,

                agent_id TEXT NOT NULL,
                user_id TEXT,
                conversation_id TEXT,
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

                latency_ms INTEGER NOT NULL,
                ttft_ms INTEGER,

                success INTEGER NOT NULL,
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
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                cost_record_id TEXT REFERENCES llm_costs(id) ON DELETE CASCADE,
                agent_id TEXT NOT NULL,
                conversation_id TEXT,
                prompt_hash TEXT NOT NULL,
                prompt_text TEXT,
                prompt_length INTEGER NOT NULL,
                sanitized INTEGER NOT NULL,
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

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_llm_costs_conversation ON llm_costs(conversation_id)",
        )
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

        info!("SQLite schema initialized successfully");
        Ok(())
    }
}

#[async_trait]
impl IDatabaseAdapter for SqliteAdapter {
    fn db(&self) -> &dyn std::any::Any {
        &self.pool
    }

    async fn initialize(&mut self, _config: Option<serde_json::Value>) -> Result<()> {
        self.init_schema().await
    }

    async fn is_ready(&self) -> Result<bool> {
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

        // SQLite-safe identifier validation
        fn validate_identifier(name: &str) -> Result<&str> {
            if name.len() > 64 {
                return Err(ZoeyError::validation(format!(
                    "Identifier too long: {} (max 64 characters)",
                    name.len()
                )));
            }
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(ZoeyError::validation(format!(
                    "Invalid identifier '{}': only alphanumeric and underscore allowed",
                    name
                )));
            }
            Ok(name)
        }

        for plugin in plugins {
            if let Some(schema) = plugin.schema {
                if options.verbose {
                    tracing::info!("Applying schema for plugin '{}' (SQLite)", plugin.name);
                }

                let schema_obj = schema.as_object().ok_or_else(|| {
                    ZoeyError::validation(format!(
                        "Invalid schema for plugin '{}': expected JSON object",
                        plugin.name
                    ))
                })?;

                for (table_name, table_def) in schema_obj.iter() {
                    let table_name = validate_identifier(table_name)?;

                    let columns_obj = table_def
                        .get("columns")
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| {
                            ZoeyError::validation(format!(
                                "Invalid table definition for '{}': missing 'columns'",
                                table_name
                            ))
                        })?;

                    let mut cols_sql: Vec<String> = Vec::new();
                    for (col_name, col_type_val) in columns_obj.iter() {
                        let col_name = validate_identifier(col_name)?;
                        let mut col_type = col_type_val
                            .as_str()
                            .ok_or_else(|| {
                                ZoeyError::validation(format!(
                                    "Invalid type for column '{}.{}'",
                                    table_name, col_name
                                ))
                            })?
                            .to_string();

                        // Map common Postgres types to SQLite equivalents
                        col_type = col_type
                            .replace("UUID", "TEXT")
                            .replace("TIMESTAMP", "INTEGER")
                            .replace("JSONB", "TEXT")
                            .replace("BOOLEAN", "INTEGER")
                            .replace("TEXT", "TEXT");

                        cols_sql.push(format!("{} {}", col_name, col_type));
                    }

                    let create_sql = format!(
                        "CREATE TABLE IF NOT EXISTS {} ({})",
                        table_name,
                        cols_sql.join(", ")
                    );

                    if options.verbose {
                        tracing::debug!("{}", create_sql);
                    }
                    if !options.dry_run {
                        sqlx::query(&create_sql).execute(&self.pool).await?;
                    }
                }

                if options.verbose {
                    tracing::info!("✓ Schema applied for plugin '{}' (SQLite)", plugin.name);
                }
            }
        }
        Ok(())
    }

    async fn persist_llm_cost(&self, record: LLMCostRecord) -> zoey_core::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO llm_costs (
                id, timestamp, agent_id, user_id, conversation_id, action_name, evaluator_name,
                provider, model, temperature, prompt_tokens, completion_tokens, total_tokens, cached_tokens,
                input_cost_usd, output_cost_usd, total_cost_usd, latency_ms, ttft_ms,
                success, error, prompt_hash, prompt_preview
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(record.id.to_string())
        .bind(record.timestamp.timestamp())
        .bind(record.agent_id.to_string())
        .bind(record.user_id)
        .bind(record.conversation_id.map(|v| v.to_string()))
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
        .bind(if record.success { 1 } else { 0 })
        .bind(record.error)
        .bind(record.prompt_hash)
        .bind(record.prompt_preview)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Agent Operations - Fully Implemented

    async fn get_agent(&self, agent_id: UUID) -> Result<Option<Agent>> {
        let row = sqlx::query(
            "SELECT id, name, character, created_at, updated_at FROM agents WHERE id = ?",
        )
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let id_str: String = row.get("id");
                let character_str: String = row.get("character");

                Ok(Some(Agent {
                    id: uuid::Uuid::parse_str(&id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    name: row.get("name"),
                    character: serde_json::from_str(&character_str)?,
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_agents(&self) -> Result<Vec<Agent>> {
        let rows = sqlx::query("SELECT id, name, character, created_at, updated_at FROM agents")
            .fetch_all(&self.pool)
            .await?;

        let mut agents = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let character_str: String = row.get("character");

            agents.push(Agent {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                name: row.get("name"),
                character: serde_json::from_str(&character_str)?,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(agents)
    }

    async fn create_agent(&self, agent: &Agent) -> Result<bool> {
        let result = sqlx::query(
            "INSERT INTO agents (id, name, character, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(agent.id.to_string())
        .bind(&agent.name)
        .bind(serde_json::to_string(&agent.character)?)
        .bind(agent.created_at)
        .bind(agent.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_agent(&self, agent_id: UUID, agent: &Agent) -> Result<bool> {
        let result =
            sqlx::query("UPDATE agents SET name = ?, character = ?, updated_at = ? WHERE id = ?")
                .bind(&agent.name)
                .bind(serde_json::to_string(&agent.character)?)
                .bind(chrono::Utc::now().timestamp())
                .bind(agent_id.to_string())
                .execute(&self.pool)
                .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_agent(&self, agent_id: UUID) -> Result<bool> {
        let result = sqlx::query("DELETE FROM agents WHERE id = ?")
            .bind(agent_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn ensure_embedding_dimension(&self, _dimension: usize) -> Result<()> {
        // SQLite doesn't have native vector support
        // Would use a library like sqlite-vss or fall back to BM25
        Ok(())
    }

    async fn get_entities_by_ids(&self, entity_ids: Vec<UUID>) -> Result<Vec<Entity>> {
        if entity_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = entity_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!("SELECT id, agent_id, name, username, email, avatar_url, metadata, created_at FROM entities WHERE id IN ({})", placeholders);

        let mut query_builder = sqlx::query(&query);
        for id in &entity_ids {
            query_builder = query_builder.bind(id.to_string());
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut entities = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let agent_id_str: String = row.get("agent_id");
            let metadata_str: String = row.get("metadata");

            entities.push(Entity {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                agent_id: uuid::Uuid::parse_str(&agent_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                name: row.get("name"),
                username: row.get("username"),
                email: row.get("email"),
                avatar_url: row.get("avatar_url"),
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                created_at: row.get("created_at"),
            });
        }

        Ok(entities)
    }

    async fn get_entities_for_room(
        &self,
        room_id: UUID,
        _include_components: bool,
    ) -> Result<Vec<Entity>> {
        let rows = sqlx::query(
            r#"
            SELECT e.id, e.agent_id, e.name, e.username, e.email, e.avatar_url, e.metadata, e.created_at
            FROM entities e
            INNER JOIN participants p ON e.id = p.entity_id
            WHERE p.room_id = ?
            "#,
        )
        .bind(room_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut entities = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let agent_id_str: String = row.get("agent_id");
            let metadata_str: String = row.get("metadata");

            entities.push(Entity {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                agent_id: uuid::Uuid::parse_str(&agent_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                name: row.get("name"),
                username: row.get("username"),
                email: row.get("email"),
                avatar_url: row.get("avatar_url"),
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                created_at: row.get("created_at"),
            });
        }

        Ok(entities)
    }

    async fn create_entities(&self, entities: Vec<Entity>) -> Result<bool> {
        for entity in entities {
            sqlx::query(
                "INSERT OR REPLACE INTO entities (id, agent_id, name, username, email, avatar_url, metadata, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(entity.id.to_string())
            .bind(entity.agent_id.to_string())
            .bind(&entity.name)
            .bind(&entity.username)
            .bind(&entity.email)
            .bind(&entity.avatar_url)
            .bind(serde_json::to_string(&entity.metadata)?)
            .bind(entity.created_at)
            .execute(&self.pool)
            .await?;
        }
        Ok(true)
    }

    async fn update_entity(&self, entity: &Entity) -> Result<()> {
        sqlx::query(
            "UPDATE entities SET name = ?, username = ?, email = ?, avatar_url = ?, metadata = ? WHERE id = ?"
        )
        .bind(&entity.name)
        .bind(&entity.username)
        .bind(&entity.email)
        .bind(&entity.avatar_url)
        .bind(serde_json::to_string(&entity.metadata)?)
        .bind(entity.id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_entity_by_id(&self, entity_id: UUID) -> Result<Option<Entity>> {
        let row = sqlx::query(
            "SELECT id, agent_id, name, username, email, avatar_url, metadata, created_at FROM entities WHERE id = ?"
        )
        .bind(entity_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let id_str: String = row.get("id");
                let agent_id_str: String = row.get("agent_id");
                let metadata_str: String = row.get("metadata");

                Ok(Some(Entity {
                    id: uuid::Uuid::parse_str(&id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    agent_id: uuid::Uuid::parse_str(&agent_id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    name: row.get("name"),
                    username: row.get("username"),
                    email: row.get("email"),
                    avatar_url: row.get("avatar_url"),
                    metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                    created_at: row.get("created_at"),
                }))
            }
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
            "SELECT id, entity_id, world_id, source_entity_id, type, data, created_at, updated_at FROM components WHERE entity_id = ? AND type = ?",
        );
        let mut bindings: Vec<String> = vec![entity_id.to_string(), component_type.to_string()];

        if let Some(wid) = world_id {
            query.push_str(" AND world_id = ?");
            bindings.push(wid.to_string());
        }
        if let Some(sid) = source_entity_id {
            query.push_str(" AND source_entity_id = ?");
            bindings.push(sid.to_string());
        } else {
            query.push_str(" AND source_entity_id IS NULL");
        }

        let mut query_builder = sqlx::query(&query);
        for binding in &bindings {
            query_builder = query_builder.bind(binding);
        }

        let row = query_builder.fetch_optional(&self.pool).await?;

        match row {
            Some(row) => {
                let id_str: String = row.get("id");
                let entity_id_str: String = row.get("entity_id");
                let world_id_str: String = row.get("world_id");
                let source_entity_id_str: Option<String> = row.get("source_entity_id");
                let data_str: String = row.get("data");

                Ok(Some(Component {
                    id: uuid::Uuid::parse_str(&id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    entity_id: uuid::Uuid::parse_str(&entity_id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    world_id: uuid::Uuid::parse_str(&world_id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    source_entity_id: source_entity_id_str
                        .map(|s| uuid::Uuid::parse_str(&s).ok())
                        .flatten(),
                    component_type: row.get("type"),
                    data: serde_json::from_str(&data_str)?,
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_components(
        &self,
        entity_id: UUID,
        world_id: Option<UUID>,
        source_entity_id: Option<UUID>,
    ) -> Result<Vec<Component>> {
        let mut query = String::from(
            "SELECT id, entity_id, world_id, source_entity_id, type, data, created_at, updated_at FROM components WHERE entity_id = ?",
        );
        let mut bindings: Vec<String> = vec![entity_id.to_string()];

        if let Some(wid) = world_id {
            query.push_str(" AND world_id = ?");
            bindings.push(wid.to_string());
        }
        if let Some(sid) = source_entity_id {
            query.push_str(" AND source_entity_id = ?");
            bindings.push(sid.to_string());
        }

        let mut query_builder = sqlx::query(&query);
        for binding in &bindings {
            query_builder = query_builder.bind(binding);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut components = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let entity_id_str: String = row.get("entity_id");
            let world_id_str: String = row.get("world_id");
            let source_entity_id_str: Option<String> = row.get("source_entity_id");
            let data_str: String = row.get("data");

            components.push(Component {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                entity_id: uuid::Uuid::parse_str(&entity_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                world_id: uuid::Uuid::parse_str(&world_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                source_entity_id: source_entity_id_str
                    .map(|s| uuid::Uuid::parse_str(&s).ok())
                    .flatten(),
                component_type: row.get("type"),
                data: serde_json::from_str(&data_str)?,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(components)
    }

    async fn create_component(&self, component: &Component) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO components (id, entity_id, world_id, source_entity_id, type, data, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (entity_id, world_id, type, source_entity_id) DO UPDATE SET
                data = excluded.data,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(component.id.to_string())
        .bind(component.entity_id.to_string())
        .bind(component.world_id.to_string())
        .bind(component.source_entity_id.map(|id| id.to_string()))
        .bind(&component.component_type)
        .bind(serde_json::to_string(&component.data)?)
        .bind(component.created_at)
        .bind(component.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_component(&self, component: &Component) -> Result<()> {
        sqlx::query("UPDATE components SET data = ?, updated_at = ? WHERE id = ?")
            .bind(serde_json::to_string(&component.data)?)
            .bind(chrono::Utc::now().timestamp())
            .bind(component.id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn delete_component(&self, component_id: UUID) -> Result<()> {
        sqlx::query("DELETE FROM components WHERE id = ?")
            .bind(component_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_memories(&self, params: MemoryQuery) -> Result<Vec<Memory>> {
        let mut query = String::from("SELECT id, entity_id, agent_id, room_id, content, metadata, created_at, unique_flag FROM memories WHERE 1=1");

        let mut bindings: Vec<String> = Vec::new();

        if let Some(agent_id) = params.agent_id {
            query.push_str(" AND agent_id = ?");
            bindings.push(agent_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            query.push_str(" AND room_id = ?");
            bindings.push(room_id.to_string());
        }
        if let Some(entity_id) = params.entity_id {
            query.push_str(" AND entity_id = ?");
            bindings.push(entity_id.to_string());
        }
        if let Some(unique) = params.unique {
            query.push_str(" AND unique_flag = ?");
            bindings.push(if unique {
                "1".to_string()
            } else {
                "0".to_string()
            });
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(count) = params.count {
            query.push_str(&format!(" LIMIT {}", count));
        }

        let mut query_builder = sqlx::query(&query);
        for binding in &bindings {
            query_builder = query_builder.bind(binding);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut memories = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let entity_id_str: String = row.get("entity_id");
            let agent_id_str: String = row.get("agent_id");
            let room_id_str: String = row.get("room_id");
            let content_str: String = row.get("content");
            let metadata_str: Option<String> = row.get("metadata");
            let unique_flag: i32 = row.get("unique_flag");

            memories.push(Memory {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                entity_id: uuid::Uuid::parse_str(&entity_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                agent_id: uuid::Uuid::parse_str(&agent_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                room_id: uuid::Uuid::parse_str(&room_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                content: serde_json::from_str(&content_str)?,
                embedding: None, // SQLite doesn't store embeddings natively
                metadata: metadata_str.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: row.get("created_at"),
                unique: Some(unique_flag != 0),
                similarity: None,
            });
        }

        Ok(memories)
    }

    async fn create_memory(&self, memory: &Memory, _table_name: &str) -> Result<UUID> {
        sqlx::query(
            "INSERT INTO memories (id, entity_id, agent_id, room_id, content, metadata, created_at, unique_flag)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(memory.id.to_string())
        .bind(memory.entity_id.to_string())
        .bind(memory.agent_id.to_string())
        .bind(memory.room_id.to_string())
        .bind(serde_json::to_string(&memory.content)?)
        .bind(memory.metadata.as_ref().map(|m| serde_json::to_string(m).ok()).flatten())
        .bind(memory.created_at)
        .bind(if memory.unique.unwrap_or(false) { 1 } else { 0 })
        .execute(&self.pool)
        .await?;

        Ok(memory.id)
    }

    async fn search_memories_by_embedding(
        &self,
        _params: SearchMemoriesParams,
    ) -> Result<Vec<Memory>> {
        // SQLite doesn't have native vector search - would need sqlite-vss extension
        not_implemented!(
            "search_memories_by_embedding - vector search requires sqlite-vss extension"
        )
    }

    async fn get_cached_embeddings(&self, _params: MemoryQuery) -> Result<Vec<Memory>> {
        // No embedding support in basic SQLite
        Ok(vec![])
    }

    async fn update_memory(&self, memory: &Memory) -> Result<bool> {
        let result = sqlx::query("UPDATE memories SET content = ?, metadata = ? WHERE id = ?")
            .bind(serde_json::to_string(&memory.content)?)
            .bind(
                memory
                    .metadata
                    .as_ref()
                    .map(|m| serde_json::to_string(m).ok())
                    .flatten(),
            )
            .bind(memory.id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn remove_memory(&self, memory_id: UUID, _table_name: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(memory_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn remove_all_memories(&self, agent_id: UUID, _table_name: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM memories WHERE agent_id = ?")
            .bind(agent_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn count_memories(&self, params: MemoryQuery) -> Result<usize> {
        let mut query = String::from("SELECT COUNT(*) as count FROM memories WHERE 1=1");

        let mut bindings: Vec<String> = Vec::new();

        if let Some(agent_id) = params.agent_id {
            query.push_str(" AND agent_id = ?");
            bindings.push(agent_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            query.push_str(" AND room_id = ?");
            bindings.push(room_id.to_string());
        }
        if let Some(entity_id) = params.entity_id {
            query.push_str(" AND entity_id = ?");
            bindings.push(entity_id.to_string());
        }
        if let Some(unique) = params.unique {
            query.push_str(" AND unique_flag = ?");
            bindings.push(if unique {
                "1".to_string()
            } else {
                "0".to_string()
            });
        }

        let mut query_builder = sqlx::query(&query);
        for binding in &bindings {
            query_builder = query_builder.bind(binding);
        }

        let row = query_builder.fetch_one(&self.pool).await?;
        let count: i64 = row.get("count");

        Ok(count as usize)
    }

    async fn get_world(&self, world_id: UUID) -> Result<Option<World>> {
        let row = sqlx::query(
            "SELECT id, name, agent_id, server_id, metadata, created_at FROM worlds WHERE id = ?",
        )
        .bind(world_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let id_str: String = row.get("id");
                let agent_id_str: String = row.get("agent_id");
                let metadata_str: String = row.get("metadata");

                Ok(Some(World {
                    id: uuid::Uuid::parse_str(&id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    name: row.get("name"),
                    agent_id: uuid::Uuid::parse_str(&agent_id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    server_id: row.get("server_id"),
                    metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                    created_at: row.get("created_at"),
                }))
            }
            None => Ok(None),
        }
    }

    async fn ensure_world(&self, world: &World) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO worlds (id, name, agent_id, server_id, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (id) DO UPDATE SET
                name = excluded.name,
                server_id = excluded.server_id,
                metadata = excluded.metadata
            "#,
        )
        .bind(world.id.to_string())
        .bind(&world.name)
        .bind(world.agent_id.to_string())
        .bind(&world.server_id)
        .bind(serde_json::to_string(&world.metadata)?)
        .bind(
            world
                .created_at
                .unwrap_or_else(|| chrono::Utc::now().timestamp()),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_room(&self, room_id: UUID) -> Result<Option<Room>> {
        let row = sqlx::query(
            "SELECT id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at FROM rooms WHERE id = ?",
        )
        .bind(room_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let id_str: String = row.get("id");
                let agent_id_str: Option<String> = row.get("agent_id");
                let world_id_str: String = row.get("world_id");
                let metadata_str: String = row.get("metadata");
                let type_str: String = row.get("type");

                Ok(Some(Room {
                    id: uuid::Uuid::parse_str(&id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    agent_id: agent_id_str
                        .map(|s| uuid::Uuid::parse_str(&s).ok())
                        .flatten(),
                    name: row.get("name"),
                    source: row.get("source"),
                    channel_type: serde_json::from_str(&format!("\"{}\"", type_str))
                        .unwrap_or(ChannelType::Unknown),
                    channel_id: row.get("channel_id"),
                    server_id: row.get("server_id"),
                    world_id: uuid::Uuid::parse_str(&world_id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                    created_at: row.get("created_at"),
                }))
            }
            None => Ok(None),
        }
    }

    async fn create_room(&self, room: &Room) -> Result<UUID> {
        let channel_type_str = serde_json::to_string(&room.channel_type)?
            .trim_matches('"')
            .to_string();

        sqlx::query(
            r#"
            INSERT INTO rooms (id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(room.id.to_string())
        .bind(room.agent_id.map(|id| id.to_string()))
        .bind(&room.name)
        .bind(&room.source)
        .bind(&channel_type_str)
        .bind(&room.channel_id)
        .bind(&room.server_id)
        .bind(room.world_id.to_string())
        .bind(serde_json::to_string(&room.metadata)?)
        .bind(room.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()))
        .execute(&self.pool)
        .await?;

        Ok(room.id)
    }

    async fn get_rooms(&self, world_id: UUID) -> Result<Vec<Room>> {
        let rows = sqlx::query(
            "SELECT id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at FROM rooms WHERE world_id = ?",
        )
        .bind(world_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut rooms = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let agent_id_str: Option<String> = row.get("agent_id");
            let world_id_str: String = row.get("world_id");
            let metadata_str: String = row.get("metadata");
            let type_str: String = row.get("type");

            rooms.push(Room {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                agent_id: agent_id_str
                    .map(|s| uuid::Uuid::parse_str(&s).ok())
                    .flatten(),
                name: row.get("name"),
                source: row.get("source"),
                channel_type: serde_json::from_str(&format!("\"{}\"", type_str))
                    .unwrap_or(ChannelType::Unknown),
                channel_id: row.get("channel_id"),
                server_id: row.get("server_id"),
                world_id: uuid::Uuid::parse_str(&world_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                created_at: row.get("created_at"),
            });
        }

        Ok(rooms)
    }

    async fn get_rooms_for_agent(&self, agent_id: UUID) -> Result<Vec<Room>> {
        let rows = sqlx::query(
            "SELECT id, agent_id, name, source, type, channel_id, server_id, world_id, metadata, created_at FROM rooms WHERE agent_id = ?",
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut rooms = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let agent_id_str: Option<String> = row.get("agent_id");
            let world_id_str: String = row.get("world_id");
            let metadata_str: String = row.get("metadata");
            let type_str: String = row.get("type");

            rooms.push(Room {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                agent_id: agent_id_str
                    .map(|s| uuid::Uuid::parse_str(&s).ok())
                    .flatten(),
                name: row.get("name"),
                source: row.get("source"),
                channel_type: serde_json::from_str(&format!("\"{}\"", type_str))
                    .unwrap_or(ChannelType::Unknown),
                channel_id: row.get("channel_id"),
                server_id: row.get("server_id"),
                world_id: uuid::Uuid::parse_str(&world_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                created_at: row.get("created_at"),
            });
        }

        Ok(rooms)
    }

    async fn add_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO participants (entity_id, room_id, joined_at, metadata)
            VALUES (?, ?, ?, '{}')
            ON CONFLICT (entity_id, room_id) DO NOTHING
            "#,
        )
        .bind(entity_id.to_string())
        .bind(room_id.to_string())
        .bind(chrono::Utc::now().timestamp())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn remove_participant(&self, entity_id: UUID, room_id: UUID) -> Result<bool> {
        let result = sqlx::query("DELETE FROM participants WHERE entity_id = ? AND room_id = ?")
            .bind(entity_id.to_string())
            .bind(room_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_participants(&self, room_id: UUID) -> Result<Vec<Participant>> {
        let rows = sqlx::query(
            "SELECT entity_id, room_id, joined_at, metadata FROM participants WHERE room_id = ?",
        )
        .bind(room_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut participants = Vec::new();
        for row in rows {
            let entity_id_str: String = row.get("entity_id");
            let room_id_str: String = row.get("room_id");
            let metadata_str: String = row.get("metadata");

            participants.push(Participant {
                entity_id: uuid::Uuid::parse_str(&entity_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                room_id: uuid::Uuid::parse_str(&room_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                joined_at: row.get("joined_at"),
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
            });
        }

        Ok(participants)
    }

    async fn create_relationship(&self, relationship: &Relationship) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO relationships (id, entity_id_a, entity_id_b, type, agent_id, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (entity_id_a, entity_id_b, type) DO UPDATE SET
                metadata = excluded.metadata
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(relationship.entity_id_a.to_string())
        .bind(relationship.entity_id_b.to_string())
        .bind(&relationship.relationship_type)
        .bind(relationship.agent_id.to_string())
        .bind(serde_json::to_string(&relationship.metadata)?)
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
            "SELECT entity_id_a, entity_id_b, type, agent_id, metadata, created_at FROM relationships WHERE entity_id_a = ? AND entity_id_b = ?",
        )
        .bind(entity_id_a.to_string())
        .bind(entity_id_b.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let entity_a_str: String = row.get("entity_id_a");
                let entity_b_str: String = row.get("entity_id_b");
                let agent_id_str: String = row.get("agent_id");
                let metadata_str: String = row.get("metadata");

                Ok(Some(Relationship {
                    entity_id_a: uuid::Uuid::parse_str(&entity_a_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    entity_id_b: uuid::Uuid::parse_str(&entity_b_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    relationship_type: row.get("type"),
                    agent_id: uuid::Uuid::parse_str(&agent_id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                    created_at: row.get("created_at"),
                }))
            }
            None => Ok(None),
        }
    }

    async fn create_task(&self, task: &Task) -> Result<UUID> {
        let status_str = serde_json::to_string(&task.status)?
            .trim_matches('"')
            .to_string();

        sqlx::query(
            r#"
            INSERT INTO tasks (id, agent_id, task_type, data, status, priority, scheduled_at, executed_at, retry_count, max_retries, error, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(task.id.to_string())
        .bind(task.agent_id.to_string())
        .bind(&task.task_type)
        .bind(serde_json::to_string(&task.data)?)
        .bind(&status_str)
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
        let status_str = serde_json::to_string(&task.status)?
            .trim_matches('"')
            .to_string();

        let result = sqlx::query(
            r#"
            UPDATE tasks SET 
                status = ?, 
                priority = ?, 
                scheduled_at = ?, 
                executed_at = ?, 
                retry_count = ?, 
                error = ?, 
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&status_str)
        .bind(task.priority)
        .bind(task.scheduled_at)
        .bind(task.executed_at)
        .bind(task.retry_count)
        .bind(&task.error)
        .bind(chrono::Utc::now().timestamp())
        .bind(task.id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_task(&self, task_id: UUID) -> Result<Option<Task>> {
        let row = sqlx::query(
            "SELECT id, agent_id, task_type, data, status, priority, scheduled_at, executed_at, retry_count, max_retries, error, created_at, updated_at FROM tasks WHERE id = ?",
        )
        .bind(task_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let id_str: String = row.get("id");
                let agent_id_str: String = row.get("agent_id");
                let data_str: String = row.get("data");
                let status_str: String = row.get("status");

                Ok(Some(Task {
                    id: uuid::Uuid::parse_str(&id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    agent_id: uuid::Uuid::parse_str(&agent_id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                    task_type: row.get("task_type"),
                    data: serde_json::from_str(&data_str)?,
                    status: serde_json::from_str(&format!("\"{}\"", status_str))
                        .unwrap_or(TaskStatus::Pending),
                    priority: row.get("priority"),
                    scheduled_at: row.get("scheduled_at"),
                    executed_at: row.get("executed_at"),
                    retry_count: row.get("retry_count"),
                    max_retries: row.get("max_retries"),
                    error: row.get("error"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_pending_tasks(&self, agent_id: UUID) -> Result<Vec<Task>> {
        let rows = sqlx::query(
            r#"
            SELECT id, agent_id, task_type, data, status, priority, scheduled_at, executed_at, retry_count, max_retries, error, created_at, updated_at 
            FROM tasks 
            WHERE agent_id = ? AND status = 'PENDING'
            ORDER BY priority DESC, scheduled_at ASC
            "#,
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut tasks = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let agent_id_str: String = row.get("agent_id");
            let data_str: String = row.get("data");
            let status_str: String = row.get("status");

            tasks.push(Task {
                id: uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                agent_id: uuid::Uuid::parse_str(&agent_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                task_type: row.get("task_type"),
                data: serde_json::from_str(&data_str)?,
                status: serde_json::from_str(&format!("\"{}\"", status_str))
                    .unwrap_or(TaskStatus::Pending),
                priority: row.get("priority"),
                scheduled_at: row.get("scheduled_at"),
                executed_at: row.get("executed_at"),
                retry_count: row.get("retry_count"),
                max_retries: row.get("max_retries"),
                error: row.get("error"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(tasks)
    }

    async fn log(&self, log: &Log) -> Result<()> {
        let id = log.id.unwrap_or_else(uuid::Uuid::new_v4);

        sqlx::query(
            r#"
            INSERT INTO logs (id, entity_id, room_id, body, type, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(log.entity_id.to_string())
        .bind(log.room_id.map(|id| id.to_string()))
        .bind(serde_json::to_string(&log.body)?)
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
        let mut bindings: Vec<String> = Vec::new();

        if let Some(entity_id) = params.entity_id {
            query.push_str(" AND entity_id = ?");
            bindings.push(entity_id.to_string());
        }
        if let Some(room_id) = params.room_id {
            query.push_str(" AND room_id = ?");
            bindings.push(room_id.to_string());
        }
        if let Some(log_type) = params.log_type {
            query.push_str(" AND type = ?");
            bindings.push(log_type);
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = params.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = params.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        let mut query_builder = sqlx::query(&query);
        for binding in &bindings {
            query_builder = query_builder.bind(binding);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut logs = Vec::new();
        for row in rows {
            let id_str: String = row.get("id");
            let entity_id_str: String = row.get("entity_id");
            let room_id_str: Option<String> = row.get("room_id");
            let body_str: String = row.get("body");

            logs.push(Log {
                id: Some(
                    uuid::Uuid::parse_str(&id_str)
                        .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                ),
                entity_id: uuid::Uuid::parse_str(&entity_id_str)
                    .map_err(|e| ZoeyError::database(format!("Invalid UUID: {}", e)))?,
                room_id: room_id_str
                    .map(|s| uuid::Uuid::parse_str(&s).ok())
                    .flatten(),
                body: serde_json::from_str(&body_str)?,
                log_type: row.get("type"),
                created_at: row.get("created_at"),
            });
        }

        Ok(logs)
    }

    async fn get_agent_run_summaries(
        &self,
        _params: RunSummaryQuery,
    ) -> Result<AgentRunSummaryResult> {
        // Run summaries require more complex aggregation - return empty result for now
        Ok(AgentRunSummaryResult {
            runs: vec![],
            total: 0,
            has_more: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_adapter_creation() {
        // This is a compilation test
        assert!(true);
    }
}
