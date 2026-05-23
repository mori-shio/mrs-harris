use mrs_harris_common::models::worker::{WorkerInfo, WorkerStatus};
use mrs_harris_common::models::job::WorkerType;
use sqlx::{MySqlPool, Row};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::str::FromStr;

fn map_row_to_worker(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<WorkerInfo> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str)?;

    let worker_type_str: String = row.try_get("worker_type")?;
    let worker_type = WorkerType::from_str(&worker_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerType: {}", e))?;

    let external_id: String = row.try_get("external_id")?;

    let status_str: String = row.try_get("status")?;
    let status = WorkerStatus::from_str(&status_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerStatus: {}", e))?;

    let run_id_str: String = row.try_get("run_id")?;
    let run_id = Uuid::parse_str(&run_id_str)?;

    let started_at: DateTime<Utc> = row.try_get("started_at")?;
    let last_heartbeat: Option<DateTime<Utc>> = row.try_get("last_heartbeat")?;
    let metadata: serde_json::Value = row.try_get("metadata")?;

    Ok(WorkerInfo {
        id,
        worker_type,
        external_id,
        status,
        run_id,
        started_at,
        last_heartbeat,
        metadata,
    })
}

/// ワーカーを登録
pub async fn register_worker(
    pool: &MySqlPool,
    id: &Uuid,
    worker_type: WorkerType,
    external_id: &str,
    run_id: &Uuid,
    metadata: &serde_json::Value,
) -> anyhow::Result<WorkerInfo> {
    let worker_type_str = worker_type.to_string();
    let status_str = WorkerStatus::Running.to_string();
    let now = Utc::now();

    sqlx::query(
        r#"INSERT INTO worker_tracking (id, worker_type, external_id, status, run_id, started_at, last_heartbeat, metadata)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#
    )
    .bind(id.to_string())
    .bind(worker_type_str)
    .bind(external_id)
    .bind(status_str)
    .bind(run_id.to_string())
    .bind(now)
    .bind(now)
    .bind(metadata)
    .execute(pool)
    .await?;

    get_worker(pool, id).await?.ok_or_else(|| anyhow::anyhow!("Registered worker not found"))
}

/// ワーカーを取得
pub async fn get_worker(pool: &MySqlPool, id: &Uuid) -> anyhow::Result<Option<WorkerInfo>> {
    let row = sqlx::query("SELECT * FROM worker_tracking WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_worker(&r)?)),
        None => Ok(None),
    }
}

/// ワーカーのステータスを更新
pub async fn update_worker_status(
    pool: &MySqlPool,
    id: &Uuid,
    status: WorkerStatus,
) -> anyhow::Result<()> {
    let status_str = status.to_string();
    sqlx::query("UPDATE worker_tracking SET status = ? WHERE id = ?")
        .bind(status_str)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// ハートビートを記録
pub async fn heartbeat(pool: &MySqlPool, id: &Uuid) -> anyhow::Result<()> {
    let now = Utc::now();
    sqlx::query("UPDATE worker_tracking SET last_heartbeat = ? WHERE id = ?")
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// アクティブなワーカー一覧を取得
pub async fn list_active_workers(pool: &MySqlPool) -> anyhow::Result<Vec<WorkerInfo>> {
    let rows = sqlx::query("SELECT * FROM worker_tracking WHERE status = 'running' ORDER BY started_at DESC")
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

use mrs_harris_common::models::worker::{WorkerDefinition, NewWorkerDefinition, WorkerDefinitionUpdate};

fn map_row_to_worker_def(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<WorkerDefinition> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str)?;

    let name: String = row.try_get("name")?;
    let description: Option<String> = row.try_get("description")?;

    let worker_type_str: String = row.try_get("worker_type")?;
    let worker_type = WorkerType::from_str(&worker_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerType: {}", e))?;

    let config: serde_json::Value = row.try_get("config")?;
    let is_active_val: i8 = row.try_get("is_active")?;
    let is_active = is_active_val != 0;

    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;

    Ok(WorkerDefinition {
        id,
        name,
        description,
        worker_type,
        config,
        is_active,
        created_at,
        updated_at,
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
pub async fn list_active_worker_definitions(pool: &MySqlPool) -> anyhow::Result<Vec<WorkerDefinition>> {
    let rows = sqlx::query("SELECT * FROM worker_definitions WHERE is_active = 1 ORDER BY name ASC")
        .fetch_all(pool)
        .await?;

    let mut defs = Vec::new();
    for r in rows {
        defs.push(map_row_to_worker_def(&r)?);
    }
    Ok(defs)
}

/// 単一のワーカー定義を取得
pub async fn get_worker_definition(pool: &MySqlPool, id: &Uuid) -> anyhow::Result<Option<WorkerDefinition>> {
    let row = sqlx::query("SELECT * FROM worker_definitions WHERE id = ?")
        .bind(id.to_string())
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
    let id = Uuid::new_v4();
    let is_active_val: i8 = if new_def.is_active { 1 } else { 0 };

    sqlx::query(
        r#"INSERT INTO worker_definitions (id, name, description, worker_type, config, is_active, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#
    )
    .bind(id.to_string())
    .bind(&new_def.name)
    .bind(&new_def.description)
    .bind(new_def.worker_type.to_string())
    .bind(&new_def.config)
    .bind(is_active_val)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(pool)
    .await?;

    get_worker_definition(pool, &id).await?.ok_or_else(|| anyhow::anyhow!("Created worker definition not found"))
}

/// ワーカー定義を更新
pub async fn update_worker_definition(
    pool: &MySqlPool,
    id: &Uuid,
    update: &WorkerDefinitionUpdate,
) -> anyhow::Result<WorkerDefinition> {
    let mut tx = pool.begin().await?;

    if let Some(ref desc) = update.description {
        sqlx::query("UPDATE worker_definitions SET description = ? WHERE id = ?")
            .bind(desc)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
    }

    if let Some(ref wt) = update.worker_type {
        sqlx::query("UPDATE worker_definitions SET worker_type = ? WHERE id = ?")
            .bind(wt.to_string())
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
    }

    if let Some(ref cfg) = update.config {
        sqlx::query("UPDATE worker_definitions SET config = ? WHERE id = ?")
            .bind(cfg)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
    }

    if let Some(active) = update.is_active {
        let is_active_val: i8 = if active { 1 } else { 0 };
        sqlx::query("UPDATE worker_definitions SET is_active = ? WHERE id = ?")
            .bind(is_active_val)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("UPDATE worker_definitions SET updated_at = ? WHERE id = ?")
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    get_worker_definition(pool, id).await?.ok_or_else(|| anyhow::anyhow!("Updated worker definition not found"))
}

/// ワーカー定義を削除
pub async fn delete_worker_definition(pool: &MySqlPool, id: &Uuid) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM worker_definitions WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

