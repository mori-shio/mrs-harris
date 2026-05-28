use askama::Template;
use axum::{Router, extract::State, response::IntoResponse, routing::get};

use chrono::{DateTime, Utc};
use sqlx::Row;

use super::auth::WebClaims;
use crate::app::AppState;

#[derive(Clone)]
pub struct WorkerRenderItem {
    pub id_short: String,
    pub worker_type_str: String,
    pub external_id: String,
    pub external_id_short: String,
    pub status_str: String,
    pub run_id: i64,
    pub run_id_short: String,
    pub started_at_str: String,
    pub last_heartbeat_str: String,
}

#[derive(Template)]
#[template(path = "workers.html")]
struct WorkersTemplate;
crate::impl_into_response!(WorkersTemplate);

#[derive(Template)]
#[template(path = "components/workers_live.html")]
struct WorkersLiveTemplate {
    active_count: usize,
    fargate_count: usize,
    lambda_count: usize,
    workers: Vec<WorkerRenderItem>,
}
crate::impl_into_response!(WorkersLiveTemplate);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workers", get(workers_page))
        .route("/workers/live", get(workers_live))
}

async fn workers_page(_claims: WebClaims) -> impl IntoResponse {
    WorkersTemplate
}

async fn workers_live(_claims: WebClaims, State(state): State<AppState>) -> impl IntoResponse {
    let pool = &state.db;

    // 1. 各メトリクス数の取得
    let active_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM worker_tracking WHERE status = 'running'")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    let fargate_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM worker_tracking WHERE status = 'running' AND worker_type = 'fargate'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let lambda_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM worker_tracking WHERE status = 'running' AND worker_type = 'lambda'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // 2. 最新のワーカー履歴50件の取得
    let rows = sqlx::query("SELECT * FROM worker_tracking ORDER BY started_at DESC LIMIT 50")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let mut workers = Vec::new();
    for row in rows {
        if let Ok(item) = map_row_to_render_item(&row) {
            workers.push(item);
        }
    }

    WorkersLiveTemplate {
        active_count: active_count as usize,
        fargate_count: fargate_count as usize,
        lambda_count: lambda_count as usize,
        workers,
    }
}

fn map_row_to_render_item(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<WorkerRenderItem> {
    let id: i64 = row.try_get("id")?;
    let id_short = id.to_string()[..8].to_string();

    let worker_type_str: String = row.try_get("worker_type")?;
    let external_id: String = row.try_get("external_id")?;

    // AWS ARN または Request ID を読みやすく短縮
    let external_id_short = if external_id.contains('/') {
        external_id
            .split('/')
            .next_back()
            .unwrap_or(&external_id)
            .to_string()
    } else if external_id.len() > 16 {
        format!("{}...", &external_id[..16])
    } else {
        external_id.clone()
    };

    let status_str: String = row.try_get("status")?;

    let run_id: i64 = row.try_get("run_id")?;

    let run_id_short = run_id.to_string()[..8].to_string();

    let started_at: DateTime<Utc> = row.try_get("started_at")?;
    let started_at_str = started_at
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let last_heartbeat: Option<DateTime<Utc>> = row.try_get("last_heartbeat")?;
    let last_heartbeat_str = match last_heartbeat {
        Some(dt) => {
            let diff = Utc::now().signed_duration_since(dt);
            if diff.num_seconds() < 60 {
                format!("{}秒前", diff.num_seconds())
            } else if diff.num_minutes() < 60 {
                format!("{}分前", diff.num_minutes())
            } else {
                dt.with_timezone(&chrono::Local)
                    .format("%H:%M:%S")
                    .to_string()
            }
        }
        None => String::new(),
    };

    Ok(WorkerRenderItem {
        id_short,
        worker_type_str,
        external_id,
        external_id_short,
        status_str,
        run_id,
        run_id_short,
        started_at_str,
        last_heartbeat_str,
    })
}
