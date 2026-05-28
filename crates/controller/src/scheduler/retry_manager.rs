use crate::app::AppState;
use chrono::Utc;

/// リトライ待ちのジョブを確認し、再実行
pub async fn check_retries(state: &AppState) -> anyhow::Result<()> {
    let now = Utc::now();
    let result = sqlx::query(
        r#"UPDATE job_runs 
           SET status = 'pending', next_retry_at = NULL
           WHERE status = 'retrying' AND next_retry_at <= ?"#,
    )
    .bind(now)
    .execute(&state.db)
    .await?;

    let count = result.rows_affected();
    if count > 0 {
        tracing::info!(
            "Moved {} retrying runs back to pending status for re-execution",
            count
        );
    }

    Ok(())
}
