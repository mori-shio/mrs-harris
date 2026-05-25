use mrs_harris_common::models::run::{JobRun, RunStatus, TriggerType, NewRun};
use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::calendar::CalendarEntry;
use sqlx::{MySqlPool, Row};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::str::FromStr;

fn map_row_to_run(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<JobRun> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str)?;

    let job_id_str: String = row.try_get("job_id")?;
    let job_id = Uuid::parse_str(&job_id_str)?;

    let status_str: String = row.try_get("status")?;
    let status = RunStatus::from_str(&status_str)
        .map_err(|e| anyhow::anyhow!("Invalid RunStatus: {}", e))?;

    let worker_type_str: String = row.try_get("worker_type")?;
    let worker_type = WorkerType::from_str(&worker_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerType: {}", e))?;

    let worker_id: Option<String> = row.try_get("worker_id")?;

    let trigger_type_str: String = row.try_get("trigger_type")?;
    let trigger_type = TriggerType::from_str(&trigger_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid TriggerType: {}", e))?;

    let attempt: u32 = row.try_get("attempt")?;
    
    let scheduled_at: Option<DateTime<Utc>> = row.try_get("scheduled_at")?;
    let started_at: Option<DateTime<Utc>> = row.try_get("started_at")?;
    let finished_at: Option<DateTime<Utc>> = row.try_get("finished_at")?;
    let next_retry_at: Option<DateTime<Utc>> = row.try_get("next_retry_at")?;
    
    let duration_ms: Option<i64> = row.try_get("duration_ms")?;
    
    let output: Option<serde_json::Value> = row.try_get("output")?;
    let error: Option<String> = row.try_get("error")?;
    let version: u32 = row.try_get("version")?;

    let worker_definition_id_str: Option<String> = row.try_get("worker_definition_id")?;
    let worker_definition_id = worker_definition_id_str
        .and_then(|s| Uuid::parse_str(&s).ok());

    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;

    Ok(JobRun {
        id,
        job_id,
        status,
        worker_type,
        worker_id,
        trigger_type,
        attempt,
        scheduled_at,
        started_at,
        finished_at,
        next_retry_at,
        duration_ms,
        output,
        error,
        version,
        worker_definition_id,
        created_at,
        updated_at,
    })
}

/// ジョブ実行を新規作成
pub async fn create_run(pool: &MySqlPool, new_run: &NewRun) -> anyhow::Result<JobRun> {
    let id = Uuid::new_v4();
    let status_str = RunStatus::Pending.to_string();
    let worker_type_str = new_run.worker_type.to_string();
    let trigger_type_str = new_run.trigger_type.to_string();
    let worker_def_id_str = new_run.worker_definition_id.map(|uid| uid.to_string());

    sqlx::query(
        r#"INSERT INTO job_runs (id, job_id, status, worker_type, trigger_type, attempt, scheduled_at, version, worker_definition_id)
           VALUES (?, ?, ?, ?, ?, 1, ?, 1, ?)"#
    )
    .bind(id.to_string())
    .bind(new_run.job_id.to_string())
    .bind(status_str)
    .bind(worker_type_str)
    .bind(trigger_type_str)
    .bind(new_run.scheduled_at)
    .bind(worker_def_id_str)
    .execute(pool)
    .await?;

    get_run(pool, &id).await?.ok_or_else(|| anyhow::anyhow!("Created run not found"))
}

/// 実行履歴を取得
pub async fn get_run(pool: &MySqlPool, id: &Uuid) -> anyhow::Result<Option<JobRun>> {
    let row = sqlx::query("SELECT * FROM job_runs WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_run(&r)?)),
        None => Ok(None),
    }
}

/// ジョブごとの実行番号（1-indexed）に対応する実行履歴を取得
pub async fn get_run_by_number(
    pool: &MySqlPool,
    job_id: &Uuid,
    run_number: i64,
) -> anyhow::Result<Option<JobRun>> {
    let row = sqlx::query(
        "SELECT * FROM job_runs WHERE job_id = ? ORDER BY created_at ASC LIMIT 1 OFFSET ?"
    )
    .bind(job_id.to_string())
    .bind(run_number - 1)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(Some(map_row_to_run(&r)?)),
        None => Ok(None),
    }
}

/// ジョブ実行一覧を取得（フィルタ対応）
pub async fn list_runs(
    pool: &MySqlPool,
    job_id: Option<&Uuid>,
    limit: Option<u32>,
    offset: Option<u32>,
    desc: bool,
) -> anyhow::Result<Vec<JobRun>> {
    let mut query_str = "SELECT * FROM job_runs WHERE 1=1".to_string();
    if job_id.is_some() {
        query_str.push_str(" AND job_id = ?");
    }
    if desc {
        query_str.push_str(" ORDER BY created_at DESC");
    } else {
        query_str.push_str(" ORDER BY created_at ASC");
    }

    if let Some(limit) = limit {
        query_str.push_str(&format!(" LIMIT {}", limit));
    }
    if let Some(offset) = offset {
        query_str.push_str(&format!(" OFFSET {}", offset));
    }

    let mut query = sqlx::query(&query_str);
    if let Some(id) = job_id {
        query = query.bind(id.to_string());
    }

    let rows = query.fetch_all(pool).await?;
    let mut runs = Vec::new();
    for r in rows {
        runs.push(map_row_to_run(&r)?);
    }
    Ok(runs)
}

/// 実行履歴のステータス更新（楽観ロックを考慮）
pub async fn update_run_status(
    pool: &MySqlPool,
    id: &Uuid,
    status: RunStatus,
    worker_id: Option<&str>,
    error: Option<&str>,
    output: Option<&serde_json::Value>,
    duration_ms: Option<i64>,
    expected_version: u32,
) -> anyhow::Result<()> {
    let status_str = status.to_string();
    let now = Utc::now();

    let mut query_str = "UPDATE job_runs SET status = ?, version = version + 1".to_string();
    if status == RunStatus::Running {
        query_str.push_str(", started_at = ?");
    } else if status.is_terminal() {
        query_str.push_str(", finished_at = ?, duration_ms = ?");
    }

    if worker_id.is_some() {
        query_str.push_str(", worker_id = ?");
    }
    if error.is_some() {
        query_str.push_str(", error = ?");
    }
    if output.is_some() {
        query_str.push_str(", output = ?");
    }

    query_str.push_str(" WHERE id = ? AND version = ?");

    let mut query = sqlx::query(&query_str).bind(status_str);

    if status == RunStatus::Running {
        query = query.bind(now);
    } else if status.is_terminal() {
        query = query.bind(now).bind(duration_ms);
    }

    if let Some(w_id) = worker_id {
        query = query.bind(w_id);
    }
    if let Some(err) = error {
        query = query.bind(err);
    }
    if let Some(out) = output {
        query = query.bind(out);
    }

    let result = query
        .bind(id.to_string())
        .bind(expected_version)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Optimistic locking failed or run not found"));
    }

    Ok(())
}

/// カレンダー用の実行一覧を取得
pub async fn get_runs_for_calendar(
    pool: &MySqlPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<Vec<CalendarEntry>> {
    let rows = sqlx::query(
        r#"SELECT r.id as run_id, r.job_id, j.name as job_name, r.status, r.scheduled_at, r.started_at, r.finished_at, r.duration_ms
           FROM job_runs r
           JOIN jobs j ON r.job_id = j.id
           WHERE r.scheduled_at >= ? AND r.scheduled_at <= ?
           OR r.started_at >= ? AND r.started_at <= ?"#
    )
    .bind(start)
    .bind(end)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    let mut entries = Vec::new();
    for r in rows {
        let run_id_str: String = r.try_get("run_id")?;
        let run_id = Uuid::parse_str(&run_id_str)?;

        let job_id_str: String = r.try_get("job_id")?;
        let job_id = Uuid::parse_str(&job_id_str)?;

        let job_name: String = r.try_get("job_name")?;
        
        let status_str: String = r.try_get("status")?;
        let status = RunStatus::from_str(&status_str)
            .map_err(|e| anyhow::anyhow!("Invalid RunStatus: {}", e))?;

        let scheduled_at: Option<DateTime<Utc>> = r.try_get("scheduled_at")?;
        let started_at: Option<DateTime<Utc>> = r.try_get("started_at")?;
        let finished_at: Option<DateTime<Utc>> = r.try_get("finished_at")?;
        let duration_ms: Option<i64> = r.try_get("duration_ms")?;

        entries.push(CalendarEntry {
            run_id,
            job_id,
            job_name,
            status,
            scheduled_at,
            started_at,
            finished_at,
            duration_ms,
        });
    }

    Ok(entries)
}

/// 実行可能かつ pending 状態のジョブをアトミックに取得して queued にする
pub async fn claim_pending_run(pool: &MySqlPool) -> anyhow::Result<Option<JobRun>> {
    let mut tx = pool.begin().await?;

    // 予定時刻を過ぎている or 定期実行予定の pending ジョブを1件取得して排他ロック
    let row = sqlx::query(
        r#"SELECT * FROM job_runs 
           WHERE status = 'pending' AND (scheduled_at IS NULL OR scheduled_at <= ?)
           LIMIT 1 FOR UPDATE"#
    )
    .bind(Utc::now())
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(r) = row {
        let run = map_row_to_run(&r)?;
        
        // ステータスを queued に更新
        sqlx::query("UPDATE job_runs SET status = 'queued', version = version + 1 WHERE id = ?")
            .bind(run.id.to_string())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        
        // 更新された状態の JobRun を取得し直して返す
        let updated = get_run(pool, &run.id).await?.ok_or_else(|| anyhow::anyhow!("Run not found"))?;
        Ok(Some(updated))
    } else {
        tx.commit().await?;
        Ok(None)
    }
}

/// リトライ予定を設定する
pub async fn schedule_retry(
    pool: &MySqlPool,
    id: &Uuid,
    next_retry_at: DateTime<Utc>,
    attempt: u32,
    error: Option<&str>,
    expected_version: u32,
) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"UPDATE job_runs 
           SET status = 'retrying', attempt = ?, next_retry_at = ?, error = ?, version = version + 1
           WHERE id = ? AND version = ?"#
    )
    .bind(attempt)
    .bind(next_retry_at)
    .bind(error)
    .bind(id.to_string())
    .bind(expected_version)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Optimistic locking failed or run not found"));
    }

    Ok(())
}

/// デッドレター（リトライ上限超え）に移行する
pub async fn move_to_dead_letter(
    pool: &MySqlPool,
    id: &Uuid,
    error: Option<&str>,
    expected_version: u32,
) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"UPDATE job_runs 
           SET status = 'dead_letter', error = ?, version = version + 1
           WHERE id = ? AND version = ?"#
    )
    .bind(error)
    .bind(id.to_string())
    .bind(expected_version)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Optimistic locking failed or run not found"));
    }

    Ok(())
}
