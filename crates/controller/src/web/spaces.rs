use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
};

use chrono::Utc;
use sqlx::Row;

use super::auth::WebClaims;
use crate::app::AppState;

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
    spaces_json: String,
}
crate::impl_into_response!(SpaceListTemplate);

#[derive(serde::Serialize, Clone, Debug)]
struct SpaceModalItem {
    id: i64,
    name: String,
    description: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct SpaceFormData {
    name: String,
    description: Option<String>,
    redirect_to: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces", get(list_spaces))
        .route("/spaces/new", post(create_space_submit))
        .route("/spaces/{id}/edit", post(edit_space_submit))
        .route("/spaces/{id}/delete", post(delete_space))
}

async fn list_spaces(_claims: WebClaims, State(state): State<AppState>) -> impl IntoResponse {
    let pool = &state.db;

    // スペース一覧および各スペースのジョブ数を取得
    let rows = sqlx::query(
        r#"SELECT s.id, s.name, s.description, s.created_at, COUNT(j.id) as job_count
           FROM spaces s
           LEFT JOIN jobs j ON j.space_id = s.id
           GROUP BY s.id
           ORDER BY s.name ASC"#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut spaces = Vec::new();
    let mut spaces_modal = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("id").unwrap_or_default();
        let name: String = row.try_get("name").unwrap_or_default();
        let description: Option<String> = row.try_get("description").ok();
        let description_text = description.unwrap_or_default();
        let created_at: chrono::DateTime<chrono::Utc> = row
            .try_get("created_at")
            .unwrap_or_else(|_| chrono::Utc::now());
        let job_count: i64 = row.try_get("job_count").unwrap_or(0);

        spaces.push(SpaceRenderItem {
            id: id.to_string(),
            name: name.clone(),
            description: description_text.clone(),
            job_count,
            created_at_str: created_at
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        });
        spaces_modal.push(SpaceModalItem {
            id,
            name,
            description: description_text,
        });
    }

    // 未分類のジョブ数をカウント
    let unclassified_row =
        sqlx::query("SELECT COUNT(id) as job_count FROM jobs WHERE space_id IS NULL")
            .fetch_one(pool)
            .await
            .unwrap();
    let unclassified_job_count: i64 = unclassified_row.try_get("job_count").unwrap_or(0);
    let spaces_json = serde_json::to_string(&spaces_modal).unwrap_or_else(|_| "[]".to_string());

    SpaceListTemplate {
        spaces,
        unclassified_job_count,
        spaces_json,
    }
}

async fn create_space_submit(
    _claims: WebClaims,
    State(state): State<AppState>,
    Form(form): Form<SpaceFormData>,
) -> impl IntoResponse {
    let pool = &state.db;
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO spaces (name, description, created_at, updated_at)
           VALUES (?, ?, ?, ?)"#,
    )
    .bind(form.name.trim())
    .bind(
        form.description
            .as_ref()
            .map(|d| d.trim())
            .filter(|d| !d.is_empty()),
    )
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let redirect_url = form
        .redirect_to
        .filter(|r| !r.trim().is_empty())
        .unwrap_or_else(|| "/spaces".to_string());
    Redirect::to(&redirect_url).into_response()
}

async fn edit_space_submit(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<SpaceFormData>,
) -> impl IntoResponse {
    let pool = &state.db;
    let now = Utc::now();

    sqlx::query(
        r#"UPDATE spaces
           SET name = ?, description = ?, updated_at = ?
           WHERE id = ?"#,
    )
    .bind(form.name.trim())
    .bind(form.description.filter(|d| !d.trim().is_empty()))
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    let redirect_url = form
        .redirect_to
        .filter(|r| !r.trim().is_empty())
        .unwrap_or_else(|| "/spaces".to_string());
    Redirect::to(&redirect_url).into_response()
}

async fn delete_space(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let pool = &state.db;

    sqlx::query("DELETE FROM spaces WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();

    Redirect::to("/spaces").into_response()
}
