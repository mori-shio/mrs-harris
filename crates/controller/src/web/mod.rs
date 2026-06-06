use crate::app::AppState;
use axum::Router;
use sqlx::{MySqlPool, Row};

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

pub async fn linked_space_breadcrumb(
    pool: &MySqlPool,
    space_id: Option<i64>,
    list_path: &str,
) -> BreadcrumbItem {
    let Some(space_id) = space_id else {
        return BreadcrumbItem::link(
            "未分類",
            format!("{list_path}?space=unclassified"),
            "help-circle",
        );
    };

    let name = sqlx::query("SELECT name FROM spaces WHERE id = ?")
        .bind(space_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get("name").ok())
        .unwrap_or_else(|| "未分類".to_string());

    BreadcrumbItem::link(name, format!("{list_path}?space={space_id}"), "folder")
}

pub fn space_scoped_list_url(list_path: &str, space_id: Option<i64>) -> String {
    match space_id {
        Some(space_id) => format!("{list_path}?space={space_id}"),
        None => format!("{list_path}?space=unclassified"),
    }
}

pub fn highlight_search_match_html(text: &str, search: Option<&str>) -> String {
    let Some(query) = search.map(str::trim).filter(|value| !value.is_empty()) else {
        return escape_html(text);
    };

    let query_lower = query.to_lowercase();
    let Some((start_byte, end_byte)) = find_case_insensitive_match_bounds(text, &query_lower)
    else {
        return escape_html(text);
    };

    format!(
        "{}<span class=\"search-match-highlight\">{}</span>{}",
        escape_html(&text[..start_byte]),
        escape_html(&text[start_byte..end_byte]),
        escape_html(&text[end_byte..])
    )
}

fn find_case_insensitive_match_bounds(text: &str, query_lower: &str) -> Option<(usize, usize)> {
    let mut boundaries = text.char_indices().map(|(idx, _)| idx).collect::<Vec<_>>();
    boundaries.push(text.len());

    for start_pos in 0..boundaries.len().saturating_sub(1) {
        let start_byte = boundaries[start_pos];
        for end_byte in boundaries.iter().skip(start_pos + 1).copied() {
            let candidate = text[start_byte..end_byte].to_lowercase();
            if candidate == query_lower {
                return Some((start_byte, end_byte));
            }
            if candidate.len() >= query_lower.len() {
                break;
            }
        }
    }

    None
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
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
