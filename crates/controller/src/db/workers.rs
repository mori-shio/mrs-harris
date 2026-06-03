use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::worker::{Worker, WorkerStatus};
use sqlx::{MySqlPool, Row};

use chrono::{DateTime, Utc};
use std::str::FromStr;

fn map_row_to_worker(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<Worker> {
    let id: i64 = row.try_get("id")?;
    let worker_definition_id: i64 = row.try_get("worker_definition_id")?;

    let worker_type_str: String = row.try_get("worker_type")?;
    let worker_type = WorkerType::from_str(&worker_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerType: {}", e))?;

    let external_id: Option<String> = row.try_get("external_id")?;

    let status_str: String = row.try_get("status")?;
    let status = WorkerStatus::from_str(&status_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerStatus: {}", e))?;

    let job_run_id: i64 = row.try_get("job_run_id")?;

    let started_at: DateTime<Utc> = row.try_get("started_at")?;
    let last_heartbeat: Option<DateTime<Utc>> = row.try_get("last_heartbeat")?;
    let metadata: serde_json::Value = row.try_get("metadata")?;

    Ok(Worker {
        id,
        worker_definition_id,
        worker_type,
        external_id,
        status,
        job_run_id,
        started_at,
        last_heartbeat,
        metadata,
    })
}

/// ワーカーを登録
pub async fn register_worker(
    pool: &MySqlPool,
    worker_definition_id: &i64,
    worker_type: WorkerType,
    external_id: Option<&str>,
    job_run_id: &i64,
    metadata: &serde_json::Value,
) -> anyhow::Result<Worker> {
    let worker_type_str = worker_type.to_string();
    let status_str = WorkerStatus::Running.to_string();
    let now = Utc::now();

    let result = sqlx::query(
        r#"INSERT INTO workers (worker_definition_id, worker_type, external_id, status, job_run_id, started_at, last_heartbeat, metadata)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#
    )
    .bind(worker_definition_id)
    .bind(worker_type_str)
    .bind(external_id)
    .bind(status_str)
    .bind(job_run_id)
    .bind(now)
    .bind(now)
    .bind(metadata)
    .execute(pool)
    .await?;

    let id = result.last_insert_id() as i64;
    get_worker(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Registered worker not found"))
}

/// ワーカーを取得
pub async fn get_worker(pool: &MySqlPool, id: &i64) -> anyhow::Result<Option<Worker>> {
    let row = sqlx::query("SELECT * FROM workers WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_worker(&r)?)),
        None => Ok(None),
    }
}

/// ワーカーの外部ID（ECS ARN等）を更新
pub async fn update_worker_external_id(
    pool: &MySqlPool,
    id: &i64,
    external_id: &str,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE workers SET external_id = ? WHERE id = ?")
        .bind(external_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ワーカーのステータスを更新
pub async fn update_worker_status(
    pool: &MySqlPool,
    id: &i64,
    status: WorkerStatus,
) -> anyhow::Result<()> {
    let status_str = status.to_string();
    sqlx::query("UPDATE workers SET status = ? WHERE id = ?")
        .bind(status_str)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ハートビートを記録
#[allow(dead_code)]
pub async fn heartbeat(pool: &MySqlPool, id: &i64) -> anyhow::Result<()> {
    let now = Utc::now();
    sqlx::query("UPDATE workers SET last_heartbeat = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// アクティブなワーカー一覧を取得
pub async fn list_active_workers(pool: &MySqlPool) -> anyhow::Result<Vec<Worker>> {
    let rows =
        sqlx::query("SELECT * FROM workers WHERE status = 'running' ORDER BY started_at DESC")
            .fetch_all(pool)
            .await?;

    let mut workers = Vec::new();
    for r in rows {
        workers.push(map_row_to_worker(&r)?);
    }
    Ok(workers)
}

// ==========================================
// Worker Definition (自作ワーカー定義) 操作
// ==========================================

use mrs_harris_common::models::worker::{
    NewWorkerDefinition, WorkerDefinition, WorkerDefinitionHistoryEntry, WorkerDefinitionUpdate,
};

fn map_row_to_worker_def(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<WorkerDefinition> {
    let id: i64 = row.try_get("id")?;

    let name: String = row.try_get("name")?;
    let description: Option<String> = row.try_get("description")?;

    let worker_type_str: String = row.try_get("worker_type")?;
    let worker_type = WorkerType::from_str(&worker_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerType: {}", e))?;

    let config: serde_json::Value = row.try_get("config")?;

    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;

    Ok(WorkerDefinition {
        id,
        name,
        description,
        worker_type,
        config,
        created_at,
        updated_at,
    })
}

fn map_row_to_worker_def_history(
    row: &sqlx::mysql::MySqlRow,
) -> anyhow::Result<WorkerDefinitionHistoryEntry> {
    Ok(WorkerDefinitionHistoryEntry {
        id: row.try_get("id")?,
        worker_definition_id: row.try_get("worker_definition_id")?,
        version: row.try_get("version")?,
        payload: row.try_get("payload")?,
        changed_by: row.try_get("changed_by")?,
        changed_at: row.try_get("changed_at")?,
    })
}

/// 全てのワーカー定義を取得
pub async fn list_worker_definitions(pool: &MySqlPool) -> anyhow::Result<Vec<WorkerDefinition>> {
    let rows = sqlx::query("SELECT * FROM worker_definitions ORDER BY name ASC")
        .fetch_all(pool)
        .await?;

    let mut defs = Vec::new();
    for r in rows {
        defs.push(map_row_to_worker_def(&r)?);
    }
    Ok(defs)
}

/// 有効なワーカー定義一覧を取得
pub async fn list_active_worker_definitions(
    pool: &MySqlPool,
) -> anyhow::Result<Vec<WorkerDefinition>> {
    list_worker_definitions(pool).await
}

/// 単一のワーカー定義を取得
pub async fn get_worker_definition(
    pool: &MySqlPool,
    id: &i64,
) -> anyhow::Result<Option<WorkerDefinition>> {
    let row = sqlx::query("SELECT * FROM worker_definitions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_worker_def(&r)?)),
        None => Ok(None),
    }
}

/// ワーカー定義名で単一のワーカー定義を取得
pub async fn get_worker_definition_by_name(
    pool: &MySqlPool,
    name: &str,
) -> anyhow::Result<Option<WorkerDefinition>> {
    let row = sqlx::query("SELECT * FROM worker_definitions WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_worker_def(&r)?)),
        None => Ok(None),
    }
}

/// ワーカー定義を新規作成
pub async fn create_worker_definition(
    pool: &MySqlPool,
    new_def: &NewWorkerDefinition,
) -> anyhow::Result<WorkerDefinition> {
    let result = sqlx::query(
        r#"INSERT INTO worker_definitions (name, description, worker_type, config, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)"#
    )
    .bind(&new_def.name)
    .bind(&new_def.description)
    .bind(new_def.worker_type.to_string())
    .bind(&new_def.config)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(pool)
    .await?;

    let id = result.last_insert_id() as i64;
    get_worker_definition(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Created worker definition not found"))
}

/// ワーカー定義を更新
pub async fn update_worker_definition(
    pool: &MySqlPool,
    id: &i64,
    update: &WorkerDefinitionUpdate,
) -> anyhow::Result<WorkerDefinition> {
    let mut tx = pool.begin().await?;

    if let Some(ref desc) = update.description {
        sqlx::query("UPDATE worker_definitions SET description = ? WHERE id = ?")
            .bind(desc)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    if let Some(ref wt) = update.worker_type {
        sqlx::query("UPDATE worker_definitions SET worker_type = ? WHERE id = ?")
            .bind(wt.to_string())
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    if let Some(ref cfg) = update.config {
        sqlx::query("UPDATE worker_definitions SET config = ? WHERE id = ?")
            .bind(cfg)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("UPDATE worker_definitions SET updated_at = ? WHERE id = ?")
        .bind(Utc::now())
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    get_worker_definition(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Updated worker definition not found"))
}

/// ワーカー定義を削除
pub async fn delete_worker_definition(pool: &MySqlPool, id: &i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM worker_definitions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_worker_definition_history(
    pool: &MySqlPool,
    worker_definition_id: &i64,
) -> anyhow::Result<Vec<WorkerDefinitionHistoryEntry>> {
    let rows = sqlx::query(
        "SELECT id, worker_definition_id, version, payload, changed_by, changed_at
         FROM worker_definition_history
         WHERE worker_definition_id = ?
         ORDER BY version DESC",
    )
    .bind(worker_definition_id)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::new();
    for row in rows {
        items.push(map_row_to_worker_def_history(&row)?);
    }
    Ok(items)
}
