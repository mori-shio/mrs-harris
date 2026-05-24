use axum::{
    extract::{State, Path},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use askama::Template;
use uuid::Uuid;
use chrono::Utc;
use sqlx::{MySqlPool, Row};

use mrs_harris_common::models::space::Space;
use crate::app::AppState;
use super::auth::WebClaims;

#[derive(serde::Serialize, Clone, Debug)]
pub struct SpaceRenderItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub job_count: i64,
    pub created_at_str: String,
}

#[derive(Template)]
#[template(path = "spaces/list.html")]
struct SpaceListTemplate {
    spaces: Vec<SpaceRenderItem>,
    unclassified_job_count: i64,
}
crate::impl_into_response!(SpaceListTemplate);

#[derive(Template)]
#[template(path = "spaces/form.html")]
struct SpaceFormTemplate {
    is_edit: bool,
    space_id: Option<Uuid>,
    name: String,
    description: String,
}
crate::impl_into_response!(SpaceFormTemplate);

#[derive(serde::Deserialize, Debug)]
pub struct SpaceFormData {
    name: String,
    description: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces", get(list_spaces))
        .route("/spaces/new", get(new_space_page).post(create_space_submit))
        .route("/spaces/{id}/edit", get(edit_space_page).post(edit_space_submit))
        .route("/spaces/{id}/delete", post(delete_space))
}

async fn list_spaces(
    _claims: WebClaims,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let pool = &state.db;

    // スペース一覧および各スペースのジョブ数を取得
    let rows = sqlx::query(
        r#"SELECT s.id, s.name, s.description, s.created_at, COUNT(j.id) as job_count
           FROM spaces s
           LEFT JOIN jobs j ON j.space_id = s.id
           GROUP BY s.id
           ORDER BY s.name ASC"#
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut spaces = Vec::new();
    for row in rows {
        let id: String = row.try_get("id").unwrap_or_default();
        let name: String = row.try_get("name").unwrap_or_default();
        let description: Option<String> = row.try_get("description").ok();
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now());
        let job_count: i64 = row.try_get("job_count").unwrap_or(0);

        spaces.push(SpaceRenderItem {
            id,
            name,
            description: description.unwrap_or_default(),
            job_count,
            created_at_str: created_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string(),
        });
    }

    // 未分類のジョブ数をカウント
    let unclassified_row = sqlx::query("SELECT COUNT(id) as job_count FROM jobs WHERE space_id IS NULL")
        .fetch_one(pool)
        .await
        .unwrap();
    let unclassified_job_count: i64 = unclassified_row.try_get("job_count").unwrap_or(0);

    SpaceListTemplate {
        spaces,
        unclassified_job_count,
    }
}

async fn new_space_page(_claims: WebClaims) -> impl IntoResponse {
    SpaceFormTemplate {
        is_edit: false,
        space_id: None,
        name: String::new(),
        description: String::new(),
    }
}

async fn create_space_submit(
    _claims: WebClaims,
    State(state): State<AppState>,
    Form(form): Form<SpaceFormData>,
) -> impl IntoResponse {
    let pool = &state.db;
    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r#"INSERT INTO spaces (id, name, description, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?)"#
    )
    .bind(id.to_string())
    .bind(&form.name.trim())
    .bind(&form.description.filter(|d| !d.trim().is_empty()))
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    Redirect::to("/spaces").into_response()
}

async fn edit_space_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = &state.db;
    let row = sqlx::query("SELECT id, name, description FROM spaces WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(pool)
        .await
        .unwrap();

    let name: String = row.try_get("name").unwrap_or_default();
    let description: Option<String> = row.try_get("description").ok();

    SpaceFormTemplate {
        is_edit: true,
        space_id: Some(id),
        name,
        description: description.unwrap_or_default(),
    }
}

async fn edit_space_submit(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<SpaceFormData>,
) -> impl IntoResponse {
    let pool = &state.db;
    let now = Utc::now();

    sqlx::query(
        r#"UPDATE spaces
           SET name = ?, description = ?, updated_at = ?
           WHERE id = ?"#
    )
    .bind(&form.name.trim())
    .bind(&form.description.filter(|d| !d.trim().is_empty()))
    .bind(now)
    .bind(id.to_string())
    .execute(pool)
    .await
    .unwrap();

    Redirect::to("/spaces").into_response()
}

async fn delete_space(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = &state.db;

    sqlx::query("DELETE FROM spaces WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .unwrap();

    Redirect::to("/spaces").into_response()
}
