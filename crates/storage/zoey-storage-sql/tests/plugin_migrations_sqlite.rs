use zoey_core::types::{IDatabaseAdapter, MigrationOptions, PluginMigration};
use zoey_storage_sql::SqliteAdapter;
use uuid::Uuid as UUID;

#[tokio::test]
async fn test_workflow_plugin_schema_sqlite() {
    let mut adapter = SqliteAdapter::new("sqlite::memory:").await.unwrap();
    adapter.initialize(None).await.unwrap();

    let workflow_schema = serde_json::json!({
        "workflows": {
            "type": "table",
            "columns": {
                "id": "UUID PRIMARY KEY",
                "name": "TEXT NOT NULL",
                "config": "JSONB",
                "status": "TEXT DEFAULT 'created'",
                "created_at": "TIMESTAMP",
                "updated_at": "TIMESTAMP"
            }
        },
        "workflow_tasks": {
            "type": "table",
            "columns": {
                "id": "UUID PRIMARY KEY",
                "workflow_id": "UUID REFERENCES workflows(id)",
                "name": "TEXT NOT NULL",
                "config": "JSONB",
                "status": "TEXT DEFAULT 'pending'",
                "result": "JSONB",
                "created_at": "TIMESTAMP"
            }
        }
    });

    adapter
        .run_plugin_migrations(
            vec![PluginMigration {
                name: "workflow".to_string(),
                schema: Some(workflow_schema),
            }],
            MigrationOptions {
                verbose: false,
                force: false,
                dry_run: false,
            },
        )
        .await
        .unwrap();

    // Verify tables exist by attempting inserts
    let wf_id = UUID::new_v4().to_string();
    let task_id = UUID::new_v4().to_string();

    let pool_any = adapter.get_connection().await.unwrap();
    let pool = pool_any.downcast_ref::<sqlx::SqlitePool>().unwrap();

    // Insert workflow
    sqlx::query("INSERT INTO workflows (id, name, config, status, created_at) VALUES (?, ?, ?, 'created', strftime('%s','now'))")
        .bind(&wf_id)
        .bind("wf1")
        .bind("{}")
        .execute(pool)
        .await
        .unwrap();

    // Insert task referencing workflow
    sqlx::query("INSERT INTO workflow_tasks (id, workflow_id, name, config, status, created_at) VALUES (?, ?, ?, ?, 'pending', strftime('%s','now'))")
        .bind(&task_id)
        .bind(&wf_id)
        .bind("task1")
        .bind("{}")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_ml_plugin_schema_sqlite() {
    let mut adapter = SqliteAdapter::new("sqlite::memory:").await.unwrap();
    adapter.initialize(None).await.unwrap();

    let ml_schema = serde_json::json!({
        "ml_models": {
            "type": "table",
            "columns": {
                "id": "UUID PRIMARY KEY",
                "name": "TEXT UNIQUE NOT NULL",
                "model_type": "TEXT NOT NULL",
                "config": "JSONB",
                "state": "TEXT",
                "created_at": "TIMESTAMP",
                "updated_at": "TIMESTAMP"
            }
        },
        "ml_inference_logs": {
            "type": "table",
            "columns": {
                "id": "UUID PRIMARY KEY",
                "model_id": "UUID REFERENCES ml_models(id)",
                "input_hash": "TEXT",
                "output_hash": "TEXT",
                "latency_ms": "FLOAT",
                "success": "BOOLEAN",
                "created_at": "TIMESTAMP"
            }
        }
    });

    adapter
        .run_plugin_migrations(
            vec![PluginMigration {
                name: "ml".to_string(),
                schema: Some(ml_schema),
            }],
            MigrationOptions {
                verbose: false,
                force: false,
                dry_run: false,
            },
        )
        .await
        .unwrap();

    let pool_any = adapter.get_connection().await.unwrap();
    let pool = pool_any.downcast_ref::<sqlx::SqlitePool>().unwrap();

    // Insert model and log referencing it
    let model_id = UUID::new_v4().to_string();
    sqlx::query("INSERT INTO ml_models (id, name, model_type, config, state, created_at) VALUES (?, 'm1', 'transformer', '{}', 'registered', strftime('%s','now'))")
        .bind(&model_id)
        .execute(pool)
        .await
        .unwrap();

    let log_id = UUID::new_v4().to_string();
    sqlx::query("INSERT INTO ml_inference_logs (id, model_id, input_hash, output_hash, latency_ms, success, created_at) VALUES (?, ?, 'ih', 'oh', 12.3, 1, strftime('%s','now'))")
        .bind(&log_id)
        .bind(&model_id)
        .execute(pool)
        .await
        .unwrap();
}
