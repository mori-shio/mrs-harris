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
pub mod step_flows;
pub mod worker_definitions;
pub mod workers;

#[derive(Clone)]
pub struct BreadcrumbItem {
    pub label: String,
    pub href: String,
    pub icon: String,
    pub hx_boost: String,
    pub current: bool,
}

impl BreadcrumbItem {
    pub fn link(
        label: impl Into<String>,
        href: impl Into<String>,
        icon: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
            icon: icon.into(),
            hx_boost: String::new(),
            current: false,
        }
    }

    pub fn current(label: impl Into<String>, icon: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: String::new(),
            icon: icon.into(),
            hx_boost: String::new(),
            current: true,
        }
    }

    pub fn with_hx_boost(mut self, value: impl Into<String>) -> Self {
        self.hx_boost = value.into();
        self
    }
}

pub fn home_breadcrumb() -> BreadcrumbItem {
    BreadcrumbItem::link("ホーム", "/", "")
}

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
        .merge(step_flows::router())
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
