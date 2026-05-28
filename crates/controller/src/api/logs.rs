use axum::{
    extract::{State, Path},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use mrs_harris_common::models::run::LogLine;
use mrs_harris_common::models::user::Claims;
use crate::app::AppState;


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs/{id}/logs", get(get_run_logs))
}

async fn get_run_logs(
    State(state): State<AppState>,
    _claims: Claims,
    Path(id): Path<i64>,
) -> Result<Json<Vec<LogLine>>, (StatusCode, Json<serde_json::Value>)> {
    let logs = crate::db::logs::get_logs(&state.db, &id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;
    Ok(Json(logs))
}
