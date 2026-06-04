use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use chrono::Utc;
use sqlx::Row;

use std::str::FromStr;

use super::auth::WebClaims;
use crate::app::AppState;
use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::worker::{
    NewWorkerDefinition, WorkerDefinition, WorkerDefinitionUpdate,
};

#[derive(Template)]
#[template(path = "worker_definitions/list.html")]
struct WorkerDefListTemplate {
    defs: Vec<WorkerDefinition>,
    toast_message: Option<&'static str>,
}
crate::impl_into_response!(WorkerDefListTemplate);

#[derive(Template)]
#[template(path = "worker_definitions/form.html")]
struct WorkerDefFormTemplate {
    is_edit: bool,
    def_name: Option<String>,
    name: String,
    description: String,
    worker_type: String,
    config_json: String,
    lambda_function_arn: String,
}
crate::impl_into_response!(WorkerDefFormTemplate);

#[derive(Template)]
#[template(path = "worker_definitions/detail.html")]
struct WorkerDefDetailTemplate {
    def: WorkerDefinition,
    history: Vec<WorkerDefinitionHistoryRenderItem>,
    latest_version: u32,
}
crate::impl_into_response!(WorkerDefDetailTemplate);

#[derive(Clone)]
struct WorkerDefinitionHistoryRenderItem {
    version: u32,
    payload_json: String,
    changed_by: String,
    changed_at_str: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct WorkerDefFormData {
    name: String,
    description: Option<String>,
    worker_type: String,
    config_json: Option<String>,
    lambda_function_arn: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct WorkerDefListQuery {
    toast: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/worker-definitions", get(list_defs))
        .route(
            "/worker-definitions/new",
            get(new_def_page).post(create_def_submit),
        )
        .route("/worker-definitions/{name}", get(def_detail_page))
        .route(
            "/worker-definitions/{name}/edit",
            get(edit_def_page).post(edit_def_submit),
        )
        .route("/worker-definitions/{name}/delete", post(delete_def))
}

async fn list_defs(
    _claims: WebClaims,
    State(state): State<AppState>,
    Query(query): Query<WorkerDefListQuery>,
) -> impl IntoResponse {
    let defs = crate::db::workers::list_worker_definitions(&state.db)
        .await
        .unwrap_or_default();
    let toast_message = match query.toast.as_deref() {
        Some("deleted") => Some("ワーカー定義を削除しました。"),
        _ => None,
    };
    WorkerDefListTemplate {
        defs,
        toast_message,
    }
}

async fn new_def_page(_claims: WebClaims) -> impl IntoResponse {
    WorkerDefFormTemplate {
        is_edit: false,
        def_name: None,
        name: String::new(),
        description: String::new(),
        worker_type: "fargate".to_string(),
        config_json: r#"{
  "cluster_arn": "arn:aws:ecs:ap-northeast-1:ACCOUNT_ID:cluster/mrs-harris",
  "task_definition": "mrs-harris-worker:1",
  "subnets": ["subnet-xxxxxxxx", "subnet-xxxxxxxx"],
  "security_groups": ["sg-xxxxxxxx"],
  "container_name": "mrs-harris-worker",
  "assign_public_ip": true
}"#
        .to_string(),
        lambda_function_arn: String::new(),
    }
}

async fn create_def_submit(
    claims: WebClaims,
    State(state): State<AppState>,
    Form(form): Form<WorkerDefFormData>,
) -> impl IntoResponse {
    let worker_type = WorkerType::from_str(&form.worker_type).unwrap_or(WorkerType::Fargate);
    let config: serde_json::Value = match worker_type {
        WorkerType::Fargate => serde_json::from_str(form.config_json.as_deref().unwrap_or(""))
            .unwrap_or_else(|_| {
                serde_json::json!({
                    "cluster_arn": "",
                    "task_definition": "",
                    "subnets": [],
                    "security_groups": [],
                    "container_name": "mrs-harris-worker"
                })
            }),
        WorkerType::Lambda => {
            let function_name = form.lambda_function_arn.clone().unwrap_or_default();
            serde_json::json!({
                "function_name": function_name
            })
        }
    };

    let new_def = NewWorkerDefinition {
        name: form.name,
        description: form.description.filter(|d| !d.trim().is_empty()),
        worker_type,
        config,
    };

    let created = crate::db::workers::create_worker_definition(&state.db, &new_def)
        .await
        .unwrap();
    let _ = record_worker_definition_history(&state.db, &created.id, &claims.0.username).await;
    Redirect::to(&format!("/worker-definitions/{}", created.name)).into_response()
}

async fn def_detail_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let def = crate::db::workers::get_worker_definition_by_name(&state.db, &name)
        .await
        .unwrap()
        .unwrap();
    let history = list_worker_definition_history(&state.db, &def.id)
        .await
        .unwrap_or_default();
    let latest_version = history.first().map(|h| h.version).unwrap_or(1);
    WorkerDefDetailTemplate {
        def,
        history,
        latest_version,
    }
}

async fn edit_def_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let def = crate::db::workers::get_worker_definition_by_name(&state.db, &name)
        .await
        .unwrap()
        .unwrap();
    let config_json = serde_json::to_string_pretty(&def.config).unwrap_or_default();
    let lambda_function_arn = def
        .config
        .get("function_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    WorkerDefFormTemplate {
        is_edit: true,
        def_name: Some(def.name.clone()),
        name: def.name,
        description: def.description.unwrap_or_default(),
        worker_type: def.worker_type.to_string(),
        config_json,
        lambda_function_arn,
    }
}

async fn edit_def_submit(
    claims: WebClaims,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Form(form): Form<WorkerDefFormData>,
) -> impl IntoResponse {
    let def = crate::db::workers::get_worker_definition_by_name(&state.db, &name)
        .await
        .unwrap()
        .unwrap();
    let id = def.id;

    let worker_type = WorkerType::from_str(&form.worker_type).unwrap_or(WorkerType::Fargate);
    let config: serde_json::Value = match worker_type {
        WorkerType::Fargate => serde_json::from_str(form.config_json.as_deref().unwrap_or(""))
            .unwrap_or(serde_json::Value::Null),
        WorkerType::Lambda => serde_json::json!({
            "function_name": form.lambda_function_arn.clone().unwrap_or_default()
        }),
    };

    let update = WorkerDefinitionUpdate {
        description: Some(form.description.unwrap_or_default()),
        worker_type: Some(worker_type),
        config: Some(config),
    };

    let _ = crate::db::workers::update_worker_definition(&state.db, &id, &update)
        .await
        .unwrap();
    let _ = record_worker_definition_history(&state.db, &id, &claims.0.username).await;
    Redirect::to(&format!("/worker-definitions/{}", name)).into_response()
}

async fn delete_def(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let def = crate::db::workers::get_worker_definition_by_name(&state.db, &name)
        .await
        .unwrap()
        .unwrap();
    crate::db::workers::delete_worker_definition(&state.db, &def.id)
        .await
        .unwrap();
    Redirect::to("/worker-definitions?toast=deleted").into_response()
}

async fn build_worker_definition_snapshot(
    pool: &sqlx::MySqlPool,
    worker_definition_id: &i64,
) -> serde_json::Value {
    let def = match crate::db::workers::get_worker_definition(pool, worker_definition_id).await {
        Ok(Some(def)) => def,
        _ => return serde_json::Value::Null,
    };

    serde_json::json!({
        "ワーカー名": def.name,
        "説明": def.description.unwrap_or_default(),
        "ワーカータイプ": def.worker_type.to_string(),
        "設定": def.config
    })
}

async fn list_worker_definition_history(
    pool: &sqlx::MySqlPool,
    worker_definition_id: &i64,
) -> anyhow::Result<Vec<WorkerDefinitionHistoryRenderItem>> {
    let existing =
        crate::db::workers::list_worker_definition_history(pool, worker_definition_id).await?;
    if !existing.is_empty() {
        return Ok(existing
            .into_iter()
            .map(|item| WorkerDefinitionHistoryRenderItem {
                version: item.version,
                payload_json: serde_json::to_string(&item.payload)
                    .unwrap_or_else(|_| "{}".to_string()),
                changed_by: item.changed_by,
                changed_at_str: item.changed_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            })
            .collect());
    }

    record_worker_definition_history(pool, worker_definition_id, "system").await?;
    let created =
        crate::db::workers::list_worker_definition_history(pool, worker_definition_id).await?;
    Ok(created
        .into_iter()
        .map(|item| WorkerDefinitionHistoryRenderItem {
            version: item.version,
            payload_json: serde_json::to_string(&item.payload).unwrap_or_else(|_| "{}".to_string()),
            changed_by: item.changed_by,
            changed_at_str: item.changed_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        })
        .collect())
}

async fn record_worker_definition_history(
    pool: &sqlx::MySqlPool,
    worker_definition_id: &i64,
    changed_by: &str,
) -> anyhow::Result<()> {
    let snapshot = build_worker_definition_snapshot(pool, worker_definition_id).await;
    if snapshot.is_null() {
        return Ok(());
    }

    let version_row = sqlx::query(
        "SELECT MAX(version) as max_v FROM worker_definition_history WHERE worker_definition_id = ?",
    )
    .bind(worker_definition_id)
    .fetch_one(pool)
    .await?;

    let current_max: Option<u32> = version_row.try_get("max_v").ok();
    let next_version = current_max.unwrap_or(0) + 1;

    sqlx::query(
        r#"INSERT INTO worker_definition_history (worker_definition_id, version, payload, changed_by, changed_at)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(worker_definition_id)
    .bind(next_version)
    .bind(snapshot)
    .bind(changed_by)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(())
}
