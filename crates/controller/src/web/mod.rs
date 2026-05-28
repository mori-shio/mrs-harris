use crate::app::AppState;
use axum::Router;

pub mod auth;
pub mod calendar;
pub mod dashboard;
pub mod database;
pub mod jobs;
pub mod runs;
pub mod settings;
pub mod spaces;
pub mod worker_definitions;
pub mod workers;

/// Web ダッシュボードルーター
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(dashboard::router())
        .merge(auth::router())
        .merge(calendar::router())
        .merge(jobs::router())
        .merge(runs::router())
        .merge(workers::router())
        .merge(settings::router())
        .merge(worker_definitions::router())
        .merge(database::router())
        .merge(spaces::router())
}

/// Askama テンプレート用の IntoResponse 簡易実装マクロ
#[macro_export]
macro_rules! impl_into_response {
    ($t:ty) => {
        impl axum::response::IntoResponse for $t {
            fn into_response(self) -> axum::response::Response {
                match askama::Template::render(&self) {
                    Ok(html) => axum::response::Html(html).into_response(),
                    Err(err) => (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Template rendering failed: {}", err),
                    )
                        .into_response(),
                }
            }
        }
    };
}
