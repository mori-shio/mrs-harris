use axum::{response::IntoResponse, routing::get, Router};
use askama::Template;
use super::auth::WebClaims;
use crate::app::AppState;

#[derive(Template)]
#[template(path = "calendar.html")]
struct CalendarTemplate;
crate::impl_into_response!(CalendarTemplate);

pub fn router() -> Router<AppState> {
    Router::new().route("/calendar", get(calendar_page))
}

async fn calendar_page(_claims: WebClaims) -> impl IntoResponse {
    CalendarTemplate
}
