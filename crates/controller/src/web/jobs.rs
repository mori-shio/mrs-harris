use axum::{
    extract::{State, Path, Query},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use askama::Template;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use sqlx::{MySqlPool, Row, Connection};
use std::str::FromStr;
use std::collections::HashMap;
use toml;

use mrs_harris_common::models::job::{Job, JobType, WorkerType, NewJob, JobUpdate, JobFilter, ShellPayload, RetryPolicy, BackoffStrategy};
use mrs_harris_common::models::run::{JobRun, RunStatus, TriggerType};
use mrs_harris_common::models::dag::DagTaskDefinition;

use super::auth::WebClaims;
use crate::app::AppState;

#[derive(Clone)]
pub struct JobRenderItem {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub job_type_ja: &'static str,
    pub worker_type: String,
    pub schedule_expr: Option<String>,
    pub is_active: bool,
    pub tags: Vec<String>,
}

#[derive(Clone)]
pub struct JobRunRenderItem {
    pub id: Uuid,
    pub id_short: String,
    pub status_str: String,
    pub status_ja: &'static str,
    pub worker_type: String,
    pub trigger_ja: &'static str,
    pub duration_str: String,
    pub started_at_str: String,
}

#[derive(Clone)]
pub struct JobSpaceTab {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

#[derive(Template)]
#[template(path = "jobs/list.html")]
struct JobsListTemplate {
    jobs: Vec<JobRenderItem>,
    spaces: Vec<JobSpaceTab>,
    current_space_id: String,
    current_search: String,
    current_job_type: String,
    current_is_active: String,
}
crate::impl_into_response!(JobsListTemplate);

#[derive(Template)]
#[template(path = "jobs/list_partial.html")]
struct JobsListPartialTemplate {
    jobs: Vec<JobRenderItem>,
}
crate::impl_into_response!(JobsListPartialTemplate);

#[derive(Clone, serde::Serialize)]
struct JobHistoryRenderItem {
    version: u32,
    changed_by: String,
    changed_at_str: String,
    payload_json: String,
}

#[derive(Template)]
#[template(path = "jobs/detail.html")]
struct JobDetailTemplate {
    job: Job,
    job_type_str: &'static str,
    worker_type_str: &'static str,
    worker_definition_name: String,
    timeout_minutes: u32,
    retry_backoff_str: &'static str,
    recent_runs: Vec<JobRunRenderItem>,
    is_dag: bool,
    command_preview: String,
    env_vars: Option<String>,
    env_vars_list: Vec<(String, String)>,
    ssm_region: String,
    ssm_path: String,
    ssm_recursive: bool,
    working_dir: Option<String>,
    slack_on_running: bool,
    slack_on_succeeded: bool,
    slack_on_failed: bool,
    created_at_str: String,
    updated_at_str: String,
    dag_tasks_json: String,
    dag_edges_json: String,
    history: Vec<JobHistoryRenderItem>,
}
crate::impl_into_response!(JobDetailTemplate);

#[derive(Template)]
#[template(path = "jobs/form.html")]
struct JobFormTemplate {
    is_edit: bool,
    job_id: Option<Uuid>,
    name: String,
    description: String,
    job_type: String,
    worker_definition_id: String,
    worker_defs: Vec<mrs_harris_common::models::worker::WorkerDefinition>,
    spaces: Vec<mrs_harris_common::models::space::Space>,
    space_id: Option<Uuid>,
    schedule_expr: String,
    script: String,
    env: String,
    ssm_region: String,
    ssm_path: String,
    ssm_recursive: bool,
    dag_tasks_json: String,
    timeout_sec: u32,
    has_retry: bool,
    max_retries: u32,
    backoff: String,
    base_delay_sec: u64,
    tags_str: String,
    is_active: bool,
    slack_on_running: bool,
    slack_on_succeeded: bool,
    slack_on_failed: bool,
}
crate::impl_into_response!(JobFormTemplate);


#[derive(serde::Deserialize, Debug)]
pub struct JobFormData {
    name: String,
    description: Option<String>,
    job_type: String,
    worker_definition_id: String,
    space_id: Option<String>,
    schedule_expr: Option<String>,
    
    // Command payload (Non-DAG)
    script: Option<String>,
    env: Option<String>,
    
    // SSM Parameter Store Settings
    ssm_region: Option<String>,
    ssm_path: Option<String>,
    ssm_recursive: Option<String>, // "on" if checked
    
    // DAG payload
    dag_tasks_json: Option<String>,
    
    // Advanced
    timeout_sec: u32,
    has_retry: Option<String>, // "on" if checked
    max_retries: u32,
    backoff: String,
    base_delay_sec: u64,
    tags_str: Option<String>,
    is_active: Option<String>, // "on" if checked
    slack_on_running: Option<String>,
    slack_on_succeeded: Option<String>,
    slack_on_failed: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/jobs", get(jobs_page))
        .route("/jobs/new", get(new_job_page).post(create_job_submit))
        .route("/jobs/{id}", get(job_detail_page))
        .route("/jobs/{id}/edit", get(edit_job_page).post(edit_job_submit))
}

pub fn map_job_to_render(job: &Job) -> JobRenderItem {
    let job_type_ja = match job.job_type {
        JobType::Cron => "Cron (定期)",
        JobType::Dag => "DAG (連結)",
        JobType::OneShot => "OneShot (単発)",
    };

    JobRenderItem {
        id: job.id,
        name: job.name.clone(),
        description: job.description.clone(),
        job_type_ja,
        worker_type: job.worker_type.to_string(),
        schedule_expr: job.schedule_expr.clone(),
        is_active: job.is_active,
        tags: job.tags.clone(),
    }
}

async fn jobs_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query_filter): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // Build filter from Query params
    let search = query_filter.get("search").filter(|s| !s.trim().is_empty()).cloned();
    let job_type = query_filter.get("job_type")
        .and_then(|t| JobType::from_str(t).ok());
    let is_active = query_filter.get("is_active")
        .and_then(|v| bool::from_str(v).ok());

    // Resolve space_id parameter by looking at either `space` or `space_id`
    let space_param = query_filter.get("space")
        .or(query_filter.get("space_id"))
        .filter(|s| !s.trim().is_empty())
        .cloned();

    let mut space_id = None;
    if let Some(sp) = space_param {
        if sp == "unclassified" || sp == "未分類" {
            space_id = Some("unclassified".to_string());
        } else if Uuid::parse_str(&sp).is_ok() {
            space_id = Some(sp);
        } else {
            // It's a space name. Query the DB for the space ID.
            let resolved_id: Option<String> = sqlx::query("SELECT id FROM spaces WHERE name = ?")
                .bind(&sp)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .map(|row| row.try_get("id").unwrap_or_default());
            if let Some(rid) = resolved_id {
                space_id = Some(rid);
            } else {
                // If space name is not found, default to showing no space matches
                space_id = Some(Uuid::nil().to_string());
            }
        }
    }

    let filter = JobFilter {
        job_type,
        is_active,
        tag: None,
        search: search.clone(),
        space_id: space_id.clone(),
        limit: None,
        offset: None,
    };

    let jobs_db = crate::db::jobs::list_jobs(&state.db, &filter).await.unwrap_or_default();
    let jobs = jobs_db.iter().map(map_job_to_render).collect::<Vec<_>>();

    let is_partial = headers.get("hx-target")
        .map(|v| v == "jobs-list-container")
        .unwrap_or(false);

    if is_partial {
        JobsListPartialTemplate { jobs }.into_response()
    } else {
        // Fetch spaces for the dynamic space tabs filter
        let spaces_rows = sqlx::query("SELECT id, name FROM spaces ORDER BY name ASC")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

        let mut spaces = Vec::new();
        let current_sid = space_id.clone().unwrap_or_default();

        // 1. "All" tab
        spaces.push(JobSpaceTab {
            id: String::new(),
            name: "すべて".to_string(),
            is_active: current_sid.is_empty(),
        });

        // 2. Dynamic space tabs
        for row in spaces_rows {
            let sid: String = row.try_get("id").unwrap_or_default();
            let name: String = row.try_get("name").unwrap_or_default();
            let active = current_sid == sid;
            spaces.push(JobSpaceTab {
                id: sid,
                name,
                is_active: active,
            });
        }

        // 3. "Unclassified" tab
        spaces.push(JobSpaceTab {
            id: "unclassified".to_string(),
            name: "未分類".to_string(),
            is_active: current_sid == "unclassified",
        });

        let current_search = query_filter.get("search").cloned().unwrap_or_default();
        let current_job_type = query_filter.get("job_type").cloned().unwrap_or_default();
        let current_is_active = query_filter.get("is_active").cloned().unwrap_or_default();

        JobsListTemplate {
            jobs,
            spaces,
            current_space_id: current_sid,
            current_search,
            current_job_type,
            current_is_active,
        }.into_response()
    }
}

async fn new_job_page(
    _claims: WebClaims,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let worker_defs = crate::db::workers::list_active_worker_definitions(&state.db).await.unwrap_or_default();

    // Query spaces from DB manually
    let spaces_rows = sqlx::query("SELECT id, name, description, created_at, updated_at FROM spaces ORDER BY name ASC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let mut spaces = Vec::new();
    for row in spaces_rows {
        let id_str: String = row.try_get("id").unwrap_or_default();
        let id = Uuid::parse_str(&id_str).unwrap_or_default();
        let name: String = row.try_get("name").unwrap_or_default();
        let description: Option<String> = row.try_get("description").ok();
        let created_at = row.try_get("created_at").unwrap_or_else(|_| Utc::now());
        let updated_at = row.try_get("updated_at").unwrap_or_else(|_| Utc::now());
        
        spaces.push(mrs_harris_common::models::space::Space {
            id,
            name,
            description,
            created_at,
            updated_at,
        });
    }

    JobFormTemplate {
        is_edit: false,
        job_id: None,
        name: String::new(),
        description: String::new(),
        job_type: "one_shot".to_string(),
        worker_definition_id: String::new(),
        worker_defs,
        spaces,
        space_id: None,
        schedule_expr: String::new(),
        script: String::new(),
        env: String::new(),
        ssm_region: String::new(),
        ssm_path: String::new(),
        ssm_recursive: false,
        dag_tasks_json: String::new(),
        timeout_sec: 3600,
        has_retry: true,
        max_retries: 3,
        backoff: "exponential".to_string(),
        base_delay_sec: 10,
        tags_str: String::new(),
        is_active: true,
        slack_on_running: false,
        slack_on_succeeded: false,
        slack_on_failed: false,
    }
}

async fn create_job_submit(
    claims: WebClaims,
    State(state): State<AppState>,
    Form(form): Form<JobFormData>,
) -> impl IntoResponse {
    let job_type = JobType::from_str(&form.job_type).unwrap_or(JobType::OneShot);
    
    // ロードされた自作ワーカーノード定義から worker_type を決定
    let worker_def_id = Uuid::parse_str(&form.worker_definition_id).unwrap();
    let def = crate::db::workers::get_worker_definition(&state.db, &worker_def_id).await.unwrap().unwrap();
    let worker_type = def.worker_type;

    // Build Retry Policy
    let backoff = match form.backoff.as_str() {
        "fixed" => BackoffStrategy::Fixed,
        "linear" => BackoffStrategy::Linear,
        _ => BackoffStrategy::Exponential,
    };
    let has_retry = form.has_retry.as_deref() == Some("on");
    let retry_policy = RetryPolicy {
        max_retries: if has_retry { form.max_retries } else { 0 },
        backoff,
        base_delay_sec: form.base_delay_sec,
    };

    // Build Tags
    let tags = form.tags_str
        .unwrap_or_default()
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>();

    let is_active = form.is_active.as_deref() == Some("on");

    // Build Payload
    let payload = if job_type == JobType::Dag {
        let task_defs: Vec<DagTaskDefinition> = serde_json::from_str(
            form.dag_tasks_json.as_deref().unwrap_or("[]")
        ).unwrap_or_default();
        
        let env_lines = form.env.as_deref().unwrap_or("");
        let mut env = HashMap::new();
        for line in env_lines.lines() {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                env.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
            }
        }

        let ssm_region = form.ssm_region.filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.filter(|p| !p.trim().is_empty());
        let ssm_recursive = if ssm_path.is_some() {
            Some(form.ssm_recursive.as_deref() == Some("on"))
        } else {
            None
        };

        serde_json::json!({
            "tasks_count": task_defs.len(),
            "env": env,
            "ssm_region": ssm_region,
            "ssm_path": ssm_path,
            "ssm_recursive": ssm_recursive
        })
    } else {
        let env_lines = form.env.as_deref().unwrap_or("");
        let mut env = HashMap::new();
        for line in env_lines.lines() {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                env.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
            }
        }

        let ssm_region = form.ssm_region.filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.filter(|p| !p.trim().is_empty());
        let ssm_recursive = if ssm_path.is_some() {
            Some(form.ssm_recursive.as_deref() == Some("on"))
        } else {
            None
        };

        let shell = ShellPayload {
            command: "sh".to_string(),
            args: vec!["-c".to_string(), form.script.unwrap_or_default()],
            working_dir: None,
            env,
            ssm_region,
            ssm_path,
            ssm_recursive,
        };
        serde_json::to_value(&shell).unwrap_or(serde_json::Value::Null)
    };

    let space_id = form.space_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Uuid::parse_str(&s).ok());

    let new_job = NewJob {
        name: form.name,
        description: form.description.filter(|d| !d.trim().is_empty()),
        job_type,
        payload,
        schedule_expr: form.schedule_expr.filter(|s| !s.trim().is_empty()),
        worker_type,
        retry_policy,
        timeout_sec: form.timeout_sec,
        is_active,
        tags,
        worker_definition_id: Some(worker_def_id),
        space_id,
    };

    // Save job within a Transaction if it's a DAG to save tasks/edges
    let pool = &state.db;
    let mut tx = pool.begin().await.unwrap();

    let job_id = Uuid::new_v4();
    let job_type_str = new_job.job_type.to_string();
    let worker_type_str = new_job.worker_type.to_string();
    let retry_policy_json = serde_json::to_value(&new_job.retry_policy).unwrap();
    let tags_json = serde_json::to_value(&new_job.tags).unwrap();
    let is_active_val: i8 = if new_job.is_active { 1 } else { 0 };

    sqlx::query(
        r#"INSERT INTO jobs (id, name, description, job_type, payload, schedule_expr, worker_type, retry_policy, timeout_sec, is_active, tags, worker_definition_id, space_id, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    )
    .bind(job_id.to_string())
    .bind(&new_job.name)
    .bind(&new_job.description)
    .bind(job_type_str)
    .bind(&new_job.payload)
    .bind(&new_job.schedule_expr)
    .bind(worker_type_str)
    .bind(retry_policy_json)
    .bind(new_job.timeout_sec)
    .bind(is_active_val)
    .bind(tags_json)
    .bind(worker_def_id.to_string())
    .bind(new_job.space_id.map(|uid| uid.to_string()))
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&mut *tx)
    .await
    .unwrap();

    if new_job.job_type == JobType::Dag {
        let task_defs: Vec<DagTaskDefinition> = serde_json::from_str(
            form.dag_tasks_json.as_deref().unwrap_or("[]")
        ).unwrap_or_default();

        for task in task_defs {
            let task_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO dag_tasks (id, dag_id, task_name, payload, worker_type, retry_policy, timeout_sec)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#
            )
            .bind(task_id.to_string())
            .bind(job_id.to_string())
            .bind(&task.name)
            .bind(&task.payload)
            .bind(task.worker_type.to_string())
            .bind(task.retry_policy.as_ref().map(|rp| serde_json::to_value(rp).unwrap()))
            .bind(task.timeout_sec)
            .execute(&mut *tx)
            .await
            .unwrap();

            for dep in task.depends_on {
                let edge_id = Uuid::new_v4();
                sqlx::query(
                    r#"INSERT INTO dag_edges (id, dag_id, from_task, to_task)
                       VALUES (?, ?, ?, ?)"#
                )
                .bind(edge_id.to_string())
                .bind(job_id.to_string())
                .bind(&dep)
                .bind(&task.name)
                .execute(&mut *tx)
                .await
                .unwrap();
            }
        }
    }

    // Slack通知設定の保存
    let slack_running = form.slack_on_running.as_deref() == Some("on");
    let slack_succeeded = form.slack_on_succeeded.as_deref() == Some("on");
    let slack_failed = form.slack_on_failed.as_deref() == Some("on");

    if slack_running || slack_succeeded || slack_failed {
        let mut events = Vec::new();
        if slack_running {
            events.push("running");
        }
        if slack_succeeded {
            events.push("succeeded");
        }
        if slack_failed {
            events.push("failed");
            events.push("dead_letter");
        }
        let events_json = serde_json::to_value(&events).unwrap();

        // デフォルトSlackチャネルの存在確認と作成
        sqlx::query(
            r#"INSERT INTO notification_channels (id, name, channel_type, config, is_active)
               VALUES ('c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', 'default-slack', 'slack', '{"webhook_url":""}', 1)
               ON DUPLICATE KEY UPDATE is_active=is_active"#
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO job_notifications (job_id, channel_id, on_events)
               VALUES (?, 'c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', ?)"#
        )
        .bind(job_id.to_string())
        .bind(events_json)
        .execute(&mut *tx)
        .await
        .unwrap();
    }

    tx.commit().await.unwrap();

    let _ = record_job_history(&state.db, &job_id, &claims.0.username).await;

    Redirect::to("/jobs").into_response()
}

async fn job_detail_page(
    claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let job = crate::db::jobs::get_job(&state.db, &id).await.unwrap().unwrap();
    
    let job_type_str = match job.job_type {
        JobType::Cron => "Cron (定期)",
        JobType::Dag => "DAG (連結)",
        JobType::OneShot => "OneShot (単発)",
    };

    let worker_type_str = match job.worker_type {
        WorkerType::Fargate => "fargate",
        WorkerType::Lambda => "lambda",
    };

    let retry_backoff_str = match job.retry_policy.backoff {
        BackoffStrategy::Fixed => "固定時間",
        BackoffStrategy::Linear => "線形",
        BackoffStrategy::Exponential => "指数",
    };

    // Load recent job runs
    let runs_db = crate::db::runs::list_runs(&state.db, Some(&id), Some(10), None).await.unwrap_or_default();
    let mut recent_runs = Vec::new();
    for r in runs_db {
        let status_ja = match r.status {
            RunStatus::Pending => "保留中",
            RunStatus::Scheduled => "予約済",
            RunStatus::Queued => "キュー済",
            RunStatus::Running => "実行中",
            RunStatus::Succeeded => "成功",
            RunStatus::Failed => "失敗",
            RunStatus::Retrying => "リトライ中",
            RunStatus::Cancelled => "キャンセル済",
            RunStatus::DeadLetter => "致命的エラー (DLQ)",
        };

        let trigger_ja = match r.trigger_type {
            TriggerType::Scheduled => "自動スケジュール",
            TriggerType::Manual => "手動実行",
            TriggerType::Dependency => "DAG依存",
        };

        let duration_str = match r.duration_ms {
            Some(ms) => {
                if ms >= 1000 {
                    format!("{:.1}s", ms as f64 / 1000.0)
                } else {
                    format!("{}ms", ms)
                }
            }
            None => "-".to_string(),
        };

        let started_at_str = match r.started_at {
            Some(dt) => dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "-".to_string(),
        };

        recent_runs.push(JobRunRenderItem {
            id: r.id,
            id_short: format!("{}...", &r.id.to_string()[..8]),
            status_str: r.status.to_string(),
            status_ja,
            worker_type: r.worker_type.to_string(),
            trigger_ja,
            duration_str,
            started_at_str,
        });
    }

    let is_dag = job.job_type == JobType::Dag;
    let mut command_preview = String::new();
    let mut env_vars = None;
    let mut env_vars_list = Vec::new();
    let mut ssm_region = String::new();
    let mut ssm_path = String::new();
    let mut ssm_recursive = false;
    let mut working_dir: Option<String> = None;
    let mut dag_tasks_json = "[]".to_string();
    let mut dag_edges_json = "[]".to_string();

    // 1. ワーカー定義名の取得
    let mut worker_definition_name = "デフォルト (起動タイプ設定)".to_string();
    if let Some(def_id) = job.worker_definition_id {
        if let Ok(Some(def)) = crate::db::workers::get_worker_definition(&state.db, &def_id).await {
            worker_definition_name = def.name;
        }
    }

    // 2. Slack通知設定の取得
    let noti_row = sqlx::query(
        "SELECT on_events FROM job_notifications WHERE job_id = ? AND channel_id = 'c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'"
    )
    .bind(job.id.to_string())
    .fetch_optional(&state.db)
    .await
    .unwrap_or_default();

    let mut slack_on_running = false;
    let mut slack_on_succeeded = false;
    let mut slack_on_failed = false;

    if let Some(row) = noti_row {
        if let Ok(on_events_val) = row.try_get::<serde_json::Value, _>("on_events") {
            if let Ok(on_events) = serde_json::from_value::<Vec<String>>(on_events_val) {
                slack_on_running = on_events.contains(&"running".to_string());
                slack_on_succeeded = on_events.contains(&"succeeded".to_string());
                slack_on_failed = on_events.contains(&"failed".to_string()) || on_events.contains(&"dead_letter".to_string());
            }
        }
    }

    // 3. 環境変数・SSM設定・DAG設定の取得
    if is_dag {
        // Load DAG tasks and edges from DB
        let tasks_rows = sqlx::query(
            "SELECT task_name, worker_type, payload FROM dag_tasks WHERE dag_id = ?"
        )
        .bind(id.to_string())
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let mut tasks = Vec::new();
        for row in tasks_rows {
            let name: String = row.try_get("task_name").unwrap();
            let wt: String = row.try_get("worker_type").unwrap();
            let pl: serde_json::Value = row.try_get("payload").unwrap();
            
            tasks.push(serde_json::json!({
                "name": name,
                "worker_type": wt,
                "payload": pl
            }));
        }
        dag_tasks_json = serde_json::to_string(&tasks).unwrap_or_else(|_| "[]".to_string());

        let edges_rows = sqlx::query(
            "SELECT from_task, to_task FROM dag_edges WHERE dag_id = ?"
        )
        .bind(id.to_string())
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let mut edges = Vec::new();
        for row in edges_rows {
            let from: String = row.try_get("from_task").unwrap();
            let to: String = row.try_get("to_task").unwrap();
            edges.push(serde_json::json!({
                "from": from,
                "to": to
            }));
        }
        dag_edges_json = serde_json::to_string(&edges).unwrap_or_else(|_| "[]".to_string());

        // DAGでも環境変数/SSM設定を復元
        if let Some(env_map) = job.payload.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env_map {
                env_vars_list.push((k.clone(), v.as_str().unwrap_or_default().to_string()));
            }
        }
        ssm_region = job.payload.get("ssm_region").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        ssm_path = job.payload.get("ssm_path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        ssm_recursive = job.payload.get("ssm_recursive").and_then(|v| v.as_bool()).unwrap_or(false);
    } else {
        // Parse payload as shell command
        if let Ok(shell) = serde_json::from_value::<ShellPayload>(job.payload.clone()) {
            let mut cmd = shell.command;
            if !shell.args.is_empty() {
                cmd.push(' ');
                cmd.push_str(&shell.args.join(" "));
            }
            command_preview = cmd;

            if !shell.env.is_empty() {
                let envs = shell.env.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("\n");
                env_vars = Some(envs);

                for (k, v) in &shell.env {
                    env_vars_list.push((k.clone(), v.clone()));
                }
            }
            ssm_region = shell.ssm_region.unwrap_or_default();
            ssm_path = shell.ssm_path.unwrap_or_default();
            ssm_recursive = shell.ssm_recursive.unwrap_or(false);
            working_dir = shell.working_dir;
        }
    }

    env_vars_list.sort_by(|a, b| a.0.cmp(&b.0));

    let timeout_minutes = job.timeout_sec / 60;

    let created_at_str = job.created_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string();
    let updated_at_str = job.updated_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string();

    // 4. 設定変更履歴の取得（空なら自動で初期履歴を記録）
    let mut history_rows = sqlx::query(
        "SELECT version, changed_by, payload, changed_at FROM job_history WHERE job_id = ? ORDER BY version DESC"
    )
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if history_rows.is_empty() {
        let _ = record_job_history(&state.db, &id, &claims.0.username).await;
        history_rows = sqlx::query(
            "SELECT version, changed_by, payload, changed_at FROM job_history WHERE job_id = ? ORDER BY version DESC"
        )
        .bind(id.to_string())
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    }

    let mut history = Vec::new();
    for row in history_rows {
        let version: u32 = row.try_get("version").unwrap_or(1);
        let changed_by: String = row.try_get("changed_by").unwrap_or_else(|_| "admin".to_string());
        let changed_at: chrono::DateTime<chrono::Utc> = row.try_get("changed_at").unwrap();
        let changed_at_str = changed_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string();
        
        let payload_val: serde_json::Value = row.try_get("payload").unwrap_or(serde_json::Value::Null);
        let payload_json = serde_json::to_string(&payload_val).unwrap_or_default();

        history.push(JobHistoryRenderItem {
            version,
            changed_by,
            changed_at_str,
            payload_json,
        });
    }

    JobDetailTemplate {
        job,
        job_type_str,
        worker_type_str,
        worker_definition_name,
        timeout_minutes,
        retry_backoff_str,
        recent_runs,
        is_dag,
        command_preview,
        env_vars,
        env_vars_list,
        ssm_region,
        ssm_path,
        ssm_recursive,
        working_dir,
        slack_on_running,
        slack_on_succeeded,
        slack_on_failed,
        created_at_str,
        updated_at_str,
        dag_tasks_json,
        dag_edges_json,
        history,
    }
}

async fn edit_job_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let job = crate::db::jobs::get_job(&state.db, &id).await.unwrap().unwrap();

    let mut script = String::new();
    let mut env = String::new();
    let mut ssm_region = String::new();
    let mut ssm_path = String::new();
    let mut ssm_recursive = false;
    let mut dag_tasks_json = String::new();

    if job.job_type == JobType::Dag {
        // Load DAG tasks and dependencies to reconstruct the JSON
        let tasks_rows = sqlx::query(
            "SELECT task_name, worker_type, payload, retry_policy, timeout_sec FROM dag_tasks WHERE dag_id = ?"
        )
        .bind(id.to_string())
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let mut defs = Vec::new();
        for row in tasks_rows {
            let name: String = row.try_get("task_name").unwrap();
            let wt: String = row.try_get("worker_type").unwrap();
            let pl: serde_json::Value = row.try_get("payload").unwrap();
            let rp_opt: Option<serde_json::Value> = row.try_get("retry_policy").ok();
            let to_opt: Option<u32> = row.try_get("timeout_sec").ok();

            // Load dependencies
            let dep_rows = sqlx::query(
                "SELECT from_task FROM dag_edges WHERE dag_id = ? AND to_task = ?"
            )
            .bind(id.to_string())
            .bind(&name)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

            let mut depends_on = Vec::new();
            for dep_row in dep_rows {
                let from: String = dep_row.try_get("from_task").unwrap();
                depends_on.push(from);
            }

            let rp = rp_opt.and_then(|v| serde_json::from_value::<RetryPolicy>(v).ok());

            defs.push(serde_json::json!({
                "name": name,
                "worker_type": wt,
                "payload": pl,
                "retry_policy": rp,
                "timeout_sec": to_opt,
                "depends_on": depends_on
            }));
        }

        dag_tasks_json = serde_json::to_string_pretty(&defs).unwrap_or_default();

        // DAGでも環境変数/SSM設定を復元
        if let Some(env_map) = job.payload.get("env").and_then(|v| v.as_object()) {
            env = env_map.iter().map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or_default())).collect::<Vec<_>>().join("\n");
        }
        ssm_region = job.payload.get("ssm_region").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        ssm_path = job.payload.get("ssm_path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        ssm_recursive = job.payload.get("ssm_recursive").and_then(|v| v.as_bool()).unwrap_or(false);
    } else {
        if let Ok(shell) = serde_json::from_value::<ShellPayload>(job.payload.clone()) {
            if (shell.command == "sh" || shell.command == "/bin/sh" || shell.command == "bash" || shell.command == "/bin/bash")
                && shell.args.len() == 2
                && shell.args[0] == "-c"
            {
                script = shell.args[1].clone();
            } else {
                let mut cmd = shell.command;
                if !shell.args.is_empty() {
                    cmd.push(' ');
                    cmd.push_str(&shell.args.join(" "));
                }
                script = cmd;
            }
            env = shell.env.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("\n");
            ssm_region = shell.ssm_region.unwrap_or_default();
            ssm_path = shell.ssm_path.unwrap_or_default();
            ssm_recursive = shell.ssm_recursive.unwrap_or(false);
        }
    }

    let tags_str = job.tags.join(", ");
    let has_retry = job.retry_policy.max_retries > 0;

    let worker_defs = crate::db::workers::list_active_worker_definitions(&state.db).await.unwrap_or_default();

    // Slack通知設定の復元取得
    let noti_row = sqlx::query(
        "SELECT on_events FROM job_notifications WHERE job_id = ? AND channel_id = 'c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'"
    )
    .bind(job.id.to_string())
    .fetch_optional(&state.db)
    .await
    .unwrap_or_default();

    let mut slack_on_running = false;
    let mut slack_on_succeeded = false;
    let mut slack_on_failed = false;

    if let Some(row) = noti_row {
        if let Ok(on_events_val) = row.try_get::<serde_json::Value, _>("on_events") {
            if let Ok(on_events) = serde_json::from_value::<Vec<String>>(on_events_val) {
                slack_on_running = on_events.contains(&"running".to_string());
                slack_on_succeeded = on_events.contains(&"succeeded".to_string());
                slack_on_failed = on_events.contains(&"failed".to_string()) || on_events.contains(&"dead_letter".to_string());
            }
        }
    }

    // Query spaces from DB manually
    let spaces_rows = sqlx::query("SELECT id, name, description, created_at, updated_at FROM spaces ORDER BY name ASC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let mut spaces = Vec::new();
    for row in spaces_rows {
        let id_str: String = row.try_get("id").unwrap_or_default();
        let id = Uuid::parse_str(&id_str).unwrap_or_default();
        let name: String = row.try_get("name").unwrap_or_default();
        let description: Option<String> = row.try_get("description").ok();
        let created_at = row.try_get("created_at").unwrap_or_else(|_| Utc::now());
        let updated_at = row.try_get("updated_at").unwrap_or_else(|_| Utc::now());
        
        spaces.push(mrs_harris_common::models::space::Space {
            id,
            name,
            description,
            created_at,
            updated_at,
        });
    }

    JobFormTemplate {
        is_edit: true,
        job_id: Some(job.id),
        name: job.name,
        description: job.description.unwrap_or_default(),
        job_type: job.job_type.to_string(),
        worker_definition_id: job.worker_definition_id.map(|uid| uid.to_string()).unwrap_or_default(),
        worker_defs,
        spaces,
        space_id: job.space_id,
        schedule_expr: job.schedule_expr.unwrap_or_default(),
        script,
        env,
        ssm_region,
        ssm_path,
        ssm_recursive,
        dag_tasks_json,
        timeout_sec: job.timeout_sec,
        has_retry,
        max_retries: job.retry_policy.max_retries,
        backoff: job.retry_policy.backoff.to_string(),
        base_delay_sec: job.retry_policy.base_delay_sec,
        tags_str,
        is_active: job.is_active,
        slack_on_running,
        slack_on_succeeded,
        slack_on_failed,
    }
}

async fn edit_job_submit(
    claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<JobFormData>,
) -> impl IntoResponse {
    let job_type = JobType::from_str(&form.job_type).unwrap_or(JobType::OneShot);
    
    // ロードされた自作ワーカーノード定義から worker_type を決定
    let worker_def_id = Uuid::parse_str(&form.worker_definition_id).unwrap();
    let def = crate::db::workers::get_worker_definition(&state.db, &worker_def_id).await.unwrap().unwrap();
    let worker_type = def.worker_type;

    let backoff = match form.backoff.as_str() {
        "fixed" => BackoffStrategy::Fixed,
        "linear" => BackoffStrategy::Linear,
        _ => BackoffStrategy::Exponential,
    };
    let has_retry = form.has_retry.as_deref() == Some("on");
    let retry_policy = RetryPolicy {
        max_retries: if has_retry { form.max_retries } else { 0 },
        backoff,
        base_delay_sec: form.base_delay_sec,
    };

    let tags = form.tags_str
        .unwrap_or_default()
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>();

    let is_active = form.is_active.as_deref() == Some("on");

    let payload = if job_type == JobType::Dag {
        let task_defs: Vec<DagTaskDefinition> = serde_json::from_str(
            form.dag_tasks_json.as_deref().unwrap_or("[]")
        ).unwrap_or_default();
        
        let env_lines = form.env.as_deref().unwrap_or("");
        let mut env = HashMap::new();
        for line in env_lines.lines() {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                env.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
            }
        }

        let ssm_region = form.ssm_region.filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.filter(|p| !p.trim().is_empty());
        let ssm_recursive = if ssm_path.is_some() {
            Some(form.ssm_recursive.as_deref() == Some("on"))
        } else {
            None
        };

        serde_json::json!({
            "tasks_count": task_defs.len(),
            "env": env,
            "ssm_region": ssm_region,
            "ssm_path": ssm_path,
            "ssm_recursive": ssm_recursive
        })
    } else {
        let env_lines = form.env.as_deref().unwrap_or("");
        let mut env = HashMap::new();
        for line in env_lines.lines() {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                env.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
            }
        }

        let ssm_region = form.ssm_region.filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.filter(|p| !p.trim().is_empty());
        let ssm_recursive = if ssm_path.is_some() {
            Some(form.ssm_recursive.as_deref() == Some("on"))
        } else {
            None
        };

        let shell = ShellPayload {
            command: "sh".to_string(),
            args: vec!["-c".to_string(), form.script.unwrap_or_default()],
            working_dir: None,
            env,
            ssm_region,
            ssm_path,
            ssm_recursive,
        };
        serde_json::to_value(&shell).unwrap_or(serde_json::Value::Null)
    };

    let space_id = form.space_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Uuid::parse_str(&s).ok());

    // Save changes in a transaction to handle DAG updates cleanly
    let pool = &state.db;
    let mut tx = pool.begin().await.unwrap();

    let retry_policy_json = serde_json::to_value(&retry_policy).unwrap();
    let tags_json = serde_json::to_value(&tags).unwrap();
    let is_active_val: i8 = if is_active { 1 } else { 0 };

    sqlx::query(
        r#"UPDATE jobs 
           SET name = ?, description = ?, job_type = ?, payload = ?, schedule_expr = ?, worker_type = ?, retry_policy = ?, timeout_sec = ?, is_active = ?, tags = ?, worker_definition_id = ?, space_id = ?, updated_at = ?
           WHERE id = ?"#
    )
    .bind(&form.name)
    .bind(&form.description.filter(|d| !d.trim().is_empty()))
    .bind(job_type.to_string())
    .bind(&payload)
    .bind(&form.schedule_expr.filter(|s| !s.trim().is_empty()))
    .bind(worker_type.to_string())
    .bind(retry_policy_json)
    .bind(form.timeout_sec)
    .bind(is_active_val)
    .bind(tags_json)
    .bind(worker_def_id.to_string())
    .bind(space_id.map(|uid| uid.to_string()))
    .bind(Utc::now())
    .bind(id.to_string())
    .execute(&mut *tx)
    .await
    .unwrap();

    if job_type == JobType::Dag {
        // Clear old tasks/edges and write new ones
        sqlx::query("DELETE FROM dag_tasks WHERE dag_id = ?").bind(id.to_string()).execute(&mut *tx).await.unwrap();
        sqlx::query("DELETE FROM dag_edges WHERE dag_id = ?").bind(id.to_string()).execute(&mut *tx).await.unwrap();

        let task_defs: Vec<DagTaskDefinition> = serde_json::from_str(
            form.dag_tasks_json.as_deref().unwrap_or("[]")
        ).unwrap_or_default();

        for task in task_defs {
            let task_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO dag_tasks (id, dag_id, task_name, payload, worker_type, retry_policy, timeout_sec)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#
            )
            .bind(task_id.to_string())
            .bind(id.to_string())
            .bind(&task.name)
            .bind(&task.payload)
            .bind(task.worker_type.to_string())
            .bind(task.retry_policy.as_ref().map(|rp| serde_json::to_value(rp).unwrap()))
            .bind(task.timeout_sec)
            .execute(&mut *tx)
            .await
            .unwrap();

            for dep in task.depends_on {
                let edge_id = Uuid::new_v4();
                sqlx::query(
                    r#"INSERT INTO dag_edges (id, dag_id, from_task, to_task)
                       VALUES (?, ?, ?, ?)"#
                )
                .bind(edge_id.to_string())
                .bind(id.to_string())
                .bind(&dep)
                .bind(&task.name)
                .execute(&mut *tx)
                .await
                .unwrap();
            }
        }
    }

    // Slack通知設定の保存
    let slack_running = form.slack_on_running.as_deref() == Some("on");
    let slack_succeeded = form.slack_on_succeeded.as_deref() == Some("on");
    let slack_failed = form.slack_on_failed.as_deref() == Some("on");

    // First delete any existing notifications link for default-slack
    sqlx::query("DELETE FROM job_notifications WHERE job_id = ? AND channel_id = 'c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'")
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();

    if slack_running || slack_succeeded || slack_failed {
        let mut events = Vec::new();
        if slack_running {
            events.push("running");
        }
        if slack_succeeded {
            events.push("succeeded");
        }
        if slack_failed {
            events.push("failed");
            events.push("dead_letter");
        }
        let events_json = serde_json::to_value(&events).unwrap();

        // デフォルトSlackチャネルの存在確認と作成
        sqlx::query(
            r#"INSERT INTO notification_channels (id, name, channel_type, config, is_active)
               VALUES ('c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', 'default-slack', 'slack', '{"webhook_url":""}', 1)
               ON DUPLICATE KEY UPDATE is_active=is_active"#
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO job_notifications (job_id, channel_id, on_events)
               VALUES (?, 'c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', ?)"#
        )
        .bind(id.to_string())
        .bind(events_json)
        .execute(&mut *tx)
        .await
        .unwrap();
    }

    tx.commit().await.unwrap();

    let _ = record_job_history(&state.db, &id, &claims.0.username).await;

    Redirect::to(&format!("/jobs/{}", id)).into_response()
}



async fn build_job_snapshot(
    pool: &MySqlPool,
    job: &Job,
) -> serde_json::Value {
    let job_type_str = match job.job_type {
        JobType::Cron => "Cron (定期)",
        JobType::Dag => "DAG (連結)",
        JobType::OneShot => "OneShot (単発)",
    };

    let worker_definition_name = if let Some(def_id) = job.worker_definition_id {
        if let Ok(Some(def)) = crate::db::workers::get_worker_definition(pool, &def_id).await {
            def.name
        } else {
            "デフォルト (起動タイプ設定)".to_string()
        }
    } else {
        "デフォルト (起動タイプ設定)".to_string()
    };

    let mut env_vars = HashMap::new();
    let mut script_or_dag = serde_json::Value::Null;
    let mut ssm_region = String::new();
    let mut ssm_path = String::new();
    let mut ssm_recursive = false;

    if job.job_type == JobType::Dag {
        // Load DAG tasks
        let tasks_rows = sqlx::query(
            "SELECT task_name, worker_type, payload FROM dag_tasks WHERE dag_id = ?"
        )
        .bind(job.id.to_string())
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut tasks = Vec::new();
        for row in tasks_rows {
            let name: String = row.try_get("task_name").unwrap();
            let wt: String = row.try_get("worker_type").unwrap();
            let pl: serde_json::Value = row.try_get("payload").unwrap();
            tasks.push(serde_json::json!({
                "タスク名": name,
                "起動タイプ": wt,
                "設定": pl
            }));
        }
        script_or_dag = serde_json::Value::Array(tasks);

        if let Some(env_map) = job.payload.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env_map {
                env_vars.insert(k.clone(), v.as_str().unwrap_or_default().to_string());
            }
        }
        ssm_region = job.payload.get("ssm_region").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        ssm_path = job.payload.get("ssm_path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        ssm_recursive = job.payload.get("ssm_recursive").and_then(|v| v.as_bool()).unwrap_or(false);
    } else {
        if let Ok(shell) = serde_json::from_value::<ShellPayload>(job.payload.clone()) {
            let mut cmd = shell.command;
            if !shell.args.is_empty() {
                cmd.push(' ');
                cmd.push_str(&shell.args.join(" "));
            }
            script_or_dag = serde_json::Value::String(cmd);

            for (k, v) in &shell.env {
                env_vars.insert(k.clone(), v.clone());
            }
            ssm_region = shell.ssm_region.unwrap_or_default();
            ssm_path = shell.ssm_path.unwrap_or_default();
            ssm_recursive = shell.ssm_recursive.unwrap_or(false);
        }
    }

    let retry_backoff_str = match job.retry_policy.backoff {
        BackoffStrategy::Fixed => "固定時間",
        BackoffStrategy::Linear => "線形",
        BackoffStrategy::Exponential => "指数",
    };

    // Slack settings
    let noti_row = sqlx::query(
        "SELECT on_events FROM job_notifications WHERE job_id = ? AND channel_id = 'c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'"
    )
    .bind(job.id.to_string())
    .fetch_optional(pool)
    .await
    .unwrap_or_default();

    let mut slack_running = false;
    let mut slack_succeeded = false;
    let mut slack_failed = false;

    if let Some(row) = noti_row {
        if let Ok(on_events_val) = row.try_get::<serde_json::Value, _>("on_events") {
            if let Ok(on_events) = serde_json::from_value::<Vec<String>>(on_events_val) {
                slack_running = on_events.contains(&"running".to_string());
                slack_succeeded = on_events.contains(&"succeeded".to_string());
                slack_failed = on_events.contains(&"failed".to_string()) || on_events.contains(&"dead_letter".to_string());
            }
        }
    }

    serde_json::json!({
        "ジョブ名": job.name,
        "説明": job.description.as_deref().unwrap_or(""),
        "ジョブタイプ": job_type_str,
        "スケジュール (Cron)": job.schedule_expr.as_deref().unwrap_or("未設定"),
        "有効化状態": if job.is_active { "有効" } else { "無効" },
        "ワーカー定義": worker_definition_name,
        "タイムアウト": format!("{} 秒", job.timeout_sec),
        "リトライ上限": job.retry_policy.max_retries,
        "バックオフ戦略": retry_backoff_str,
        "初期遅延": format!("{} 秒", job.retry_policy.base_delay_sec),
        "タグ": job.tags,
        "直接設定の環境変数": env_vars,
        "SSMパラメータ連携": {
            "リージョン": ssm_region,
            "パス": ssm_path,
            "再帰取得": if ssm_recursive { "有効" } else { "無効" }
        },
        "Slack通知": {
            "ジョブ起動時": if slack_running { "有効" } else { "無効" },
            "成功時": if slack_succeeded { "有効" } else { "無効" },
            "失敗時": if slack_failed { "有効" } else { "無効" }
        },
        "スクリプト / DAG構成": script_or_dag
    })
}

async fn record_job_history(
    pool: &MySqlPool,
    job_id: &Uuid,
    changed_by: &str,
) -> anyhow::Result<()> {
    // 1. 最新のジョブ情報を取得
    let job = match crate::db::jobs::get_job(pool, job_id).await? {
        Some(j) => j,
        None => return Ok(()),
    };

    // 2. スナップショットJSONの作成
    let snapshot = build_job_snapshot(pool, &job).await;

    // 3. バージョンの決定
    let version_row = sqlx::query(
        "SELECT MAX(version) as max_v FROM job_history WHERE job_id = ?"
    )
    .bind(job_id.to_string())
    .fetch_one(pool)
    .await?;
    
    let current_max: Option<u32> = version_row.try_get("max_v").ok();
    let next_version = current_max.unwrap_or(0) + 1;

    // 4. 履歴に挿入
    let history_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO job_history (id, job_id, version, payload, changed_by, changed_at)
           VALUES (?, ?, ?, ?, ?, ?)"#
    )
    .bind(history_id.to_string())
    .bind(job_id.to_string())
    .bind(next_version)
    .bind(snapshot)
    .bind(changed_by)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(())
}
