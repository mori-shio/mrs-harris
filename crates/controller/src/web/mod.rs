use axum::Router;
use crate::app::AppState;

pub mod auth;
pub mod calendar;
pub mod dashboard;
pub mod jobs;
pub mod runs;
pub mod settings;
pub mod workers;
pub mod worker_definitions;

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

