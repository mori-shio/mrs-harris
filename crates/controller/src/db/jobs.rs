use mrs_harris_common::models::job::{
    Job, JobFilter, JobType, JobUpdate, NewJob, RetryPolicy, WorkerType,
};
use sqlx::{MySqlPool, Row};

use chrono::{DateTime, Utc};
use std::str::FromStr;

/// 行データから Job 構造体へマッピングするヘルパー
fn map_row_to_job(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<Job> {
    let id: i64 = row.try_get("id")?;

    let name: String = row.try_get("name")?;
    let description: Option<String> = row.try_get("description")?;

    let job_type_str: String = row.try_get("job_type")?;
    let job_type =
        JobType::from_str(&job_type_str).map_err(|e| anyhow::anyhow!("Invalid job_type: {}", e))?;

    let payload: serde_json::Value = row.try_get("payload")?;
    let schedule_expr: Option<String> = row.try_get("schedule_expr")?;

    let worker_type_str: String = row.try_get("worker_type")?;
    let worker_type = WorkerType::from_str(&worker_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid worker_type: {}", e))?;

    let retry_policy_val: serde_json::Value = row.try_get("retry_policy")?;
    let retry_policy: RetryPolicy = serde_json::from_value(retry_policy_val)?;

    let timeout_sec: u32 = row.try_get("timeout_sec")?;

    let is_active_val: i8 = row.try_get("is_active")?;
    let is_active = is_active_val != 0;

    let tags_val: serde_json::Value = row.try_get("tags")?;
    let tags: Vec<String> = serde_json::from_value(tags_val)?;

    let worker_definition_id: Option<i64> = row.try_get("worker_definition_id")?;
    let space_id: Option<i64> = row.try_get("space_id")?;

    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;

    Ok(Job {
        id,
        name,
        description,
        job_type,
        payload,
        schedule_expr,
        worker_type,
        retry_policy,
        timeout_sec,
        is_active,
        tags,
        worker_definition_id,
        space_id,
        created_at,
        updated_at,
    })
}

/// ジョブを新規作成
pub async fn create_job(pool: &MySqlPool, new_job: &NewJob) -> anyhow::Result<Job> {
    let job_type_str = new_job.job_type.to_string();
    let retry_policy_json = serde_json::to_value(&new_job.retry_policy)?;
    let tags_json = serde_json::to_value(&new_job.tags)?;
    let is_active_val: i8 = if new_job.is_active { 1 } else { 0 };

    let result = sqlx::query(
        r#"INSERT INTO jobs (name, description, job_type, payload, schedule_expr, retry_policy, timeout_sec, is_active, tags, worker_definition_id, space_id)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    )
    .bind(&new_job.name)
    .bind(&new_job.description)
    .bind(job_type_str)
    .bind(&new_job.payload)
    .bind(&new_job.schedule_expr)
    .bind(retry_policy_json)
    .bind(new_job.timeout_sec)
    .bind(is_active_val)
    .bind(tags_json)
    .bind(new_job.worker_definition_id)
    .bind(new_job.space_id)
    .execute(pool)
    .await?;

    let new_id = result.last_insert_id() as i64;
    get_job(pool, &new_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Created job not found"))
}

/// ジョブを取得
pub async fn get_job(pool: &MySqlPool, id: &i64) -> anyhow::Result<Option<Job>> {
    let row = sqlx::query(
        "SELECT j.*, wd.worker_type FROM jobs j \
         LEFT JOIN worker_definitions wd ON j.worker_definition_id = wd.id \
         WHERE j.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_job(&r)?)),
        None => Ok(None),
    }
}

/// ジョブを名前で取得
pub async fn get_job_by_name(pool: &MySqlPool, name: &str) -> anyhow::Result<Option<Job>> {
    let row = sqlx::query(
        "SELECT j.*, wd.worker_type FROM jobs j \
         LEFT JOIN worker_definitions wd ON j.worker_definition_id = wd.id \
         WHERE j.name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_job(&r)?)),
        None => Ok(None),
    }
}

/// ジョブ一覧を取得（フィルタ対応）
pub async fn list_jobs(pool: &MySqlPool, filter: &JobFilter) -> anyhow::Result<Vec<Job>> {
    let mut query_str = "SELECT j.*, wd.worker_type FROM jobs j \
                         LEFT JOIN worker_definitions wd ON j.worker_definition_id = wd.id \
                         WHERE 1=1"
        .to_string();

    if filter.job_type.is_some() {
        query_str.push_str(" AND j.job_type = ?");
    }
    if filter.is_active.is_some() {
        query_str.push_str(" AND j.is_active = ?");
    }
    if filter.search.is_some() {
        query_str.push_str(" AND (j.name LIKE ? OR j.description LIKE ?)");
    }
    if let Some(ref space_id) = filter.space_id {
        if space_id == "unclassified" {
            query_str.push_str(" AND j.space_id IS NULL");
        } else if !space_id.trim().is_empty() {
            query_str.push_str(" AND j.space_id = ?");
        }
    }

    query_str.push_str(" ORDER BY j.created_at DESC");

    if let Some(limit) = filter.limit {
        query_str.push_str(&format!(" LIMIT {}", limit));
    }
    if let Some(offset) = filter.offset {
        query_str.push_str(&format!(" OFFSET {}", offset));
    }

    let mut query = sqlx::query(&query_str);

    if let Some(ref job_type) = filter.job_type {
        query = query.bind(job_type.to_string());
    }
    if let Some(is_active) = filter.is_active {
        query = query.bind(if is_active { 1i8 } else { 0i8 });
    }
    if let Some(ref search) = filter.search {
        let search_pattern = format!("%{}%", search);
        query = query.bind(search_pattern.clone()).bind(search_pattern);
    }
    if let Some(ref space_id) = filter.space_id
        && space_id != "unclassified"
        && !space_id.trim().is_empty()
    {
        query = query.bind(space_id);
    }

    let rows = query.fetch_all(pool).await?;
    let mut jobs = Vec::new();
    for r in rows {
        jobs.push(map_row_to_job(&r)?);
    }

    // タグフィルタ（インメモリで処理。またはJSON_CONTAINSなどを使うことも可能）
    if let Some(ref tag) = filter.tag {
        jobs.retain(|j| j.tags.contains(tag));
    }

    Ok(jobs)
}

/// ジョブを更新
pub async fn update_job(pool: &MySqlPool, id: &i64, update: &JobUpdate) -> anyhow::Result<Job> {
    let mut query_str = "UPDATE jobs SET ".to_string();
    let mut sets = Vec::new();

    if update.description.is_some() {
        sets.push("description = ?");
    }
    if update.payload.is_some() {
        sets.push("payload = ?");
    }
    if update.schedule_expr.is_some() {
        sets.push("schedule_expr = ?");
    }
    if update.retry_policy.is_some() {
        sets.push("retry_policy = ?");
    }
    if update.timeout_sec.is_some() {
        sets.push("timeout_sec = ?");
    }
    if update.is_active.is_some() {
        sets.push("is_active = ?");
    }
    if update.tags.is_some() {
        sets.push("tags = ?");
    }
    if update.worker_definition_id.is_some() {
        sets.push("worker_definition_id = ?");
    }
    if update.space_id.is_some() {
        sets.push("space_id = ?");
    }

    if sets.is_empty() {
        return get_job(pool, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Job not found"));
    }

    query_str.push_str(&sets.join(", "));
    query_str.push_str(" WHERE id = ?");

    let mut query = sqlx::query(&query_str);

    if let Some(ref description) = update.description {
        query = query.bind(description);
    }
    if let Some(ref payload) = update.payload {
        query = query.bind(payload);
    }
    if let Some(ref schedule_expr) = update.schedule_expr {
        query = query.bind(schedule_expr);
    }
    if let Some(ref retry_policy) = update.retry_policy {
        query = query.bind(serde_json::to_value(retry_policy)?);
    }
    if let Some(timeout_sec) = update.timeout_sec {
        query = query.bind(timeout_sec);
    }
    if let Some(is_active) = update.is_active {
        query = query.bind(if is_active { 1i8 } else { 0i8 });
    }
    if let Some(ref tags) = update.tags {
        query = query.bind(serde_json::to_value(tags)?);
    }
    if let Some(ref worker_definition_id) = update.worker_definition_id {
        query = query.bind(worker_definition_id);
    }
    if let Some(ref space_id) = update.space_id {
        query = query.bind(space_id);
    }

    query.bind(id).execute(pool).await?;

    get_job(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Updated job not found"))
}

/// ジョブを削除
pub async fn delete_job(pool: &MySqlPool, id: &i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM jobs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
