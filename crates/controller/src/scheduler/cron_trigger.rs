use crate::app::AppState;
use chrono::Utc;
use cron::Schedule;
use mrs_harris_common::models::job::JobType;
use mrs_harris_common::models::run::{NewRun, TriggerType};
use std::str::FromStr;

/// Cron 式に基づいてジョブをトリガー
pub async fn check_cron_jobs(state: &AppState) -> anyhow::Result<()> {
    // 1. アクティブなジョブ一覧を取得
    let filter = mrs_harris_common::models::job::JobFilter {
        is_active: Some(true),
        job_type: Some(JobType::Cron),
        ..Default::default()
    };
    let jobs = crate::db::jobs::list_jobs(&state.db, &filter).await?;

    for job in jobs {
        let schedule_expr = match &job.schedule_expr {
            Some(expr) if !expr.trim().is_empty() => expr,
            _ => continue,
        };

        let schedule = match Schedule::from_str(schedule_expr) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Invalid cron expression for job {}: {}", job.name, e);
                continue;
            }
        };

        // 2. 最新の実行レコードを取得
        let latest_runs =
            crate::db::runs::list_runs(&state.db, Some(&job.id), Some(1), None, true).await?;
        let latest_run = latest_runs.first();

        //chrono v0.4 の TimeDelta (Duration) を使う
        let next_run_time = if let Some(run) = latest_run {
            // 前回の予定時刻がある場合、その予定時刻の「次」を求める
            let base_time = run.scheduled_at.unwrap_or(run.created_at);
            schedule.after(&base_time).next()
        } else {
            // 初回の場合、現在時刻より少し前の時刻を起点にして「次」の実行予定を求める
            schedule
                .after(&(Utc::now() - chrono::Duration::seconds(1)))
                .next()
        };

        if let Some(scheduled_at) = next_run_time {
            // 現在時刻を過ぎていたらトリガーする
            if scheduled_at <= Utc::now() {
                tracing::info!(
                    "Triggering cron job '{}' for scheduled time {:?}",
                    job.name,
                    scheduled_at
                );

                let new_run = NewRun {
                    job_id: job.id,
                    worker_type: job.worker_type,
                    trigger_type: TriggerType::Scheduled,
                    scheduled_at: Some(scheduled_at),
                    worker_definition_id: job.worker_definition_id,
                    worker_definition_history_id: None,
                };

                if let Err(e) = crate::db::runs::create_run(&state.db, &new_run).await {
                    tracing::error!("Failed to create run for job '{}': {}", job.name, e);
                }
            }
        }
    }

    Ok(())
}
