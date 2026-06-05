use crate::app::AppState;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use mrs_harris_common::models::run::{RunStatus, WorkerCallback};
use mrs_harris_common::models::worker::WorkerStatus;

use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/internal/callback", post(worker_callback))
        .route("/internal/task/{id}", axum::routing::get(get_internal_task))
}

async fn get_internal_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 1. まず job_runs から探す
    let run_opt = crate::db::runs::get_run(&state.db, &id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    if let Some(run) = run_opt {
        // ジョブ定義を取得して payload を返す
        let job = crate::db::jobs::get_job(&state.db, &run.job_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Job definition not found" })),
                )
            })?;

        return Ok(Json(serde_json::json!({
            "run_id": run.id,
            "job_id": job.id,
            "payload": job.payload,
            "timeout_sec": job.timeout_sec,
        })));
    }

    // 2. なければ task_runs から探す
    let task_run_row = sqlx::query(
        r#"SELECT tr.id as task_run_id, tr.run_id, dt.payload, dt.timeout_sec, jr.job_id
           FROM task_runs tr
           JOIN job_runs jr ON tr.run_id = jr.id
           JOIN dag_tasks dt ON jr.job_id = dt.dag_id AND tr.task_name = dt.task_name
           WHERE tr.id = ?"#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
        )
    })?;

    if let Some(row) = task_run_row {
        let task_run_id: i64 = row.try_get("task_run_id").unwrap();
        let _run_id: i64 = row.try_get("run_id").unwrap();
        let job_id: i64 = row.try_get("job_id").unwrap();
        let payload_str: String = row.try_get("payload").unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(&payload_str).unwrap_or(serde_json::json!({}));
        let timeout_sec: Option<i32> = row.try_get("timeout_sec").unwrap_or(None);

        return Ok(Json(serde_json::json!({
            "run_id": task_run_id,
            "job_id": job_id,
            "payload": payload,
            "timeout_sec": timeout_sec.unwrap_or(3600),
        })));
    }

    Err((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "Task not found" })),
    ))
}

async fn worker_callback(
    State(state): State<AppState>,
    Json(payload): Json<WorkerCallback>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // 1. 実行レコードを取得
    let run_opt = crate::db::runs::get_run(&state.db, &payload.task_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    if let Some(run) = run_opt {
        // すでに終端状態の場合は何もしない
        if run.status.is_terminal() {
            if let Err(e) =
                crate::scheduler::step_flow_engine::handle_child_run_update(&state, run.id).await
            {
                tracing::error!(
                    "Failed to evaluate StepFlow for terminal child run callback: {}",
                    e
                );
            }
            return Ok(StatusCode::OK);
        }

        // 2. ログの保存
        if !payload.logs.is_empty() {
            crate::db::logs::append_log_lines(&state.db, &payload.logs)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": format!("Failed to save logs: {}", e) })),
                    )
                })?;
        }

        // 3. 実行ステータスの更新
        if payload.status == RunStatus::Failed {
            if let Ok(Some(job)) = crate::db::jobs::get_job(&state.db, &run.job_id).await {
                if run.attempt < job.retry_policy.max_retries {
                    // リトライをスケジュール
                    let next_attempt = run.attempt + 1;
                    let delay = match job.retry_policy.backoff {
                        mrs_harris_common::models::job::BackoffStrategy::Fixed => {
                            job.retry_policy.base_delay_sec
                        }
                        mrs_harris_common::models::job::BackoffStrategy::Linear => {
                            job.retry_policy.base_delay_sec * next_attempt as u64
                        }
                        mrs_harris_common::models::job::BackoffStrategy::Exponential => {
                            job.retry_policy.base_delay_sec * 2u64.pow(run.attempt)
                        }
                    };

                    let next_retry_at =
                        chrono::Utc::now() + chrono::Duration::seconds(delay as i64);
                    tracing::info!(
                        "Scheduling retry for run {} in {} seconds (attempt {})",
                        run.id,
                        delay,
                        next_attempt
                    );

                    crate::db::runs::schedule_retry(
                        &state.db,
                        &payload.task_id,
                        next_retry_at,
                        next_attempt,
                        payload.error.as_deref(),
                    )
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({ "error": format!("Failed to schedule retry: {}", e) })),
                        )
                    })?;
                } else {
                    // 最大リトライに達したため DeadLetter へ移行
                    tracing::info!("Run {} exceeded max retries. Moving to DeadLetter.", run.id);
                    crate::db::runs::move_to_dead_letter(
                        &state.db,
                        &payload.task_id,
                        payload.error.as_deref(),
                    )
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({ "error": format!("Failed to move to dead letter: {}", e) })),
                        )
                    })?;

                    let _ = crate::notification::trigger_notifications(
                        &state,
                        &payload.task_id,
                        "dead_letter",
                    )
                    .await;
                }
            } else {
                // ジョブが取得できない場合、通常のステータス更新
                crate::db::runs::update_run_status(
                    &state.db,
                    &payload.task_id,
                    payload.status.clone(),
                    None,
                    payload.error.as_deref(),
                    payload.output.as_ref(),
                    payload.duration_ms,
                )
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": format!("Failed to update run status: {}", e) })),
                    )
                })?;

                let _ = crate::notification::trigger_notifications(
                    &state,
                    &payload.task_id,
                    &payload.status.to_string(),
                )
                .await;
            }
        } else {
            // 成功などの場合
            crate::db::runs::update_run_status(
                &state.db,
                &payload.task_id,
                payload.status.clone(),
                None,
                payload.error.as_deref(),
                payload.output.as_ref(),
                payload.duration_ms,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": format!("Failed to update run status: {}", e) })),
                )
            })?;

            let _ = crate::notification::trigger_notifications(
                &state,
                &payload.task_id,
                &payload.status.to_string(),
            )
            .await;
        }

        // 4. ワーカー情報のステータスを更新
        if let Some(worker_id) = run.worker_id {
            let worker_status = if payload.status == RunStatus::Succeeded {
                WorkerStatus::Completed
            } else {
                WorkerStatus::Failed
            };
            let _ = crate::db::workers::update_worker_status(&state.db, &worker_id, worker_status)
                .await;
        }

        // 5. ジョブタイプが DAG の場合、DAG実行エンジンを起動して後続タスクを評価・実行する
        if let Ok(Some(_job)) = crate::db::jobs::get_job(&state.db, &run.job_id).await
            && false
            && let Err(e) =
                crate::scheduler::dag_engine::resolve_and_dispatch(state.clone(), run.id).await
        {
            tracing::error!("Failed to resolve and dispatch DAG: {}", e);
        }

        if let Err(e) =
            crate::scheduler::step_flow_engine::handle_child_run_update(&state, run.id).await
        {
            tracing::error!("Failed to evaluate StepFlow after child run update: {}", e);
        }
    } else {
        // 2. なければ task_runs から探す（DAGタスクコールバック）
        if let Err(e) = handle_dag_task_callback(&state, &payload).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("DAG task callback error: {}", e) })),
            ));
        }
    }

    Ok(StatusCode::OK)
}

async fn handle_dag_task_callback(
    state: &AppState,
    payload: &WorkerCallback,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    let task_run_id = payload.task_id;

    // 1. 現在の task_run を取得
    let task_run_opt =
        sqlx::query("SELECT run_id, task_name, attempt, status FROM task_runs WHERE id = ?")
            .bind(task_run_id)
            .fetch_optional(&state.db)
            .await?;

    let (run_id, task_name, attempt, current_status_str) = match task_run_opt {
        Some(row) => {
            let run_id: i64 = row.try_get("run_id")?;
            let t_name: String = row.try_get("task_name")?;
            let att: u32 = row.try_get("attempt")?;
            let stat: String = row.try_get("status")?;
            (run_id, t_name, att, stat)
        }
        None => return Err(anyhow::anyhow!("Task run not found")),
    };

    if current_status_str == "succeeded" || current_status_str == "failed" {
        return Ok(());
    }

    // 2. ログを保存
    if !payload.logs.is_empty() {
        crate::db::logs::append_log_lines(&state.db, &payload.logs).await?;
    }

    // 3. タスク実行ステータスの更新
    let status_str = payload.status.to_string();
    let output_str = payload.output.as_ref().map(|o| o.to_string());

    sqlx::query(
        r#"UPDATE task_runs 
           SET status = ?, finished_at = ?, duration_ms = ?, output = ?, error = ?
           WHERE id = ?"#,
    )
    .bind(status_str)
    .bind(now)
    .bind(payload.duration_ms)
    .bind(output_str)
    .bind(&payload.error)
    .bind(task_run_id)
    .execute(&state.db)
    .await?;

    // 4. もし失敗かつリトライ回数未満なら、リトライをスケジュールする
    let mut is_final_failure = false;
    if payload.status == RunStatus::Failed {
        // dag_tasks からリトライポリシーを取得
        let task_def_opt = sqlx::query("SELECT retry_policy FROM dag_tasks WHERE dag_id = (SELECT job_id FROM job_runs WHERE id = ?) AND task_name = ?")
            .bind(run_id)
            .bind(&task_name)
            .fetch_optional(&state.db)
            .await?;

        let mut max_retries = 0;
        if let Some(row) = task_def_opt {
            let rp_str: Option<String> = row.try_get("retry_policy")?;
            if let Some(rp_json) = rp_str
                && let Ok(rp) =
                    serde_json::from_str::<mrs_harris_common::models::job::RetryPolicy>(&rp_json)
            {
                max_retries = rp.max_retries;
            }
        }

        if attempt < max_retries {
            let next_attempt = attempt + 1;
            tracing::info!("Retrying DAG task {} (attempt {})", task_name, next_attempt);
            // 'queued' に戻して、再度ディスパッチできるようにする（dag_engine::resolve_and_dispatch で拾われる）
            sqlx::query("UPDATE task_runs SET status = 'queued', attempt = ?, finished_at = NULL, duration_ms = NULL, error = NULL WHERE id = ?")
                .bind(next_attempt)
                .bind(task_run_id)
                .execute(&state.db)
                .await?;
        } else {
            is_final_failure = true;
        }
    }

    // 5. グラフの解決と次のタスクのディスパッチ
    if payload.status == RunStatus::Succeeded || is_final_failure {
        crate::scheduler::dag_engine::resolve_and_dispatch(state.clone(), run_id).await?;
    }

    Ok(())
}
