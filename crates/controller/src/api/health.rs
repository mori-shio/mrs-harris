use axum::{Router, routing::get, Json};
use crate::app::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "mrs-harris",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
