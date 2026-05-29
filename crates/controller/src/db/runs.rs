use mrs_harris_common::models::calendar::CalendarEntry;
use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::run::{
    JobRun, LogArchiveStatus, LogArchiveStore, NewRun, RunStatus, TriggerType,
};
use sqlx::{MySqlPool, Row};

use chrono::{DateTime, Utc};
use std::str::FromStr;

fn require_job_history_id(job_id: i64, job_history_id: Option<i64>) -> anyhow::Result<i64> {
    job_history_id.ok_or_else(|| anyhow::anyhow!("job {} has no job_history record", job_id))
}

async fn infer_terminal_duration_ms(
    pool: &MySqlPool,
    id: &i64,
    finished_at: DateTime<Utc>,
    duration_ms: Option<i64>,
) -> anyhow::Result<Option<i64>> {
    if duration_ms.is_some() {
        return Ok(duration_ms);
    }

    let started_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT started_at FROM job_runs WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;

    Ok(started_at.map(|started| (finished_at - started).num_milliseconds().max(0)))
}

fn map_row_to_run(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<JobRun> {
    let id: i64 = row.try_get("id")?;

    let job_id: i64 = row.try_get("job_id")?;
    let run_number: i64 = row.try_get("run_number")?;

    let status_str: String = row.try_get("status")?;
    let status = RunStatus::from_str(&status_str)
        .map_err(|e| anyhow::anyhow!("Invalid RunStatus: {}", e))?;

    let worker_type_str: String = row.try_get("worker_type")?;
    let worker_type = WorkerType::from_str(&worker_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid WorkerType: {}", e))?;

    let worker_id: Option<i64> = row.try_get("worker_id")?;

    let trigger_type_str: String = row.try_get("trigger_type")?;
    let trigger_type = TriggerType::from_str(&trigger_type_str)
        .map_err(|e| anyhow::anyhow!("Invalid TriggerType: {}", e))?;

    let attempt: u32 = row.try_get("attempt")?;

    let scheduled_at: Option<DateTime<Utc>> = row.try_get("scheduled_at")?;
    let started_at: Option<DateTime<Utc>> = row.try_get("started_at")?;
    let finished_at: Option<DateTime<Utc>> = row.try_get("finished_at")?;
    let next_retry_at: Option<DateTime<Utc>> = row.try_get("next_retry_at")?;

    let duration_ms: Option<i64> = row.try_get("duration_ms")?;
    let log_archive_status = row
        .try_get::<Option<String>, _>("log_archive_status")?
        .map(|status| LogArchiveStatus::from_str(&status))
        .transpose()
        .map_err(|e| anyhow::anyhow!("Invalid LogArchiveStatus: {}", e))?;
    let log_archive_store = row
        .try_get::<Option<String>, _>("log_archive_store")?
        .map(|store| LogArchiveStore::from_str(&store))
        .transpose()
        .map_err(|e| anyhow::anyhow!("Invalid LogArchiveStore: {}", e))?;
    let log_archive_key: Option<String> = row.try_get("log_archive_key")?;
    let log_line_count: Option<i64> = row.try_get("log_line_count")?;
    let log_archive_bytes: Option<i64> = row.try_get("log_archive_bytes")?;
    let log_archived_at: Option<DateTime<Utc>> = row.try_get("log_archived_at")?;

    let output: Option<serde_json::Value> = row.try_get("output")?;
    let error: Option<String> = row.try_get("error")?;
    let job_history_id: Option<i64> = row.try_get("job_history_id")?;
    let worker_definition_id: Option<i64> = row.try_get("worker_definition_id")?;
    let config_version: Option<u32> = row.try_get("config_version").ok().flatten();
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;

    Ok(JobRun {
        id,
        job_id,
        run_number,
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
        log_archive_status,
        log_archive_store,
        log_archive_key,
        log_line_count,
        log_archive_bytes,
        log_archived_at,
        output,
        error,
        job_history_id,
        worker_definition_id,
        config_version,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_job_history_id_rejects_missing_history() {
        let err = require_job_history_id(123, None).unwrap_err();

        assert!(
            err.to_string()
                .contains("job 123 has no job_history record")
        );
    }
}

/// ジョブ実行を新規作成
pub async fn create_run(pool: &MySqlPool, new_run: &NewRun) -> anyhow::Result<JobRun> {
    let mut tx = pool.begin().await?;

    let status_str = RunStatus::Pending.to_string();
    let trigger_type_str = new_run.trigger_type.to_string();

    // クエリで現在のジョブ設定バージョン履歴の最新の id を取得する
    let history_row = sqlx::query("SELECT MAX(id) as max_h_id FROM job_history WHERE job_id = ?")
        .bind(new_run.job_id)
        .fetch_optional(&mut *tx)
        .await?;

    let job_history_id = match history_row {
        Some(row) => row.try_get::<Option<i64>, &str>("max_h_id").unwrap_or(None),
        None => None,
    };
    let job_history_id = require_job_history_id(new_run.job_id, job_history_id)?;

    // 最新の run_number を FOR UPDATE で取得
    let max_run_number: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(run_number), 0) FROM job_runs WHERE job_id = ? FOR UPDATE",
    )
    .bind(new_run.job_id)
    .fetch_one(&mut *tx)
    .await?;

    let new_run_number = max_run_number + 1;

    let result = sqlx::query(
        r#"INSERT INTO job_runs (job_id, status, trigger_type, attempt, scheduled_at, job_history_id, run_number)
           VALUES (?, ?, ?, 1, ?, ?, ?)"#
    )
    .bind(new_run.job_id)
    .bind(status_str)
    .bind(trigger_type_str)
    .bind(new_run.scheduled_at)
    .bind(job_history_id)
    .bind(new_run_number)
    .execute(&mut *tx)
    .await?;

    let new_id = result.last_insert_id() as i64;
    tx.commit().await?;

    get_run(pool, &new_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Created run not found"))
}

/// 実行履歴を取得
pub async fn get_run(pool: &MySqlPool, id: &i64) -> anyhow::Result<Option<JobRun>> {
    let row = sqlx::query(
        "SELECT r.*, h.version as config_version, \
                COALESCE(w.worker_definition_id, j.worker_definition_id) as worker_definition_id, \
                COALESCE(wd.worker_type, jd.worker_type) as worker_type \
         FROM job_runs r \
         LEFT JOIN job_history h ON r.job_history_id = h.id \
         LEFT JOIN jobs j ON r.job_id = j.id \
         LEFT JOIN workers w ON r.worker_id = w.id \
         LEFT JOIN worker_definitions wd ON w.worker_definition_id = wd.id \
         LEFT JOIN worker_definitions jd ON j.worker_definition_id = jd.id \
         WHERE r.id = ?",
    )
    .bind(id)
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
    job_id: &i64,
    run_number: i64,
) -> anyhow::Result<Option<JobRun>> {
    let row = sqlx::query(
        "SELECT r.*, h.version as config_version, \
                COALESCE(w.worker_definition_id, j.worker_definition_id) as worker_definition_id, \
                COALESCE(wd.worker_type, jd.worker_type) as worker_type \
         FROM job_runs r \
         LEFT JOIN job_history h ON r.job_history_id = h.id \
         LEFT JOIN jobs j ON r.job_id = j.id \
         LEFT JOIN workers w ON r.worker_id = w.id \
         LEFT JOIN worker_definitions wd ON w.worker_definition_id = wd.id \
         LEFT JOIN worker_definitions jd ON j.worker_definition_id = jd.id \
         WHERE r.job_id = ? AND r.run_number = ?",
    )
    .bind(job_id)
    .bind(run_number)
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
    job_id: Option<&i64>,
    limit: Option<u32>,
    offset: Option<u32>,
    desc: bool,
) -> anyhow::Result<Vec<JobRun>> {
    let mut query_str = "SELECT r.*, h.version as config_version, \
                                COALESCE(w.worker_definition_id, j.worker_definition_id) as worker_definition_id, \
                                COALESCE(wd.worker_type, jd.worker_type) as worker_type \
                         FROM job_runs r \
                         LEFT JOIN job_history h ON r.job_history_id = h.id \
                         LEFT JOIN jobs j ON r.job_id = j.id \
                         LEFT JOIN workers w ON r.worker_id = w.id \
                         LEFT JOIN worker_definitions wd ON w.worker_definition_id = wd.id \
                         LEFT JOIN worker_definitions jd ON j.worker_definition_id = jd.id \
                         WHERE 1=1".to_string();
    if job_id.is_some() {
        query_str.push_str(" AND r.job_id = ?");
    }
    if desc {
        query_str.push_str(" ORDER BY r.created_at DESC");
    } else {
        query_str.push_str(" ORDER BY r.created_at ASC");
    }

    if let Some(limit) = limit {
        query_str.push_str(&format!(" LIMIT {}", limit));
    }
    if let Some(offset) = offset {
        query_str.push_str(&format!(" OFFSET {}", offset));
    }

    let mut query = sqlx::query(&query_str);
    if let Some(id) = job_id {
        query = query.bind(id);
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
    id: &i64,
    status: RunStatus,
    worker_id: Option<i64>,
    error: Option<&str>,
    output: Option<&serde_json::Value>,
    duration_ms: Option<i64>,
) -> anyhow::Result<()> {
    let status_str = status.to_string();
    let now = Utc::now();
    let terminal_duration_ms = if status.is_terminal() {
        infer_terminal_duration_ms(pool, id, now, duration_ms).await?
    } else {
        duration_ms
    };

    let mut query_str = "UPDATE job_runs SET status = ?".to_string();
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

    query_str.push_str(" WHERE id = ?");

    let mut query = sqlx::query(&query_str).bind(status_str);

    if status == RunStatus::Running {
        query = query.bind(now);
    } else if status.is_terminal() {
        query = query.bind(now).bind(terminal_duration_ms);
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

    let result = query.bind(id).execute(pool).await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Run not found or already in target status"));
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
        let run_id: i64 = r.try_get("run_id")?;
        let job_id: i64 = r.try_get("job_id")?;

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
        r#"SELECT r.*, 
                  COALESCE(w.worker_definition_id, j.worker_definition_id) as worker_definition_id,
                  COALESCE(wd.worker_type, jd.worker_type) as worker_type
           FROM job_runs r
           LEFT JOIN jobs j ON r.job_id = j.id
           LEFT JOIN workers w ON r.worker_id = w.id
           LEFT JOIN worker_definitions wd ON w.worker_definition_id = wd.id
           LEFT JOIN worker_definitions jd ON j.worker_definition_id = jd.id
           WHERE r.status = 'pending' AND (r.scheduled_at IS NULL OR r.scheduled_at <= ?)
           LIMIT 1 FOR UPDATE"#,
    )
    .bind(Utc::now())
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(r) = row {
        let run = map_row_to_run(&r)?;

        // ステータスを queued に更新
        sqlx::query("UPDATE job_runs SET status = 'queued' WHERE id = ?")
            .bind(run.id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        // 更新された状態の JobRun を取得し直して返す
        let updated = get_run(pool, &run.id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Run not found"))?;
        Ok(Some(updated))
    } else {
        tx.commit().await?;
        Ok(None)
    }
}

/// リトライ予定を設定する
pub async fn schedule_retry(
    pool: &MySqlPool,
    id: &i64,
    next_retry_at: DateTime<Utc>,
    attempt: u32,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"UPDATE job_runs 
           SET status = 'retrying', attempt = ?, next_retry_at = ?, error = ?
           WHERE id = ?"#,
    )
    .bind(attempt)
    .bind(next_retry_at)
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Run not found"));
    }

    Ok(())
}

/// デッドレター（リトライ上限超え）に移行する
pub async fn move_to_dead_letter(
    pool: &MySqlPool,
    id: &i64,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let finished_at = Utc::now();
    let duration_ms = infer_terminal_duration_ms(pool, id, finished_at, None).await?;
    let result = sqlx::query(
        r#"UPDATE job_runs 
           SET status = 'dead_letter', error = ?, finished_at = ?, duration_ms = ?
           WHERE id = ?"#,
    )
    .bind(error)
    .bind(finished_at)
    .bind(duration_ms)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Run not found"));
    }

    Ok(())
}
