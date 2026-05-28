use crate::app::AppState;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use mrs_harris_common::models::job::{Job, JobFilter, JobUpdate, NewJob};
use mrs_harris_common::models::user::Claims;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/jobs", get(list_jobs).post(create_job))
        .route(
            "/jobs/{name}",
            get(get_job).put(update_job).delete(delete_job),
        )
        .route("/jobs/{name}/trigger", axum::routing::post(trigger_job))
}

async fn list_jobs(
    State(state): State<AppState>,
    _claims: Claims, // 認証必須
    Query(filter): Query<JobFilter>,
) -> Result<Json<Vec<Job>>, (StatusCode, Json<serde_json::Value>)> {
    let jobs = crate::db::jobs::list_jobs(&state.db, &filter)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;
    Ok(Json(jobs))
}

async fn create_job(
    State(state): State<AppState>,
    _claims: Claims,
    Json(payload): Json<NewJob>,
) -> Result<(StatusCode, Json<Job>), (StatusCode, Json<serde_json::Value>)> {
    // payload (JSON の中身) の簡単なバリデーション
    if payload.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Job name cannot be empty" })),
        ));
    }

    let job = crate::db::jobs::create_job(&state.db, &payload)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to create job: {}", e) })),
            )
        })?;
    Ok((StatusCode::CREATED, Json(job)))
}

async fn get_job(
    State(state): State<AppState>,
    _claims: Claims,
    Path(name): Path<String>,
) -> Result<Json<Job>, (StatusCode, Json<serde_json::Value>)> {
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    match job_opt {
        Some(job) => Ok(Json(job)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Job not found" })),
        )),
    }
}

async fn update_job(
    State(state): State<AppState>,
    _claims: Claims,
    Path(name): Path<String>,
    Json(payload): Json<JobUpdate>,
) -> Result<Json<Job>, (StatusCode, Json<serde_json::Value>)> {
    // 存在チェック
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;
    let job = match job_opt {
        Some(j) => j,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Job not found" })),
            ));
        }
    };

    let updated_job = crate::db::jobs::update_job(&state.db, &job.id, &payload)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to update job: {}", e) })),
            )
        })?;
    Ok(Json(updated_job))
}

async fn delete_job(
    State(state): State<AppState>,
    _claims: Claims,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // 存在チェック
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;
    let job = match job_opt {
        Some(j) => j,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Job not found" })),
            ));
        }
    };

    crate::db::jobs::delete_job(&state.db, &job.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to delete job: {}", e) })),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

async fn trigger_job(
    State(state): State<AppState>,
    _claims: Claims,
    Path(name): Path<String>,
) -> Result<Json<mrs_harris_common::models::run::JobRun>, (StatusCode, Json<serde_json::Value>)> {
    // 1. ジョブ定義を取得
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    let job = match job_opt {
        Some(j) => j,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Job not found" })),
            ));
        }
    };

    // 2. 新規 Run (実行履歴) の作成
    let new_run = mrs_harris_common::models::run::NewRun {
        job_id: job.id,
        worker_type: job.worker_type,
        trigger_type: mrs_harris_common::models::run::TriggerType::Manual,
        scheduled_at: Some(chrono::Utc::now()),
        worker_definition_id: job.worker_definition_id,
    };

    let run = crate::db::runs::create_run(&state.db, &new_run)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to trigger run: {}", e) })),
            )
        })?;

    // 3. バックグラウンドでワーカーディスパッチャーを呼ぶ
    // (Scheduler でポーリングして queued -> running に移譲する構成をとっているため、ここでは create_run だけでOKですが、
    //  即時に Worker Manager にディスパッチ要求を送り出すことも可能です。
    //  ここでは scheduled_at = Utc::now() としており、スケジューラが次のループで即検知して実行します。)

    Ok(Json(run))
}
