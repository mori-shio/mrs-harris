use crate::app::AppState;
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use mrs_harris_common::models::user::Claims;
use mrs_harris_common::models::worker::Worker;

pub fn router() -> Router<AppState> {
    Router::new().route("/workers", get(list_workers))
}

async fn list_workers(
    State(state): State<AppState>,
    _claims: Claims,
) -> Result<Json<Vec<Worker>>, (StatusCode, Json<serde_json::Value>)> {
    let workers = crate::db::workers::list_active_workers(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;
    Ok(Json(workers))
}
