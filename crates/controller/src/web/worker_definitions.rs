use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
};

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
}
crate::impl_into_response!(WorkerDefListTemplate);

#[derive(Template)]
#[template(path = "worker_definitions/form.html")]
struct WorkerDefFormTemplate {
    is_edit: bool,
    def_id: Option<i64>,
    name: String,
    description: String,
    worker_type: String,
    config_json: String,
    lambda_function_arn: String,
    is_active: bool,
}
crate::impl_into_response!(WorkerDefFormTemplate);

#[derive(Template)]
#[template(path = "worker_definitions/detail.html")]
struct WorkerDefDetailTemplate {
    def: WorkerDefinition,
}
crate::impl_into_response!(WorkerDefDetailTemplate);

#[derive(serde::Deserialize, Debug)]
pub struct WorkerDefFormData {
    name: String,
    description: Option<String>,
    worker_type: String,
    config_json: Option<String>,
    lambda_function_arn: Option<String>,
    is_active: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/worker-definitions", get(list_defs))
        .route(
            "/worker-definitions/new",
            get(new_def_page).post(create_def_submit),
        )
        .route("/worker-definitions/{id}", get(def_detail_page))
        .route(
            "/worker-definitions/{id}/edit",
            get(edit_def_page).post(edit_def_submit),
        )
        .route("/worker-definitions/{id}/delete", post(delete_def))
}

async fn list_defs(_claims: WebClaims, State(state): State<AppState>) -> impl IntoResponse {
    let defs = crate::db::workers::list_worker_definitions(&state.db)
        .await
        .unwrap_or_default();
    WorkerDefListTemplate { defs }
}

async fn new_def_page(_claims: WebClaims) -> impl IntoResponse {
    WorkerDefFormTemplate {
        is_edit: false,
        def_id: None,
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
        is_active: true,
    }
}

async fn create_def_submit(
    _claims: WebClaims,
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
        is_active: form.is_active.as_deref() == Some("on"),
    };

    let _ = crate::db::workers::create_worker_definition(&state.db, &new_def)
        .await
        .unwrap();
    Redirect::to("/worker-definitions").into_response()
}

async fn def_detail_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let def = crate::db::workers::get_worker_definition(&state.db, &id)
        .await
        .unwrap()
        .unwrap();
    WorkerDefDetailTemplate { def }
}

async fn edit_def_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let def = crate::db::workers::get_worker_definition(&state.db, &id)
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
        def_id: Some(def.id),
        name: def.name,
        description: def.description.unwrap_or_default(),
        worker_type: def.worker_type.to_string(),
        config_json,
        lambda_function_arn,
        is_active: def.is_active,
    }
}

async fn edit_def_submit(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<WorkerDefFormData>,
) -> impl IntoResponse {
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
        is_active: Some(form.is_active.as_deref() == Some("on")),
    };

    let _ = crate::db::workers::update_worker_definition(&state.db, &id, &update)
        .await
        .unwrap();
    Redirect::to(&format!("/worker-definitions/{}", id)).into_response()
}

async fn delete_def(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    crate::db::workers::delete_worker_definition(&state.db, &id)
        .await
        .unwrap();
    Redirect::to("/worker-definitions").into_response()
}
