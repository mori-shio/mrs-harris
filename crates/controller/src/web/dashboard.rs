use askama::Template;
use axum::{Router, extract::State, response::IntoResponse, routing::get};

use chrono::{DateTime, Duration, Utc};
use sqlx::{MySqlPool, Row};
use std::str::FromStr;

use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::run::RunStatus;
use mrs_harris_common::models::run::TriggerType;

use super::auth::WebClaims;
use crate::app::AppState;

#[derive(Clone)]
pub struct DashboardRunItem {
    pub id: i64,
    pub job_name: String,
    pub status_badge_class: &'static str,
    pub status_ja: String,
    pub worker_type: WorkerType,
    pub trigger_ja: String,
    pub duration_str: String,
    pub started_at_str: String,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    active_jobs_count: usize,
    total_jobs_count: usize,
    today_runs_count: usize,
    success_rate_percent: u32,
    succeeded_today: usize,
    failed_today: usize,
    active_workers_count: usize,
    recent_runs: Vec<DashboardRunItem>,
}
crate::impl_into_response!(DashboardTemplate);

#[derive(Template)]
#[template(path = "components/dashboard_live.html")]
struct DashboardLiveTemplate {
    active_jobs_count: usize,
    total_jobs_count: usize,
    today_runs_count: usize,
    success_rate_percent: u32,
    succeeded_today: usize,
    failed_today: usize,
    active_workers_count: usize,
    recent_runs: Vec<DashboardRunItem>,
}
crate::impl_into_response!(DashboardLiveTemplate);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/dashboard/live", get(dashboard_live))
}

async fn fetch_dashboard_data(pool: &MySqlPool) -> anyhow::Result<DashboardLiveTemplate> {
    // 1. 全ジョブ定義数
    let total_jobs_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(pool)
        .await?;

    // 2. 有効なジョブ定義数
    let active_jobs_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE is_active = 1")
            .fetch_one(pool)
            .await?;

    // 3. 直近24時間の実行
    let cutoff = Utc::now() - Duration::hours(24);
    let today_runs_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_runs WHERE created_at >= ?")
            .bind(cutoff)
            .fetch_one(pool)
            .await?;

    // 4. 24時間以内の成功数
    let succeeded_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM job_runs WHERE status = 'succeeded' AND created_at >= ?",
    )
    .bind(cutoff)
    .fetch_one(pool)
    .await?;

    // 5. 24時間以内の失敗数
    let failed_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM job_runs WHERE status IN ('failed', 'dead_letter') AND created_at >= ?"
    )
    .bind(cutoff)
    .fetch_one(pool)
    .await?;

    // 6. 成功率の算出
    let success_rate_percent = if today_runs_count > 0 {
        ((succeeded_today as f64 / today_runs_count as f64) * 100.0).round() as u32
    } else {
        100 // 実行がゼロの場合は100%表示
    };

    // 7. アクティブワーカー数
    let active_workers_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workers WHERE status = 'running'")
            .fetch_one(pool)
            .await?;

    // 8. 直近の実行履歴（10件）
    let rows = sqlx::query(
        r#"SELECT r.id, r.status, wd.worker_type, r.trigger_type, r.started_at, r.duration_ms, j.name as job_name
           FROM job_runs r
           JOIN jobs j ON r.job_id = j.id
           LEFT JOIN workers w ON r.worker_id = w.id
           LEFT JOIN worker_definitions wd ON w.worker_definition_id = wd.id
           ORDER BY r.created_at DESC
           LIMIT 10"#
    )
    .fetch_all(pool)
    .await?;

    let mut recent_runs = Vec::new();
    for r in rows {
        let id: i64 = r.try_get("id")?;
        let job_name: String = r.try_get("job_name")?;

        let status_str: String = r.try_get("status")?;
        let status = RunStatus::from_str(&status_str)?;

        let worker_type_str: String = r.try_get("worker_type")?;
        let worker_type = WorkerType::from_str(&worker_type_str)?;

        let trigger_type_str: String = r.try_get("trigger_type")?;
        let trigger_type = TriggerType::from_str(&trigger_type_str)?;

        let started_at: Option<DateTime<Utc>> = r.try_get("started_at")?;
        let duration_ms: Option<i64> = r.try_get("duration_ms")?;

        let status_ja = status.label_ja().to_string();
        let trigger_ja = trigger_type.label_ja().to_string();

        let duration_str = match duration_ms {
            Some(ms) => {
                if ms >= 1000 {
                    format!("{:.1}s", ms as f64 / 1000.0)
                } else {
                    format!("{}ms", ms)
                }
            }
            None => "-".to_string(),
        };

        let started_at_str = match started_at {
            Some(dt) => dt
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            None => "-".to_string(),
        };

        recent_runs.push(DashboardRunItem {
            id,
            job_name,
            status_badge_class: status.badge_class(),
            status_ja,
            worker_type,
            trigger_ja,
            duration_str,
            started_at_str,
        });
    }

    Ok(DashboardLiveTemplate {
        active_jobs_count: active_jobs_count as usize,
        total_jobs_count: total_jobs_count as usize,
        today_runs_count: today_runs_count as usize,
        success_rate_percent,
        succeeded_today: succeeded_today as usize,
        failed_today: failed_today as usize,
        active_workers_count: active_workers_count as usize,
        recent_runs,
    })
}

async fn dashboard(_claims: WebClaims, State(state): State<AppState>) -> impl IntoResponse {
    match fetch_dashboard_data(&state.db).await {
        Ok(data) => DashboardTemplate {
            active_jobs_count: data.active_jobs_count,
            total_jobs_count: data.total_jobs_count,
            today_runs_count: data.today_runs_count,
            success_rate_percent: data.success_rate_percent,
            succeeded_today: data.succeeded_today,
            failed_today: data.failed_today,
            active_workers_count: data.active_workers_count,
            recent_runs: data.recent_runs,
        }
        .into_response(),
        Err(e) => {
            tracing::error!("Dashboard data fetch error: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal Server Error: {}", e),
            )
                .into_response()
        }
    }
}

async fn dashboard_live(_claims: WebClaims, State(state): State<AppState>) -> impl IntoResponse {
    match fetch_dashboard_data(&state.db).await {
        Ok(data) => data.into_response(),
        Err(e) => {
            tracing::error!("Live dashboard data fetch error: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal Server Error: {}", e),
            )
                .into_response()
        }
    }
}
