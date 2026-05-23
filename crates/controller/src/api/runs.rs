use axum::{
    extract::{State, Path, Query},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use mrs_harris_common::models::run::{JobRun, RunStatus};
use mrs_harris_common::models::user::Claims;
use crate::app::AppState;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct RunFilter {
    pub job_id: Option<Uuid>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs", get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/cancel", axum::routing::post(cancel_run))
}

async fn list_runs(
    State(state): State<AppState>,
    _claims: Claims,
    Query(filter): Query<RunFilter>,
) -> Result<Json<Vec<JobRun>>, (StatusCode, Json<serde_json::Value>)> {
    let runs = crate::db::runs::list_runs(
        &state.db,
        filter.job_id.as_ref(),
        filter.limit,
        filter.offset,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
        )
    })?;
    Ok(Json(runs))
}

async fn get_run(
    State(state): State<AppState>,
    _claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<JobRun>, (StatusCode, Json<serde_json::Value>)> {
    let run_opt = crate::db::runs::get_run(&state.db, &id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    match run_opt {
        Some(run) => Ok(Json(run)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Run not found" })),
        )),
    }
}

async fn cancel_run(
    State(state): State<AppState>,
    _claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let run_opt = crate::db::runs::get_run(&state.db, &id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    let run = match run_opt {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Run not found" })),
            ));
        }
    };

    if run.status.is_terminal() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("Cannot cancel run in terminal status: {}", run.status) })),
        ));
    }

    crate::db::runs::update_run_status(
        &state.db,
        &id,
        RunStatus::Cancelled,
        None,
        Some("Cancelled by user"),
        None,
        None,
        run.version,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to cancel run: {}", e) })),
        )
    })?;

    Ok(StatusCode::OK)
}
