use axum::Router;
use crate::app::AppState;

pub mod auth;
pub mod callback;
pub mod calendar;
pub mod health;
pub mod jobs;
pub mod logs;
pub mod runs;
pub mod workers;

/// API ルーター
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(jobs::router())
        .merge(runs::router())
        .merge(calendar::router())
        .merge(workers::router())
        .merge(logs::router())
        .merge(auth::router())
        .merge(callback::router())
}
