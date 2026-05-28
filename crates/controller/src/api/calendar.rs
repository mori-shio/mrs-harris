use crate::app::AppState;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::get,
};
use mrs_harris_common::models::calendar::CalendarEntry;
use mrs_harris_common::models::user::Claims;

#[derive(serde::Deserialize)]
pub struct CalendarRange {
    pub from: chrono::DateTime<chrono::Utc>,
    pub to: chrono::DateTime<chrono::Utc>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/calendar/events", get(get_calendar_events))
}

async fn get_calendar_events(
    State(state): State<AppState>,
    _claims: Claims,
    Query(range): Query<CalendarRange>,
) -> Result<Json<Vec<CalendarEntry>>, (StatusCode, Json<serde_json::Value>)> {
    let entries = crate::db::runs::get_runs_for_calendar(&state.db, range.from, range.to)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;
    Ok(Json(entries))
}
