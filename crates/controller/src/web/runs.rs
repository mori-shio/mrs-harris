use askama::Template;
use axum::{
    Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Redirect},
    routing::get,
};

use chrono::{DateTime, Utc};
use sqlx::{MySqlPool, Row};
use std::str::FromStr;

use mrs_harris_common::models::run::{JobRun, LogArchiveStatus, LogLine, LogStream};

use super::auth::WebClaims;
use crate::app::AppState;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mrs_harris_common::models::job::WorkerType;
    use mrs_harris_common::models::run::{LogArchiveStatus, RunStatus, TriggerType};

    fn sample_run(status: RunStatus, log_archive_status: Option<LogArchiveStatus>) -> JobRun {
        let now = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        JobRun {
            id: 1,
            job_id: 2,
            run_number: 3,
            status,
            worker_type: WorkerType::Lambda,
            worker_id: None,
            trigger_type: TriggerType::Manual,
            attempt: 1,
            scheduled_at: None,
            started_at: Some(now),
            finished_at: None,
            next_retry_at: None,
            duration_ms: None,
            log_archive_status,
            log_archive_store: None,
            log_archive_key: None,
            log_line_count: None,
            log_archive_bytes: None,
            log_archived_at: None,
            output: None,
            error: None,
            job_history_id: Some(1),
            worker_definition_history_id: Some(2),
            config_version: Some(1),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn live_polling_continues_until_archive_state_set_for_terminal_runs() {
        assert!(should_live_polling(&sample_run(RunStatus::Running, None)));
        assert!(should_live_polling(&sample_run(RunStatus::Succeeded, None)));
        assert!(should_live_polling(&sample_run(
            RunStatus::Failed,
            Some(LogArchiveStatus::Pending)
        )));
        assert!(!should_live_polling(&sample_run(
            RunStatus::Succeeded,
            Some(LogArchiveStatus::Archived)
        )));
        assert!(!should_live_polling(&sample_run(
            RunStatus::Cancelled,
            Some(LogArchiveStatus::Failed)
        )));
    }

    #[test]
    fn websocket_is_used_only_for_non_terminal_runs() {
        assert!(should_use_log_websocket(&sample_run(
            RunStatus::Running,
            None
        )));
        assert!(should_use_log_websocket(&sample_run(
            RunStatus::Queued,
            None
        )));
        assert!(!should_use_log_websocket(&sample_run(
            RunStatus::Succeeded,
            None
        )));
        assert!(!should_use_log_websocket(&sample_run(
            RunStatus::Succeeded,
            Some(LogArchiveStatus::Pending)
        )));
        assert!(!should_use_log_websocket(&sample_run(
            RunStatus::DeadLetter,
            Some(LogArchiveStatus::Archived)
        )));
    }
}

#[derive(Clone)]
pub struct TaskRunItem {
    pub task_name: String,
    pub status_str: String,
    pub status_badge_class: String,
    pub status_ja: String,
    pub attempt: u32,
    pub duration_str: String,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "runs/detail.html")]
struct RunDetailTemplate {
    run: JobRun,
    job_name: String,
    run_number: i64,
    is_live_polling: bool,
    use_log_websocket: bool,
    is_dag: bool,
    task_runs: Vec<TaskRunItem>,
    status_ja: String,
    status_badge_class: &'static str,
    archive_state_label: String,
    archive_state_badge_class: &'static str,
    duration_str: String,
    started_at_str: String,
    finished_at_str: String,
    dag_tasks_json: String,
    dag_edges_json: String,
    task_runs_json: String,
    config_payload_json: String,
    is_live_partial: bool,
}
crate::impl_into_response!(RunDetailTemplate);

#[derive(Template)]
#[template(path = "runs/detail_live.html")]
struct RunDetailLiveTemplate {
    run: JobRun,
    job_name: String,
    run_number: i64,
    is_live_polling: bool,
    use_log_websocket: bool,
    is_dag: bool,
    task_runs: Vec<TaskRunItem>,
    status_ja: String,
    status_badge_class: &'static str,
    archive_state_label: String,
    archive_state_badge_class: &'static str,
    duration_str: String,
    started_at_str: String,
    finished_at_str: String,
    dag_tasks_json: String,
    dag_edges_json: String,
    task_runs_json: String,
    config_payload_json: String,
    is_live_partial: bool,
}
crate::impl_into_response!(RunDetailLiveTemplate);

fn should_live_polling(run: &JobRun) -> bool {
    if !run.status.is_terminal() {
        return true;
    }

    matches!(
        run.log_archive_status,
        None | Some(LogArchiveStatus::Pending) | Some(LogArchiveStatus::Exporting)
    )
}

fn should_use_log_websocket(run: &JobRun) -> bool {
    !run.status.is_terminal()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs/{id}", get(run_detail_page))
        .route("/runs/{id}/live", get(run_detail_live))
        .route("/runs/{id}/logs", get(run_logs_json))
        .route("/runs/{id}/logs/ws", get(run_logs_ws_upgrade))
        .route(
            "/jobs/{job_name}/runs/{run_number}",
            get(run_detail_by_number),
        )
        .route(
            "/jobs/{job_name}/runs/{run_number}/live",
            get(run_detail_by_number_live),
        )
}

async fn fetch_run_detail_data(
    pool: &MySqlPool,
    id: i64,
) -> anyhow::Result<(
    JobRun,
    String, // job_name
    i64,    // run_number
    bool,   // is_dag
    Vec<TaskRunItem>,
    String,       // status_ja
    &'static str, // status_badge_class
    String,       // archive_state_label
    &'static str, // archive_state_badge_class
    String,       // trigger_ja
    String,       // duration_str
    String,       // started_at_str
    String,       // finished_at_str
    String,       // dag_tasks_json
    String,       // dag_edges_json
    String,       // task_runs_json
    String,       // config_payload_json
)> {
    let run = crate::db::runs::get_run(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Run not found"))?;

    let run_number: i64 = run.run_number;

    let job = crate::db::jobs::get_job(pool, &run.job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Job not found"))?;

    let job_name = job.name;
    let is_dag = false;

    // Fetch task runs if it is a DAG
    let mut task_runs = Vec::new();
    let mut dag_tasks_json = "[]".to_string();
    let mut dag_edges_json = "[]".to_string();
    let mut task_runs_json = "[]".to_string();

    if is_dag {
        let task_runs_rows = sqlx::query(
            "SELECT task_name, status, attempt, duration_ms, error FROM task_runs WHERE run_id = ? ORDER BY created_at ASC"
        )
        .bind(id.to_string())
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for row in task_runs_rows {
            let task_name: String = row.try_get("task_name")?;
            let status_db_str: String = row.try_get("status")?;
            let attempt: u32 = row.try_get("attempt")?;
            let duration_ms: Option<i64> = row.try_get("duration_ms")?;
            let error: Option<String> = row.try_get("error")?;

            let (status_ja, status_badge_class) = match status_db_str.as_str() {
                "pending" => ("保留中", "pending"),
                "scheduled" => ("予約済み", "pending"),
                "queued" => ("キュー待ち", "pending"),
                "running" => ("実行中", "running"),
                "succeeded" => ("成功", "succeeded"),
                "failed" => ("失敗", "failed"),
                "retrying" => ("再試行中", "retrying"),
                "cancelled" => ("キャンセル済み", "cancelled"),
                "dead_letter" => ("失敗 (要確認)", "failed"),
                "skipped" => ("スキップ", "skipped"),
                _ => ("不明", "pending"),
            };

            let duration_str = match duration_ms {
                Some(ms) => {
                    if ms >= 1000 {
                        format!("{:.1}s", ms as f64 / 1000.0)
                    } else {
                        format!("{}ms", ms)
                    }
                }
                None => "-".to_string(),
            };

            task_runs.push(TaskRunItem {
                task_name,
                status_str: status_db_str,
                status_badge_class: status_badge_class.to_string(),
                status_ja: status_ja.to_string(),
                attempt,
                duration_str,
                error,
            });
        }

        // Fetch DAG tasks definitions
        let tasks_rows =
            sqlx::query("SELECT task_name, worker_type, payload FROM dag_tasks WHERE dag_id = ?")
                .bind(run.job_id.to_string())
                .fetch_all(pool)
                .await
                .unwrap_or_default();

        let mut tasks = Vec::new();
        for row in tasks_rows {
            let name: String = row.try_get("task_name")?;
            let wt: String = row.try_get("worker_type")?;
            let pl: serde_json::Value = row.try_get("payload")?;
            tasks.push(serde_json::json!({
                "name": name,
                "worker_type": wt,
                "payload": pl
            }));
        }
        dag_tasks_json = serde_json::to_string(&tasks).unwrap_or_else(|_| "[]".to_string());

        // Fetch DAG edges
        let edges_rows = sqlx::query("SELECT from_task, to_task FROM dag_edges WHERE dag_id = ?")
            .bind(run.job_id.to_string())
            .fetch_all(pool)
            .await
            .unwrap_or_default();

        let mut edges = Vec::new();
        for row in edges_rows {
            let from: String = row.try_get("from_task")?;
            let to: String = row.try_get("to_task")?;
            edges.push(serde_json::json!({
                "from": from,
                "to": to
            }));
        }
        dag_edges_json = serde_json::to_string(&edges).unwrap_or_else(|_| "[]".to_string());

        // Prepare task_runs_json for live status updates in javascript
        let mut runs_json_items = Vec::new();
        for tr in &task_runs {
            runs_json_items.push(serde_json::json!({
                "task_name": tr.task_name,
                "status_str": tr.status_str
            }));
        }
        task_runs_json =
            serde_json::to_string(&runs_json_items).unwrap_or_else(|_| "[]".to_string());
    }

    let status_ja = run.status.label_ja().to_string();
    let status_badge_class = run.status.badge_class();
    let archive_state_label = match run.log_archive_status.as_ref() {
        Some(status) => {
            let store_suffix = run
                .log_archive_store
                .as_ref()
                .map(|store| format!(" ({})", store.label_ja()))
                .unwrap_or_default();
            format!("{}{}", status.label_ja(), store_suffix)
        }
        None if run.status.is_terminal() => "ホットログ".to_string(),
        None => "ライブ".to_string(),
    };
    let archive_state_badge_class = match run.log_archive_status.as_ref() {
        Some(status) => status.badge_class(),
        None if run.status.is_terminal() => "pending",
        None => "info",
    };
    let trigger_ja = run.trigger_type.label_ja().to_string();

    let duration_str = match run.duration_ms {
        Some(ms) => {
            if ms >= 1000 {
                format!("{:.1}s", ms as f64 / 1000.0)
            } else {
                format!("{}ms", ms)
            }
        }
        None => "-".to_string(),
    };

    let started_at_str = match run.started_at {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
    };

    let finished_at_str = match run.finished_at {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
    };

    let config_payload_json = match run.job_history_id {
        Some(h_id) => {
            let payload_val: serde_json::Value =
                sqlx::query_scalar("SELECT payload FROM job_history WHERE id = ?")
                    .bind(h_id)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(serde_json::Value::Null);
            serde_json::to_string(&payload_val).unwrap_or_else(|_| "{}".to_string())
        }
        None => "{}".to_string(),
    };

    Ok((
        run,
        job_name,
        run_number,
        is_dag,
        task_runs,
        status_ja,
        status_badge_class,
        archive_state_label,
        archive_state_badge_class,
        trigger_ja,
        duration_str,
        started_at_str,
        finished_at_str,
        dag_tasks_json,
        dag_edges_json,
        task_runs_json,
        config_payload_json,
    ))
}

async fn run_detail_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match fetch_run_detail_data(&state.db, id).await {
        Ok(data) => RunDetailTemplate {
            is_live_polling: should_live_polling(&data.0),
            use_log_websocket: should_use_log_websocket(&data.0),
            run: data.0,
            job_name: data.1,
            run_number: data.2,
            is_dag: data.3,
            task_runs: data.4,
            status_ja: data.5,
            status_badge_class: data.6,
            archive_state_label: data.7,
            archive_state_badge_class: data.8,
            duration_str: data.10,
            started_at_str: data.11,
            finished_at_str: data.12,
            dag_tasks_json: data.13,
            dag_edges_json: data.14,
            task_runs_json: data.15,
            config_payload_json: data.16,
            is_live_partial: false,
        }
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch run details: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal Server Error: {}", e),
            )
                .into_response()
        }
    }
}

async fn run_detail_live(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match fetch_run_detail_data(&state.db, id).await {
        Ok(data) => RunDetailLiveTemplate {
            is_live_polling: should_live_polling(&data.0),
            use_log_websocket: should_use_log_websocket(&data.0),
            run: data.0,
            job_name: data.1,
            run_number: data.2,
            is_dag: data.3,
            task_runs: data.4,
            status_ja: data.5,
            status_badge_class: data.6,
            archive_state_label: data.7,
            archive_state_badge_class: data.8,
            duration_str: data.10,
            started_at_str: data.11,
            finished_at_str: data.12,
            dag_tasks_json: data.13,
            dag_edges_json: data.14,
            task_runs_json: data.15,
            config_payload_json: data.16,
            is_live_partial: true,
        }
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch run details for live polling: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal Server Error: {}", e),
            )
                .into_response()
        }
    }
}

async fn fetch_run_detail_data_by_number(
    pool: &MySqlPool,
    job_id: i64,
    run_number: i64,
) -> anyhow::Result<(
    JobRun,
    String, // job_name
    i64,    // run_number
    bool,   // is_dag
    Vec<TaskRunItem>,
    String,       // status_ja
    &'static str, // status_badge_class
    String,       // archive_state_label
    &'static str, // archive_state_badge_class
    String,       // trigger_ja
    String,       // duration_str
    String,       // started_at_str
    String,       // finished_at_str
    String,       // dag_tasks_json
    String,       // dag_edges_json
    String,       // task_runs_json
    String,       // config_payload_json
)> {
    let run = crate::db::runs::get_run_by_number(pool, &job_id, run_number)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Run not found"))?;

    fetch_run_detail_data(pool, run.id).await
}

async fn run_detail_by_number(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path((job_name, run_number)): Path<(String, i64)>,
) -> impl IntoResponse {
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &job_name)
        .await
        .unwrap_or(None);
    let job_id = match job_opt {
        Some(j) => j.id,
        None => return Redirect::to("/jobs").into_response(),
    };

    match fetch_run_detail_data_by_number(&state.db, job_id, run_number).await {
        Ok(data) => RunDetailTemplate {
            is_live_polling: should_live_polling(&data.0),
            use_log_websocket: should_use_log_websocket(&data.0),
            run: data.0,
            job_name: data.1,
            run_number: data.2,
            is_dag: data.3,
            task_runs: data.4,
            status_ja: data.5,
            status_badge_class: data.6,
            archive_state_label: data.7,
            archive_state_badge_class: data.8,
            duration_str: data.10,
            started_at_str: data.11,
            finished_at_str: data.12,
            dag_tasks_json: data.13,
            dag_edges_json: data.14,
            task_runs_json: data.15,
            config_payload_json: data.16,
            is_live_partial: false,
        }
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch run details by number: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal Server Error: {}", e),
            )
                .into_response()
        }
    }
}

async fn run_detail_by_number_live(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path((job_name, run_number)): Path<(String, i64)>,
) -> impl IntoResponse {
    let job_opt = crate::db::jobs::get_job_by_name(&state.db, &job_name)
        .await
        .unwrap_or(None);
    let job_id = match job_opt {
        Some(j) => j.id,
        None => return Redirect::to("/jobs").into_response(),
    };

    match fetch_run_detail_data_by_number(&state.db, job_id, run_number).await {
        Ok(data) => RunDetailLiveTemplate {
            is_live_polling: should_live_polling(&data.0),
            use_log_websocket: should_use_log_websocket(&data.0),
            run: data.0,
            job_name: data.1,
            run_number: data.2,
            is_dag: data.3,
            task_runs: data.4,
            status_ja: data.5,
            status_badge_class: data.6,
            archive_state_label: data.7,
            archive_state_badge_class: data.8,
            duration_str: data.10,
            started_at_str: data.11,
            finished_at_str: data.12,
            dag_tasks_json: data.13,
            dag_edges_json: data.14,
            task_runs_json: data.15,
            config_payload_json: data.16,
            is_live_partial: true,
        }
        .into_response(),
        Err(e) => {
            tracing::error!(
                "Failed to fetch run details by number for live polling: {}",
                e
            );
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal Server Error: {}", e),
            )
                .into_response()
        }
    }
}

async fn run_logs_ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, id))
}

async fn load_initial_run_logs(state: &AppState, run: &JobRun) -> anyhow::Result<Vec<LogLine>> {
    if run.log_archive_status == Some(LogArchiveStatus::Archived) {
        match crate::log_archive::archive_store_for_run(&state.config, run) {
            Ok(store) => match store.get_run_logs(run).await {
                Ok(logs) => return Ok(logs),
                Err(e) => {
                    tracing::error!("Failed to get archived logs for run {}: {}", run.id, e);
                }
            },
            Err(e) => {
                tracing::error!(
                    "Failed to resolve archive store for run {}: {}. Falling back to hot logs.",
                    run.id,
                    e
                );
            }
        }
    }

    crate::db::logs::get_logs(&state.db, &run.id).await
}

async fn run_logs_json(
    _claims: WebClaims,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let run = match crate::db::runs::get_run(&state.db, &id).await {
        Ok(Some(run)) => run,
        Ok(None) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(Vec::<LogLine>::new()),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to get run {} for log json: {}", id, e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<LogLine>::new()),
            )
                .into_response();
        }
    };

    match load_initial_run_logs(&state, &run).await {
        Ok(logs) => Json(logs).into_response(),
        Err(e) => {
            tracing::error!("Failed to load initial logs for run {}: {}", id, e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<LogLine>::new()),
            )
                .into_response()
        }
    }
}

fn map_row_to_log(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<LogLine> {
    let id_u64: u64 = row.try_get("id")?;
    let id = id_u64 as i64;
    let run_id: i64 = row.try_get("job_run_id")?;

    let task_name: Option<String> = row.try_get("task_name")?;
    let stream_str: String = row.try_get("stream")?;
    let stream = LogStream::from_str(&stream_str)
        .map_err(|e| anyhow::anyhow!("Invalid LogStream: {}", e))?;
    let line: String = row.try_get("line")?;
    let logged_at: DateTime<Utc> = row.try_get("logged_at")?;

    Ok(LogLine {
        id: Some(id),
        run_id,
        task_name,
        stream,
        line,
        logged_at,
    })
}

async fn handle_socket(mut socket: WebSocket, state: AppState, run_id: i64) {
    let mut last_log_id: u64 = 0;
    let initial_run = match crate::db::runs::get_run(&state.db, &run_id).await {
        Ok(run) => run,
        Err(e) => {
            tracing::error!(
                "Failed to get run before log streaming for run {}: {}",
                run_id,
                e
            );
            None
        }
    };

    // 1. Fetch initial logs
    let initial_logs = match &initial_run {
        Some(run) => load_initial_run_logs(&state, run).await,
        None => crate::db::logs::get_logs(&state.db, &run_id).await,
    };

    match initial_logs {
        Ok(logs) => {
            if !logs.is_empty() {
                // Find maximum id to set last_log_id
                for log in &logs {
                    if let Some(id) = log.id
                        && id as u64 > last_log_id
                    {
                        last_log_id = id as u64;
                    }
                }

                // Serialize and send
                if let Ok(json_str) = serde_json::to_string(&logs)
                    && socket.send(Message::Text(json_str.into())).await.is_err()
                {
                    return;
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to get initial logs for run {}: {}", run_id, e);
        }
    }

    if matches!(
        initial_run
            .as_ref()
            .and_then(|run| run.log_archive_status.clone()),
        Some(LogArchiveStatus::Archived)
    ) {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    // 2. Poll for new logs
    loop {
        // Wait 1 second
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Fetch run status from db to see if it finished
        let run_opt = match crate::db::runs::get_run(&state.db, &run_id).await {
            Ok(opt) => opt,
            Err(e) => {
                tracing::error!("Failed to get run status for logs ws: {}", e);
                None
            }
        };

        let is_terminal = match &run_opt {
            Some(run) => run.status.is_terminal(),
            None => true, // If run is deleted or not found, treat as terminal to exit
        };

        let new_logs_rows_res = sqlx::query(
            "SELECT * FROM job_logs WHERE job_run_id = ? AND id > ? ORDER BY logged_at ASC, id ASC",
        )
        .bind(run_id.to_string())
        .bind(last_log_id)
        .fetch_all(&state.db)
        .await;

        match new_logs_rows_res {
            Ok(rows) => {
                let mut new_logs = Vec::new();
                for r in rows {
                    if let Ok(log) = map_row_to_log(&r) {
                        if let Some(id) = log.id
                            && id as u64 > last_log_id
                        {
                            last_log_id = id as u64;
                        }
                        new_logs.push(log);
                    }
                }

                if !new_logs.is_empty()
                    && let Ok(json_str) = serde_json::to_string(&new_logs)
                    && socket.send(Message::Text(json_str.into())).await.is_err()
                {
                    return; // Client disconnected
                }
            }
            Err(e) => {
                tracing::error!("Failed to query new logs: {}", e);
            }
        }

        // If the job run is in terminal status, we can exit the loop
        if is_terminal {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = socket.send(Message::Close(None)).await;
            break;
        }
    }
}
