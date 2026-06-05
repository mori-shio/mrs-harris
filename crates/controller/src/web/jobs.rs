use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use chrono::Utc;
use sqlx::{MySqlPool, Row};
use std::collections::HashMap;
use std::str::FromStr;

use mrs_harris_common::models::dag::DagTaskDefinition;
use mrs_harris_common::models::job::{
    BackoffStrategy, Job, JobFilter, JobType, NewJob, RetryPolicy, ShellPayload,
};
use mrs_harris_common::models::run::JobRun;

use super::auth::WebClaims;
use crate::app::AppState;

#[derive(Clone)]
pub struct JobRenderItem {
    pub name: String,
    pub job_type_label: &'static str,
    pub worker_name: String,
    pub schedule_display: String,
    pub is_active: bool,
    pub tags: Vec<String>,
}

#[derive(Clone)]
pub struct JobRunRenderItem {
    pub status_badge_class: &'static str,
    pub status_ja: &'static str,
    pub run_number: i64,
    pub trigger_ja: &'static str,
    pub duration_str: String,
    pub started_at_str: String,
    pub config_version_str: String,
}

fn job_run_render_item_from_run(run: &JobRun) -> JobRunRenderItem {
    let status_ja = run.status.label_ja();
    let trigger_ja = run.trigger_type.label_ja();

    let duration_str = match run.duration_ms {
        Some(ms) if ms >= 1000 => format!("{:.1}s", ms as f64 / 1000.0),
        Some(ms) => format!("{}ms", ms),
        None => "-".to_string(),
    };

    let started_at_str = match run.started_at {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
    };

    let config_version_str = match run.config_version {
        Some(v) => format!("v{}", v),
        None => "-".to_string(),
    };

    JobRunRenderItem {
        status_badge_class: run.status.badge_class(),
        status_ja,
        run_number: run.run_number,
        trigger_ja,
        duration_str,
        started_at_str,
        config_version_str,
    }
}

fn display_shell_command(shell: &ShellPayload) -> String {
    if shell.command == "sh" && shell.args.len() >= 2 && shell.args[0] == "-c" {
        return shell.args[1..].join(" ");
    }

    let mut cmd = shell.command.clone();
    if !shell.args.is_empty() {
        cmd.push(' ');
        cmd.push_str(&shell.args.join(" "));
    }
    cmd
}

fn job_type_label(job_type: &JobType) -> &'static str {
    match job_type {
        JobType::Cron => "Cron",
        JobType::OneShot => "OneShot",
    }
}

fn schedule_display(schedule_expr: Option<&str>) -> String {
    match schedule_expr.map(str::trim).filter(|expr| !expr.is_empty()) {
        Some(expr) => expr.to_string(),
        None => "-".to_string(),
    }
}

#[derive(Clone)]
pub struct JobSpaceTab {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct JobCopyCandidate {
    pub name: String,
    pub description: String,
}

#[derive(Template)]
#[template(path = "jobs/list.html")]
struct JobsListTemplate {
    jobs: Vec<JobRenderItem>,
    copy_candidates_json: String,
    spaces: Vec<JobSpaceTab>,
    current_space_name: String,
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

#[derive(serde::Deserialize)]
struct RunsQuery {
    page: Option<u32>,
    sort: Option<String>,
}

#[derive(Template)]
#[template(path = "jobs/runs_table.html")]
struct JobRunsTableTemplate {
    job_name: String,
    recent_runs: Vec<JobRunRenderItem>,
    total_runs: i64,
    current_page: u32,
    total_pages: u32,
    start_index: usize,
    end_index: usize,
    page_items: Vec<Option<u32>>,
    current_sort: String,
    empty_runs: Vec<()>,
}
crate::impl_into_response!(JobRunsTableTemplate);

#[derive(Template)]
#[template(path = "jobs/detail.html")]
struct JobDetailTemplate {
    job: Job,
    job_name: String,
    initial_tab: String,
    job_type_str: &'static str,
    worker_definition_name: String,
    timeout_minutes: u32,
    retry_backoff_str: &'static str,
    recent_runs: Vec<JobRunRenderItem>,
    total_runs: i64,
    current_page: u32,
    total_pages: u32,
    start_index: usize,
    end_index: usize,
    page_items: Vec<Option<u32>>,
    current_sort: String,
    empty_runs: Vec<()>,
    is_dag: bool,
    command_preview: String,
    env_vars_text: String,
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

    // History pagination
    latest_version: u32,
    history_current_page: u32,
    history_total_pages: u32,
    total_history: i64,
    history_start_index: usize,
    history_end_index: usize,
    history_page_items: Vec<Option<u32>>,
    empty_history: Vec<()>,
}
crate::impl_into_response!(JobDetailTemplate);

#[derive(Template)]
#[template(path = "jobs/form.html")]
struct JobFormTemplate {
    is_edit: bool,
    job_id: Option<i64>,
    original_name: Option<String>,
    name: String,
    description: String,
    tags_str: String,
    job_type: String,
    worker_definition_id: String,
    worker_defs: Vec<mrs_harris_common::models::worker::WorkerDefinition>,
    spaces: Vec<mrs_harris_common::models::space::Space>,
    space_id: Option<i64>,
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
    is_active: bool,
    slack_on_running: bool,
    slack_on_succeeded: bool,
    slack_on_failed: bool,
    error_message: Option<String>,
}
crate::impl_into_response!(JobFormTemplate);

#[derive(Debug, serde::Deserialize, Default)]
struct NewJobQuery {
    copy_from: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mrs_harris_common::models::job::{JobType, WorkerType};
    use mrs_harris_common::models::run::{RunStatus, TriggerType};

    fn sample_run(run_number: i64) -> JobRun {
        let now = Utc.with_ymd_and_hms(2026, 5, 28, 12, 0, 0).unwrap();

        JobRun {
            id: 10,
            job_id: 20,
            run_number,
            status: RunStatus::Succeeded,
            worker_type: WorkerType::Lambda,
            worker_id: None,
            trigger_type: TriggerType::Manual,
            attempt: 1,
            scheduled_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            next_retry_at: None,
            duration_ms: Some(1500),
            log_archive_status: None,
            log_archive_store: None,
            log_archive_key: None,
            log_line_count: None,
            log_archive_bytes: None,
            log_archived_at: None,
            output: None,
            error: None,
            job_history_id: Some(30),
            worker_definition_history_id: Some(40),
            config_version: Some(3),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn run_render_item_uses_persisted_run_number() {
        let item = job_run_render_item_from_run(&sample_run(42));

        assert_eq!(item.run_number, 42);
    }

    #[test]
    fn job_type_label_uses_english_only() {
        assert_eq!(job_type_label(&JobType::Cron), "Cron");
        assert_eq!(job_type_label(&JobType::OneShot), "OneShot");
    }

    #[test]
    fn schedule_display_uses_dash_when_empty() {
        assert_eq!(schedule_display(None), "-");
        assert_eq!(schedule_display(Some("")), "-");
        assert_eq!(schedule_display(Some("   ")), "-");
        assert_eq!(schedule_display(Some("0 0 * * *")), "0 0 * * *");
    }
}

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
        .route("/api/jobs/validate-name", post(api_validate_job_name))
        .route("/jobs/{id}", get(job_detail_page))
        .route("/jobs/{id}/runs", get(job_runs_list))
        .route("/jobs/{id}/edit", get(edit_job_page).post(edit_job_submit))
}

pub fn map_job_to_render(
    job: &Job,
    worker_name_map: &std::collections::HashMap<i64, String>,
) -> JobRenderItem {
    let worker_name = job
        .worker_definition_id
        .and_then(|id| worker_name_map.get(&id).cloned())
        .unwrap_or_else(|| "-".to_string());

    JobRenderItem {
        name: job.name.clone(),
        job_type_label: job_type_label(&job.job_type),
        worker_name,
        schedule_display: schedule_display(job.schedule_expr.as_deref()),
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
    let search = query_filter
        .get("search")
        .filter(|s| !s.trim().is_empty())
        .cloned();
    let job_type = query_filter
        .get("job_type")
        .and_then(|t| JobType::from_str(t).ok());
    let is_active = query_filter
        .get("is_active")
        .and_then(|v| bool::from_str(v).ok());

    // Resolve space_id parameter by looking at either `space` or `space_id`
    let space_param = query_filter
        .get("space")
        .or(query_filter.get("space_id"))
        .filter(|s| !s.trim().is_empty())
        .cloned();

    let mut space_id = None;
    let mut current_space_name = String::new();
    if let Some(sp) = space_param {
        if sp == "unclassified" || sp == "未分類" {
            space_id = Some("unclassified".to_string());
            current_space_name = "unclassified".to_string();
        } else if let Ok(parsed_id) = sp.parse::<i64>() {
            space_id = Some(sp.clone());
            // Look up space name by ID for current_space_name
            let resolved_name: Option<String> = sqlx::query("SELECT name FROM spaces WHERE id = ?")
                .bind(parsed_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .map(|row| row.try_get("name").unwrap_or_default());
            if let Some(rname) = resolved_name {
                current_space_name = rname;
            }
        } else {
            // It's a space name. Query the DB for the space ID.
            let resolved_id: Option<i64> = sqlx::query("SELECT id FROM spaces WHERE name = ?")
                .bind(&sp)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .map(|row| row.try_get("id").unwrap_or_default());
            if let Some(rid) = resolved_id {
                space_id = Some(rid.to_string());
                current_space_name = sp;
            } else {
                // If space name is not found, default to showing no space matches
                space_id = Some("-1".to_string());
                current_space_name = sp;
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

    let jobs_db = crate::db::jobs::list_jobs(&state.db, &filter)
        .await
        .unwrap_or_default();

    let worker_rows = sqlx::query("SELECT id, name FROM worker_definitions")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let mut worker_name_map: std::collections::HashMap<i64, String> =
        std::collections::HashMap::new();
    for row in worker_rows {
        let uid: i64 = row.try_get("id").unwrap_or_default();
        if true {
            let name: String = row.try_get("name").unwrap_or_default();
            worker_name_map.insert(uid, name);
        }
    }

    let jobs = jobs_db
        .iter()
        .map(|j| map_job_to_render(j, &worker_name_map))
        .collect::<Vec<_>>();
    let copy_candidates = jobs_db
        .iter()
        .map(|job| JobCopyCandidate {
            name: job.name.clone(),
            description: job.description.clone().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let copy_candidates_json =
        serde_json::to_string(&copy_candidates).unwrap_or_else(|_| "[]".to_string());

    let is_partial = headers
        .get("hx-target")
        .map(|v| v == "jobs-list-container")
        .unwrap_or(false);

    if is_partial {
        JobsListPartialTemplate { jobs }.into_response()
    } else {
        // Fetch spaces for the dynamic space tabs filter
        let spaces_rows = sqlx::query("SELECT id, name FROM spaces ORDER BY priority ASC, id ASC")
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
            let sid: i64 = row.try_get("id").unwrap_or_default();
            let sid_str = sid.to_string();
            let name: String = row.try_get("name").unwrap_or_default();
            let active = current_sid == sid_str;
            spaces.push(JobSpaceTab {
                id: sid_str,
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
            copy_candidates_json,
            spaces,
            current_space_name,
            current_search,
            current_job_type,
            current_is_active,
        }
        .into_response()
    }
}

async fn load_spaces(pool: &MySqlPool) -> Vec<mrs_harris_common::models::space::Space> {
    let spaces_rows = sqlx::query(
        "SELECT id, name, description, priority, created_at, updated_at FROM spaces ORDER BY priority ASC, id ASC",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut spaces = Vec::new();
    for row in spaces_rows {
        let id: i64 = row.try_get("id").unwrap_or_default();
        let name: String = row.try_get("name").unwrap_or_default();
        let description: Option<String> = row.try_get("description").ok();
        let priority: i32 = row.try_get("priority").unwrap_or_default();
        let created_at = row.try_get("created_at").unwrap_or_else(|_| Utc::now());
        let updated_at = row.try_get("updated_at").unwrap_or_else(|_| Utc::now());

        spaces.push(mrs_harris_common::models::space::Space {
            id,
            name,
            description,
            priority,
            created_at,
            updated_at,
        });
    }

    spaces
}

async fn build_job_form_template_from_job(
    pool: &MySqlPool,
    job: Option<&Job>,
    original_name: Option<String>,
    is_edit: bool,
    copied_from_existing: bool,
) -> JobFormTemplate {
    let worker_defs = crate::db::workers::list_active_worker_definitions(pool)
        .await
        .unwrap_or_default();
    let spaces = load_spaces(pool).await;

    if let Some(job) = job {
        let mut script = String::new();
        let mut env = String::new();
        let mut ssm_region = String::new();
        let mut ssm_path = String::new();
        let mut ssm_recursive = false;
        let mut dag_tasks_json = String::new();

        if false {
            let rows = sqlx::query(
                "SELECT id, name, worker_type, payload, retry_policy, timeout_sec FROM dag_tasks WHERE dag_id = ? ORDER BY id ASC",
            )
            .bind(job.id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            let mut defs = Vec::new();
            for row in rows {
                let id: i64 = row.try_get("id").unwrap();
                let name: String = row.try_get("name").unwrap();
                let wt: String = row.try_get("worker_type").unwrap();
                let pl: serde_json::Value = row.try_get("payload").unwrap();
                let rp_opt: Option<serde_json::Value> = row.try_get("retry_policy").ok();
                let to_opt: Option<u32> = row.try_get("timeout_sec").ok();

                let dep_rows =
                    sqlx::query("SELECT from_task FROM dag_edges WHERE dag_id = ? AND to_task = ?")
                        .bind(id)
                        .bind(&name)
                        .fetch_all(pool)
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

            if let Some(env_map) = job.payload.get("env").and_then(|v| v.as_object()) {
                env = env_map
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or_default()))
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            ssm_region = job
                .payload
                .get("ssm_region")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ssm_path = job
                .payload
                .get("ssm_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ssm_recursive = job
                .payload
                .get("ssm_recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        } else if let Ok(shell) = serde_json::from_value::<ShellPayload>(job.payload.clone()) {
            if (shell.command == "sh"
                || shell.command == "/bin/sh"
                || shell.command == "bash"
                || shell.command == "/bin/bash")
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
            env = shell
                .env
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("\n");
            ssm_region = shell.ssm_region.unwrap_or_default();
            ssm_path = shell.ssm_path.unwrap_or_default();
            ssm_recursive = shell.ssm_recursive.unwrap_or(false);
        }

        let tags_str = job.tags.join(", ");
        let has_retry = job.retry_policy.max_retries > 0;

        let noti_row = sqlx::query(
            "SELECT on_events FROM job_notifications WHERE job_id = ? AND channel_id = 1",
        )
        .bind(job.id.to_string())
        .fetch_optional(pool)
        .await
        .unwrap_or_default();

        let mut slack_on_running = false;
        let mut slack_on_succeeded = false;
        let mut slack_on_failed = false;

        if let Some(row) = noti_row
            && let Ok(on_events_val) = row.try_get::<serde_json::Value, _>("on_events")
            && let Ok(on_events) = serde_json::from_value::<Vec<String>>(on_events_val)
        {
            slack_on_running = on_events.contains(&"running".to_string());
            slack_on_succeeded = on_events.contains(&"succeeded".to_string());
            slack_on_failed = on_events.contains(&"failed".to_string())
                || on_events.contains(&"dead_letter".to_string());
        }

        return JobFormTemplate {
            error_message: None,
            original_name,
            is_edit,
            job_id: if copied_from_existing {
                None
            } else {
                Some(job.id)
            },
            name: if copied_from_existing {
                String::new()
            } else {
                job.name.clone()
            },
            description: job.description.clone().unwrap_or_default(),
            job_type: job.job_type.to_string(),
            worker_definition_id: job
                .worker_definition_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            worker_defs,
            spaces,
            space_id: job.space_id,
            schedule_expr: job.schedule_expr.clone().unwrap_or_default(),
            script: if copied_from_existing && script.is_empty() {
                "set -eux\n\n".to_string()
            } else {
                script
            },
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
        };
    }

    JobFormTemplate {
        error_message: None,
        original_name: None,
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
        script: "set -eux\n\n".to_string(),
        env: String::new(),
        ssm_region: String::new(),
        ssm_path: String::new(),
        ssm_recursive: false,
        dag_tasks_json: String::new(),
        timeout_sec: 3600,
        has_retry: false,
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

async fn new_job_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Query(query): Query<NewJobQuery>,
) -> impl IntoResponse {
    if let Some(copy_from) = query.copy_from
        && !copy_from.trim().is_empty()
    {
        let job_opt = crate::db::jobs::get_job_by_name(&state.db, &copy_from)
            .await
            .unwrap_or(None);
        let job = match job_opt {
            Some(job) => job,
            None => {
                return (
                    axum::http::StatusCode::NOT_FOUND,
                    "Copy source job not found",
                )
                    .into_response();
            }
        };

        return build_job_form_template_from_job(&state.db, Some(&job), None, false, true)
            .await
            .into_response();
    }

    build_job_form_template_from_job(&state.db, None, None, false, false)
        .await
        .into_response()
}

async fn create_job_submit(
    claims: WebClaims,
    State(state): State<AppState>,
    Form(form): Form<JobFormData>,
) -> impl IntoResponse {
    let job_type = JobType::from_str(&form.job_type).unwrap_or(JobType::OneShot);

    // ロードされた自作ワーカーノード定義から worker_type を決定
    let worker_def_id = form.worker_definition_id.parse::<i64>().unwrap();
    let def = crate::db::workers::get_worker_definition(&state.db, &worker_def_id)
        .await
        .unwrap()
        .unwrap();
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
    let tags = form
        .tags_str
        .clone()
        .unwrap_or_default()
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>();

    let is_active = form.is_active.as_deref() == Some("on");

    // Build Payload
    let payload = if false {
        let task_defs: Vec<DagTaskDefinition> =
            serde_json::from_str(form.dag_tasks_json.as_deref().unwrap_or("[]"))
                .unwrap_or_default();

        let env_lines = form.env.as_deref().unwrap_or("");
        let mut env = HashMap::new();
        for line in env_lines.lines() {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                env.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
            }
        }

        let ssm_region = form.ssm_region.clone().filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.clone().filter(|p| !p.trim().is_empty());
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

        let ssm_region = form.ssm_region.clone().filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.clone().filter(|p| !p.trim().is_empty());
        let ssm_recursive = if ssm_path.is_some() {
            Some(form.ssm_recursive.as_deref() == Some("on"))
        } else {
            None
        };

        let shell = ShellPayload {
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                form.script
                    .clone()
                    .unwrap_or_default()
                    .replace("\r\n", "\n"),
            ],
            working_dir: None,
            env,
            ssm_region,
            ssm_path,
            ssm_recursive,
        };
        serde_json::to_value(&shell).unwrap_or(serde_json::Value::Null)
    };

    let space_id = form
        .space_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.parse::<i64>().ok());

    let new_job = NewJob {
        name: form.name.clone(),
        description: form.description.clone().filter(|d| !d.trim().is_empty()),
        job_type,
        payload,
        schedule_expr: form.schedule_expr.clone().filter(|s| !s.trim().is_empty()),
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

    let job_type_str = new_job.job_type.to_string();
    let retry_policy_json = serde_json::to_value(&new_job.retry_policy).unwrap();
    let tags_json = serde_json::to_value(&new_job.tags).unwrap();
    let is_active_val: i8 = if new_job.is_active { 1 } else { 0 };

    let res = sqlx::query(
        r#"INSERT INTO jobs (name, description, job_type, payload, schedule_expr, retry_policy, timeout_sec, is_active, tags, worker_definition_id, space_id, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    )
    .bind(&new_job.name)
    .bind(&new_job.description)
    .bind(job_type_str)
    .bind(&new_job.payload)
    .bind(&new_job.schedule_expr)
    .bind(retry_policy_json)
    .bind(new_job.timeout_sec)
    .bind(is_active_val)
    .bind(tags_json)
    .bind(worker_def_id)
    .bind(new_job.space_id)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&mut *tx)
    .await;

    let res = match res {
        Ok(res) => res,
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e
                && (db_err.code().as_deref() == Some("23000")
                    || db_err.message().contains("Duplicate"))
            {
                let space_rows =
                    sqlx::query("SELECT id, name, description, priority, created_at, updated_at FROM spaces ORDER BY priority ASC, id ASC")
                        .fetch_all(&state.db)
                        .await
                        .unwrap_or_default();
                let mut spaces = Vec::new();
                for row in space_rows {
                    let id: i64 = row.try_get("id").unwrap_or_default();
                    let name: String = row.try_get("name").unwrap_or_default();
                    let description: Option<String> = row.try_get("description").ok();
                    let priority: i32 = row.try_get("priority").unwrap_or_default();
                    let created_at = row
                        .try_get("created_at")
                        .unwrap_or_else(|_| chrono::Utc::now());
                    let updated_at = row
                        .try_get("updated_at")
                        .unwrap_or_else(|_| chrono::Utc::now());
                    spaces.push(mrs_harris_common::models::space::Space {
                        id,
                        name,
                        description,
                        priority,
                        created_at,
                        updated_at,
                    });
                }
                let worker_defs = crate::db::workers::list_active_worker_definitions(&state.db)
                    .await
                    .unwrap_or_default();
                return JobFormTemplate {
                    error_message: Some(
                        "指定されたジョブ名は既に使用されています。別の名前を指定してください。"
                            .to_string(),
                    ),
                    is_edit: false,
                    job_id: None,
                    original_name: None,
                    name: form.name.clone(),
                    description: form.description.clone().unwrap_or_default(),
                    tags_str: form.tags_str.clone().unwrap_or_default(),
                    job_type: form.job_type.clone(),
                    worker_definition_id: form.worker_definition_id.clone(),
                    worker_defs,
                    spaces,
                    space_id: new_job.space_id,
                    schedule_expr: form.schedule_expr.clone().unwrap_or_default(),
                    script: form.script.clone().unwrap_or_default(),
                    env: form.env.clone().unwrap_or_default(),
                    ssm_region: form.ssm_region.clone().unwrap_or_default(),
                    ssm_path: form.ssm_path.clone().unwrap_or_default(),
                    ssm_recursive: form.ssm_recursive.as_deref() == Some("on"),
                    dag_tasks_json: form.dag_tasks_json.clone().unwrap_or_default(),
                    timeout_sec: form.timeout_sec,
                    has_retry: form.has_retry.as_deref() == Some("on"),
                    max_retries: form.max_retries,
                    backoff: form.backoff.clone(),
                    base_delay_sec: form.base_delay_sec,
                    is_active: form.is_active.as_deref() == Some("on"),
                    slack_on_running: form.slack_on_running.as_deref() == Some("on"),
                    slack_on_succeeded: form.slack_on_succeeded.as_deref() == Some("on"),
                    slack_on_failed: form.slack_on_failed.as_deref() == Some("on"),
                }
                .into_response();
            }
            return Redirect::to("/jobs/new").into_response();
        }
    };

    let job_id = res.last_insert_id() as i64;

    if false {
        let task_defs: Vec<DagTaskDefinition> =
            serde_json::from_str(form.dag_tasks_json.as_deref().unwrap_or("[]"))
                .unwrap_or_default();

        for task in task_defs {
            sqlx::query(
                r#"INSERT INTO dag_tasks (dag_id, task_name, payload, worker_type, retry_policy, timeout_sec)
                   VALUES (?, ?, ?, ?, ?, ?)"#
            )
            .bind(job_id)
            .bind(&task.name)
            .bind(&task.payload)
            .bind(task.worker_type.to_string())
            .bind(task.retry_policy.as_ref().map(|rp| serde_json::to_value(rp).unwrap()))
            .bind(task.timeout_sec)
            .execute(&mut *tx)
            .await
            .unwrap();

            for dep in task.depends_on {
                sqlx::query(
                    r#"INSERT INTO dag_edges (dag_id, from_task, to_task)
                       VALUES (?, ?, ?)"#,
                )
                .bind(job_id)
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
               VALUES (1, 'default-slack', 'slack', '{"webhook_url":""}', 1)
               ON DUPLICATE KEY UPDATE is_active=is_active"#,
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO job_notifications (job_id, channel_id, on_events)
               VALUES (?, 1, ?)"#,
        )
        .bind(job_id.to_string())
        .bind(events_json)
        .execute(&mut *tx)
        .await
        .unwrap();
    }

    tx.commit().await.unwrap();

    if let Err(e) = ensure_job_history(&state.db, &job_id, &claims.0.username).await {
        tracing::error!("Failed to create initial job history after create: {:?}", e);
    }

    Redirect::to("/jobs").into_response()
}

async fn job_detail_page(
    claims: WebClaims,
    State(state): State<AppState>,
    Path(name_in_path): Path<String>,
    Query(query): Query<RunsQuery>,
) -> impl IntoResponse {
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name_in_path)
        .await
        .unwrap_or(None);
    let job = match job_opt {
        Some(j) => j,
        None => return (axum::http::StatusCode::NOT_FOUND, "Job not found").into_response(),
    };
    let id = job.id;

    let job_type_str = job_type_label(&job.job_type);

    let retry_backoff_str = match job.retry_policy.backoff {
        BackoffStrategy::Fixed => "固定時間",
        BackoffStrategy::Linear => "線形",
        BackoffStrategy::Exponential => "指数",
    };

    // Load recent job runs with pagination metadata
    let current_page = 1u32;
    let limit = 10u32;
    let offset = 0u32;
    let current_sort = query.sort.clone().unwrap_or_else(|| "default".to_string());
    let desc = current_sort != "asc";

    let total_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_runs WHERE job_id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let total_pages = ((total_runs as f64) / (limit as f64)).ceil() as u32;
    let start_index = if total_runs == 0 { 0 } else { 1 };
    let end_index = std::cmp::min(limit as usize, total_runs as usize);
    let pages = (1..=total_pages).collect::<Vec<u32>>();

    let runs_db =
        match crate::db::runs::list_runs(&state.db, Some(&id), Some(limit), Some(offset), desc)
            .await
        {
            Ok(runs) => runs,
            Err(e) => {
                tracing::error!("Failed to list runs in job_detail_page: {:?}", e);
                Vec::new()
            }
        };
    let mut recent_runs = Vec::new();
    for r in runs_db {
        recent_runs.push(job_run_render_item_from_run(&r));
    }

    let is_dag = false;
    let mut command_preview = String::new();
    let mut env_vars_list = Vec::new();
    let mut ssm_region = String::new();
    let mut ssm_path = String::new();
    let mut ssm_recursive = false;
    let mut working_dir: Option<String> = None;
    let mut dag_tasks_json = "[]".to_string();
    let mut dag_edges_json = "[]".to_string();

    // 1. ワーカー定義名の取得
    let mut worker_definition_name = "デフォルト (起動タイプ設定)".to_string();
    if let Some(def_id) = job.worker_definition_id
        && let Ok(Some(def)) = crate::db::workers::get_worker_definition(&state.db, &def_id).await
    {
        worker_definition_name = def.name;
    }

    // 2. Slack通知設定の取得
    let noti_row =
        sqlx::query("SELECT on_events FROM job_notifications WHERE job_id = ? AND channel_id = 1")
            .bind(job.id.to_string())
            .fetch_optional(&state.db)
            .await
            .unwrap_or_default();

    let mut slack_on_running = false;
    let mut slack_on_succeeded = false;
    let mut slack_on_failed = false;

    if let Some(row) = noti_row
        && let Ok(on_events_val) = row.try_get::<serde_json::Value, _>("on_events")
        && let Ok(on_events) = serde_json::from_value::<Vec<String>>(on_events_val)
    {
        slack_on_running = on_events.contains(&"running".to_string());
        slack_on_succeeded = on_events.contains(&"succeeded".to_string());
        slack_on_failed = on_events.contains(&"failed".to_string())
            || on_events.contains(&"dead_letter".to_string());
    }

    // 3. 環境変数・SSM設定・DAG設定の取得
    if is_dag {
        // Load DAG tasks and edges from DB
        let tasks_rows =
            sqlx::query("SELECT task_name, worker_type, payload FROM dag_tasks WHERE dag_id = ?")
                .bind(id)
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

        let edges_rows = sqlx::query("SELECT from_task, to_task FROM dag_edges WHERE dag_id = ?")
            .bind(id)
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
        ssm_region = job
            .payload
            .get("ssm_region")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        ssm_path = job
            .payload
            .get("ssm_path")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        ssm_recursive = job
            .payload
            .get("ssm_recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    } else {
        // Parse payload as shell command
        if let Ok(shell) = serde_json::from_value::<ShellPayload>(job.payload.clone()) {
            command_preview = display_shell_command(&shell);

            if !shell.env.is_empty() {
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
    let env_vars_text = env_vars_list
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n");

    let timeout_minutes = job.timeout_sec / 60;

    let created_at_str = job
        .created_at
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let updated_at_str = job
        .updated_at
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    // 4. 設定変更履歴の取得（空なら自動で初期履歴を記録）
    let mut history_rows = sqlx::query(
        "SELECT version, changed_by, payload, changed_at FROM job_history WHERE job_id = ? ORDER BY version DESC"
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if history_rows.is_empty() {
        if let Err(e) = ensure_job_history(&state.db, &id, &claims.0.username).await {
            tracing::error!(
                "Failed to ensure initial job history on detail load: {:?}",
                e
            );
        }
        history_rows = sqlx::query(
            "SELECT version, changed_by, payload, changed_at FROM job_history WHERE job_id = ? ORDER BY version DESC"
        )
        .bind(id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    }

    let mut history = Vec::new();
    for row in history_rows {
        let version: u32 = row.try_get("version").unwrap_or(1);
        let changed_by: String = row
            .try_get("changed_by")
            .unwrap_or_else(|_| "admin".to_string());
        let changed_at: chrono::DateTime<chrono::Utc> = row.try_get("changed_at").unwrap();
        let changed_at_str = changed_at
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let payload_val: serde_json::Value =
            row.try_get("payload").unwrap_or(serde_json::Value::Null);
        let payload_json = serde_json::to_string(&payload_val).unwrap_or_default();

        history.push(JobHistoryRenderItem {
            version,
            changed_by,
            changed_at_str,
            payload_json,
        });
    }

    let empty_runs = if total_runs > 10 {
        vec![(); 10 - recent_runs.len()]
    } else {
        Vec::new()
    };

    JobDetailTemplate {
        job_name: job.name.clone(),
        job,
        job_type_str,
        worker_definition_name,
        timeout_minutes,
        retry_backoff_str,
        recent_runs,
        total_runs,
        current_page,
        total_pages,
        start_index,
        end_index,
        page_items: pages.into_iter().map(Some).collect(),
        current_sort,
        empty_runs,
        is_dag,
        command_preview,
        env_vars_text,
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
        latest_version: history.first().map(|h| h.version).unwrap_or(0),
        history,
        history_current_page: 1,
        history_total_pages: 1,
        total_history: 0,
        history_start_index: 0,
        history_end_index: 0,
        history_page_items: Vec::new(),
        empty_history: Vec::new(),
        initial_tab: "runs".to_string(),
    }
    .into_response()
}

async fn job_runs_list(
    _claims: WebClaims,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name_in_path): Path<String>,
    Query(query): Query<RunsQuery>,
) -> impl IntoResponse {
    let is_hx_request = headers
        .get("hx-request")
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_hx_request {
        let mut redirect_url = format!("/jobs/{name_in_path}?tab=runs");
        let mut params = Vec::new();
        if let Some(page) = query.page
            && page > 1
        {
            params.push(format!("page={page}"));
        }
        if let Some(sort) = query.sort.as_deref()
            && sort != "default"
        {
            params.push(format!("sort={sort}"));
        }
        if !params.is_empty() {
            redirect_url.push('&');
            redirect_url.push_str(&params.join("&"));
        }
        return axum::response::Redirect::to(&redirect_url).into_response();
    }

    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name_in_path)
        .await
        .unwrap_or(None);
    let job = match job_opt {
        Some(j) => j,
        None => return axum::response::Redirect::to("/jobs").into_response(),
    };
    let id = job.id;

    let current_page = query.page.unwrap_or(1).max(1);
    let limit = 10u32;
    let offset = (current_page - 1) * limit;
    let current_sort = query.sort.clone().unwrap_or_else(|| "default".to_string());
    let desc = current_sort != "asc";

    let total_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_runs WHERE job_id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let total_pages = ((total_runs as f64) / (limit as f64)).ceil() as u32;
    let start_index = if total_runs == 0 {
        0
    } else {
        (offset as usize) + 1
    };
    let end_index = std::cmp::min((offset + limit) as usize, total_runs as usize);
    let pages = (1..=total_pages).collect::<Vec<u32>>();

    let runs_db =
        match crate::db::runs::list_runs(&state.db, Some(&id), Some(limit), Some(offset), desc)
            .await
        {
            Ok(runs) => runs,
            Err(e) => {
                tracing::error!("Failed to list runs in job_runs_list: {:?}", e);
                Vec::new()
            }
        };

    let mut recent_runs = Vec::new();
    for r in runs_db {
        recent_runs.push(job_run_render_item_from_run(&r));
    }

    let empty_runs = if total_runs > 10 {
        vec![(); 10 - recent_runs.len()]
    } else {
        Vec::new()
    };

    JobRunsTableTemplate {
        job_name: job.name.clone(),
        recent_runs,
        total_runs,
        current_page,
        total_pages,
        start_index,
        end_index,
        page_items: pages.into_iter().map(Some).collect(),
        current_sort,
        empty_runs,
    }
    .into_response()
}

async fn edit_job_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(name_in_path): Path<String>,
) -> impl IntoResponse {
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name_in_path)
        .await
        .unwrap_or(None);
    let job = match job_opt {
        Some(j) => j,
        None => return (axum::http::StatusCode::NOT_FOUND, "Job not found").into_response(),
    };
    build_job_form_template_from_job(&state.db, Some(&job), Some(job.name.clone()), true, false)
        .await
        .into_response()
}

async fn edit_job_submit(
    claims: WebClaims,
    State(state): State<AppState>,
    Path(name_in_path): Path<String>,
    Form(form): Form<JobFormData>,
) -> impl IntoResponse {
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &name_in_path)
        .await
        .unwrap_or(None);
    let existing_job = match job_opt {
        Some(j) => j,
        None => return Redirect::to("/jobs").into_response(),
    };
    let id = existing_job.id;

    let job_type = JobType::from_str(&form.job_type).unwrap_or(JobType::OneShot);

    // ロードされた自作ワーカーノード定義から worker_type を決定
    let worker_def_id = form.worker_definition_id.parse::<i64>().unwrap();
    let _def = crate::db::workers::get_worker_definition(&state.db, &worker_def_id)
        .await
        .unwrap()
        .unwrap();

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

    let tags = form
        .tags_str
        .clone()
        .unwrap_or_default()
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>();

    let is_active = form.is_active.as_deref() == Some("on");

    let payload = if false {
        let task_defs: Vec<DagTaskDefinition> =
            serde_json::from_str(form.dag_tasks_json.as_deref().unwrap_or("[]"))
                .unwrap_or_default();

        let env_lines = form.env.as_deref().unwrap_or("");
        let mut env = HashMap::new();
        for line in env_lines.lines() {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                env.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
            }
        }

        let ssm_region = form.ssm_region.clone().filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.clone().filter(|p| !p.trim().is_empty());
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

        let ssm_region = form.ssm_region.clone().filter(|r| !r.trim().is_empty());
        let ssm_path = form.ssm_path.clone().filter(|p| !p.trim().is_empty());
        let ssm_recursive = if ssm_path.is_some() {
            Some(form.ssm_recursive.as_deref() == Some("on"))
        } else {
            None
        };

        let shell = ShellPayload {
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                form.script
                    .clone()
                    .unwrap_or_default()
                    .replace("\r\n", "\n"),
            ],
            working_dir: None,
            env,
            ssm_region,
            ssm_path,
            ssm_recursive,
        };
        serde_json::to_value(&shell).unwrap_or(serde_json::Value::Null)
    };

    let space_id = form
        .space_id
        .clone()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.parse::<i64>().ok());

    let pool = &state.db;
    let mut tx = pool.begin().await.unwrap();

    let retry_policy_json = serde_json::to_value(&retry_policy).unwrap();
    let tags_json = serde_json::to_value(&tags).unwrap();
    let is_active_val: i8 = if is_active { 1 } else { 0 };

    let update_result = sqlx::query(
        r#"UPDATE jobs 
           SET name = ?, description = ?, job_type = ?, payload = ?, schedule_expr = ?, retry_policy = ?, timeout_sec = ?, is_active = ?, tags = ?, worker_definition_id = ?, space_id = ?, updated_at = ?
           WHERE id = ?"#
    )
    .bind(&form.name)
    .bind(form.description.clone().filter(|d| !d.trim().is_empty()))
    .bind(job_type.to_string())
    .bind(&payload)
    .bind(form.schedule_expr.clone().filter(|s| !s.trim().is_empty()))
    .bind(retry_policy_json)
    .bind(form.timeout_sec)
    .bind(is_active_val)
    .bind(tags_json)
    .bind(worker_def_id)
    .bind(space_id)
    .bind(Utc::now())
    .bind(id)
    .execute(&mut *tx)
    .await;

    match update_result {
        Ok(_) => {}
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e
                && (db_err.code().as_deref() == Some("23000")
                    || db_err.message().contains("Duplicate"))
            {
                let space_rows =
                    sqlx::query("SELECT id, name, description, priority, created_at, updated_at FROM spaces ORDER BY priority ASC, id ASC")
                        .fetch_all(&state.db)
                        .await
                        .unwrap_or_default();
                let mut spaces = Vec::new();
                for row in space_rows {
                    let id: i64 = row.try_get("id").unwrap_or_default();
                    let name: String = row.try_get("name").unwrap_or_default();
                    let description: Option<String> = row.try_get("description").ok();
                    let priority: i32 = row.try_get("priority").unwrap_or_default();
                    let created_at = row
                        .try_get("created_at")
                        .unwrap_or_else(|_| chrono::Utc::now());
                    let updated_at = row
                        .try_get("updated_at")
                        .unwrap_or_else(|_| chrono::Utc::now());
                    spaces.push(mrs_harris_common::models::space::Space {
                        id,
                        name,
                        description,
                        priority,
                        created_at,
                        updated_at,
                    });
                }
                let worker_defs = crate::db::workers::list_active_worker_definitions(&state.db)
                    .await
                    .unwrap_or_default();
                return JobFormTemplate {
                    error_message: Some(
                        "指定されたジョブ名は既に使用されています。別の名前を指定してください。"
                            .to_string(),
                    ),
                    is_edit: true,
                    job_id: Some(existing_job.id),
                    original_name: Some(existing_job.name.clone()),
                    name: form.name.clone(),
                    description: form.description.clone().unwrap_or_default(),
                    tags_str: form.tags_str.clone().unwrap_or_default(),
                    job_type: form.job_type.clone(),
                    worker_definition_id: form.worker_definition_id.clone(),
                    worker_defs,
                    spaces,
                    space_id: space_id.or(existing_job.space_id),
                    schedule_expr: form.schedule_expr.clone().unwrap_or_default(),
                    script: form.script.clone().unwrap_or_default(),
                    env: form.env.clone().unwrap_or_default(),
                    ssm_region: form.ssm_region.clone().unwrap_or_default(),
                    ssm_path: form.ssm_path.clone().unwrap_or_default(),
                    ssm_recursive: form.ssm_recursive.as_deref() == Some("on"),
                    dag_tasks_json: form.dag_tasks_json.clone().unwrap_or_default(),
                    timeout_sec: form.timeout_sec,
                    has_retry: form.has_retry.as_deref() == Some("on"),
                    max_retries: form.max_retries,
                    backoff: form.backoff.clone(),
                    base_delay_sec: form.base_delay_sec,
                    is_active: form.is_active.as_deref() == Some("on"),
                    slack_on_running: form.slack_on_running.as_deref() == Some("on"),
                    slack_on_succeeded: form.slack_on_succeeded.as_deref() == Some("on"),
                    slack_on_failed: form.slack_on_failed.as_deref() == Some("on"),
                }
                .into_response();
            }
            // Other DB errors
            return Redirect::to(&format!("/jobs/{}", name_in_path)).into_response();
        }
    }

    if false {
        // Clear old tasks/edges and write new ones
        sqlx::query("DELETE FROM dag_tasks WHERE dag_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("DELETE FROM dag_edges WHERE dag_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();

        let task_defs: Vec<DagTaskDefinition> =
            serde_json::from_str(form.dag_tasks_json.as_deref().unwrap_or("[]"))
                .unwrap_or_default();

        for task in task_defs {
            sqlx::query(
                r#"INSERT INTO dag_tasks (dag_id, task_name, payload, worker_type, retry_policy, timeout_sec)
                   VALUES (?, ?, ?, ?, ?, ?)"#
            )
            .bind(id)
            .bind(&task.name)
            .bind(&task.payload)
            .bind(task.worker_type.to_string())
            .bind(task.retry_policy.as_ref().map(|rp| serde_json::to_value(rp).unwrap()))
            .bind(task.timeout_sec)
            .execute(&mut *tx)
            .await
            .unwrap();

            for dep in task.depends_on {
                sqlx::query(
                    r#"INSERT INTO dag_edges (dag_id, from_task, to_task)
                       VALUES (?, ?, ?)"#,
                )
                .bind(id)
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

    sqlx::query("DELETE FROM job_notifications WHERE job_id = ? AND channel_id = 1")
        .bind(id)
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

        sqlx::query(
            r#"INSERT INTO notification_channels (id, name, channel_type, config, is_active)
               VALUES (1, 'default-slack', 'slack', '{"webhook_url":""}', 1)
               ON DUPLICATE KEY UPDATE is_active=is_active"#,
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO job_notifications (job_id, channel_id, on_events)
               VALUES (?, 1, ?)"#,
        )
        .bind(id)
        .bind(events_json)
        .execute(&mut *tx)
        .await
        .unwrap();
    }

    tx.commit().await.unwrap();

    if let Err(e) = record_job_history(&state.db, &id, &claims.0.username).await {
        tracing::error!("Failed to record job history after edit: {:?}", e);
    }

    Redirect::to(&format!("/jobs/{}", form.name)).into_response()
}

async fn build_job_snapshot(pool: &MySqlPool, job: &Job) -> serde_json::Value {
    let job_type_str = job_type_label(&job.job_type);

    let worker_definition_name = if let Some(def_id) = job.worker_definition_id {
        if let Ok(Some(def)) = crate::db::workers::get_worker_definition(pool, &def_id).await {
            def.name
        } else {
            "デフォルト (起動タイプ設定)".to_string()
        }
    } else {
        "デフォルト (起動タイプ設定)".to_string()
    };

    let space_name = if let Some(space_id) = job.space_id {
        sqlx::query_scalar::<_, String>("SELECT name FROM spaces WHERE id = ?")
            .bind(space_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "未分類 (Unclassified)".to_string())
    } else {
        "未分類 (Unclassified)".to_string()
    };

    let mut env_vars = HashMap::new();
    let mut script_or_dag = serde_json::Value::Null;
    let mut ssm_region = String::new();
    let mut ssm_path = String::new();
    let mut ssm_recursive = false;

    if false {
        // Load DAG tasks
        let tasks_rows =
            sqlx::query("SELECT task_name, worker_type, payload FROM dag_tasks WHERE dag_id = ?")
                .bind(job.id)
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
        ssm_region = job
            .payload
            .get("ssm_region")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        ssm_path = job
            .payload
            .get("ssm_path")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        ssm_recursive = job
            .payload
            .get("ssm_recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    } else {
        if let Ok(shell) = serde_json::from_value::<ShellPayload>(job.payload.clone()) {
            script_or_dag = serde_json::Value::String(display_shell_command(&shell));

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
    let noti_row =
        sqlx::query("SELECT on_events FROM job_notifications WHERE job_id = ? AND channel_id = 1")
            .bind(job.id.to_string())
            .fetch_optional(pool)
            .await
            .unwrap_or_default();

    let mut slack_running = false;
    let mut slack_succeeded = false;
    let mut slack_failed = false;

    if let Some(row) = noti_row
        && let Ok(on_events_val) = row.try_get::<serde_json::Value, _>("on_events")
        && let Ok(on_events) = serde_json::from_value::<Vec<String>>(on_events_val)
    {
        slack_running = on_events.contains(&"running".to_string());
        slack_succeeded = on_events.contains(&"succeeded".to_string());
        slack_failed = on_events.contains(&"failed".to_string())
            || on_events.contains(&"dead_letter".to_string());
    }

    serde_json::json!({
        "ジョブ名": job.name,
        "所属スペース": space_name,
        "説明": job.description.as_deref().unwrap_or(""),
        "ジョブタイプ": job_type_str,
        "スケジュール (Cron)": schedule_display(job.schedule_expr.as_deref()),
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

pub(crate) async fn ensure_job_history(
    pool: &MySqlPool,
    job_id: &i64,
    changed_by: &str,
) -> anyhow::Result<()> {
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM job_history WHERE job_id = ? LIMIT 1")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;

    if existing.is_some() {
        return Ok(());
    }

    record_job_history(pool, job_id, changed_by).await
}

async fn record_job_history(
    pool: &MySqlPool,
    job_id: &i64,
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
    let version_row = sqlx::query("SELECT MAX(version) as max_v FROM job_history WHERE job_id = ?")
        .bind(job_id)
        .fetch_one(pool)
        .await?;

    let current_max: Option<u32> = version_row.try_get("max_v").ok();
    let next_version = current_max.unwrap_or(0) + 1;

    // 4. 履歴に挿入
    sqlx::query(
        r#"INSERT INTO job_history (job_id, version, payload, changed_by, changed_at)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(job_id)
    .bind(next_version)
    .bind(snapshot)
    .bind(changed_by)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(())
}

#[derive(serde::Deserialize)]
pub struct ValidateNameForm {
    pub name: String,
    pub current_job_id: Option<i64>,
}

pub async fn api_validate_job_name(
    _claims: WebClaims,
    State(state): State<AppState>,
    Form(form): Form<ValidateNameForm>,
) -> impl IntoResponse {
    let name_trimmed = form.name.trim();
    if name_trimmed.is_empty() {
        return axum::response::Html("").into_response();
    }
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, name_trimmed)
        .await
        .unwrap_or(None);
    let mut exists = job_opt.is_some();
    if let Some(job) = job_opt
        && let Some(ignore_id) = form.current_job_id
        && job.id == ignore_id
    {
        exists = false;
    }

    if exists {
        axum::response::Html(r#"<span style="color: #ef4444; font-size: 0.85rem; display: block; margin-top: 4px;">このジョブ名は既に使用されています。</span>"#).into_response()
    } else {
        axum::response::Html("").into_response()
    }
}
