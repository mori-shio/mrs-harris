use crate::app::AppState;
use mrs_harris_common::models::run::{LogLine, LogStream, RunStatus};

use chrono::Utc;

/// タイムアウトしたジョブを検出し、失敗としてマーク
pub async fn check_timeouts(state: &AppState) -> anyhow::Result<()> {
    let now = Utc::now();

    // 実行中のジョブ一覧をJOINして取得
    let rows = sqlx::query(
        r#"SELECT r.id as run_id, j.name as job_name, j.timeout_sec, r.status, r.started_at, r.created_at
           FROM job_runs r
           JOIN jobs j ON r.job_id = j.id
           WHERE r.status = 'running' OR r.status = 'queued'"#
    )
    .fetch_all(&state.db)
    .await?;

    for r in rows {
        let run_id: i64 = sqlx::Row::try_get(&r, "run_id")?;

        let job_name: String = sqlx::Row::try_get(&r, "job_name")?;
        let timeout_sec: u32 = sqlx::Row::try_get(&r, "timeout_sec")?;
        let status: String = sqlx::Row::try_get(&r, "status")?;
        let started_at: Option<chrono::DateTime<Utc>> = sqlx::Row::try_get(&r, "started_at")?;
        let created_at: chrono::DateTime<Utc> = sqlx::Row::try_get(&r, "created_at")?;

        let is_timeout = if status == "running" {
            if let Some(start) = started_at {
                let elapsed = now.signed_duration_since(start).num_seconds();
                elapsed > timeout_sec as i64
            } else {
                false
            }
        } else {
            // queued のまま一定時間（例：10分）起動しない場合もタイムアウトとみなす
            let elapsed = now.signed_duration_since(created_at).num_seconds();
            elapsed > 600 // 10 minutes
        };

        if is_timeout {
            tracing::warn!("Reaping timed out run {} for job '{}'", run_id, job_name);

            // 1. システムログにタイムアウトを記録
            let log_line = LogLine {
                id: None,
                run_id,
                task_name: None,
                stream: LogStream::System,
                line: format!(
                    "System: Job execution timed out after {} seconds.",
                    timeout_sec
                ),
                logged_at: now,
            };
            let _ = crate::db::logs::append_log_line(&state.db, &log_line).await;

            // 2. ステータスを Failed に更新
            let duration_ms =
                started_at.map(|start| now.signed_duration_since(start).num_milliseconds());
            let _ = crate::db::runs::update_run_status(
                &state.db,
                &run_id,
                RunStatus::Failed,
                None,
                Some("Execution timed out"),
                None,
                duration_ms,
            )
            .await;

            let _ = crate::notification::trigger_notifications(state, &run_id, "failed").await;
        }
    }

    Ok(())
}
