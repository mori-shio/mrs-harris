use crate::app::AppState;
use crate::worker_manager;
use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::run::{JobRun, RunStatus, TriggerType};

use chrono::Utc;
use sqlx::{MySqlPool, Row};
use std::collections::{HashMap, VecDeque};

use std::future::Future;
use std::pin::Pin;

/// DAG の依存関係を解決し、実行可能なタスクをディスパッチ
pub fn resolve_and_dispatch(
    state: AppState,
    run_id: i64,
) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
    Box::pin(async move {
        let pool = &state.db;

        // 1. job_runs から現在のジョブ実行レコードを取得
        let run_opt = crate::db::runs::get_run(pool, &run_id).await?;
        let run = match run_opt {
            Some(r) => r,
            None => {
                tracing::error!("Job run {} not found", run_id);
                return Ok(());
            }
        };

        // すでに終端状態の場合は何もしない
        if run.status.is_terminal() {
            return Ok(());
        }

        let dag_id = run.job_id;

        // 2. dag_tasks からすべてのタスク定義を取得
        let task_definitions = {
            let tasks_rows =
                sqlx::query("SELECT task_name, worker_type FROM dag_tasks WHERE dag_id = ?")
                    .bind(dag_id)
                    .fetch_all(pool)
                    .await?;

            let mut task_definitions = HashMap::new();
            for row in &tasks_rows {
                let name: String = row.try_get("task_name")?;
                let wt_str: String = row.try_get("worker_type")?;
                let worker_type = wt_str.parse::<WorkerType>().unwrap_or(WorkerType::Fargate);
                task_definitions.insert(name, worker_type);
            }
            task_definitions
        };

        if task_definitions.is_empty() {
            tracing::warn!("DAG {} has no tasks defined. Completing job run.", dag_id);
            // タスクがなければ即成功とする
            complete_dag_run(pool, &run, RunStatus::Succeeded, None).await?;
            return Ok(());
        }

        let total_tasks = task_definitions.len();

        // 3. dag_edges からすべてのエッジ（依存関係）を取得
        let (parents, children) = {
            let edges_rows =
                sqlx::query("SELECT from_task, to_task FROM dag_edges WHERE dag_id = ?")
                    .bind(dag_id)
                    .fetch_all(pool)
                    .await?;

            // 依存元（親）のマップ: to_task -> Vec<from_task>
            let mut parents: HashMap<String, Vec<String>> = HashMap::new();
            // 依存先（子）のマップ: from_task -> Vec<to_task>
            let mut children: HashMap<String, Vec<String>> = HashMap::new();

            for row in &edges_rows {
                let from_task: String = row.try_get("from_task")?;
                let to_task: String = row.try_get("to_task")?;
                parents
                    .entry(to_task.clone())
                    .or_default()
                    .push(from_task.clone());
                children.entry(from_task).or_default().push(to_task);
            }
            (parents, children)
        };

        // 4. task_runs から現在の実行履歴を取得
        let mut task_runs_map = {
            let runs_rows =
                sqlx::query("SELECT id, task_name, status FROM task_runs WHERE run_id = ?")
                    .bind(run_id)
                    .fetch_all(pool)
                    .await?;

            let mut task_runs_map = HashMap::new(); // task_name -> (task_run_id, status_str)
            for row in &runs_rows {
                let id: i64 = row.try_get("id")?;
                let name: String = row.try_get("task_name")?;
                let status: String = row.try_get("status")?;
                task_runs_map.insert(name, (id, status));
            }
            task_runs_map
        };

        // 5. 失敗タスクがある場合、下流のタスクを再帰的に skipped にマークする
        let mut newly_skipped = Vec::new();
        let mut failed_tasks = Vec::new();
        for (name, (_, status)) in &task_runs_map {
            if status == "failed" {
                failed_tasks.push(name.clone());
            }
        }

        // 各失敗タスクについて下流を skipped にマーク
        let mut task_statuses: HashMap<String, String> = task_runs_map
            .iter()
            .map(|(k, (_, v))| (k.clone(), v.clone()))
            .collect();

        for failed_task in failed_tasks {
            let skipped = mark_downstream_skipped(&failed_task, &children, &mut task_statuses);
            newly_skipped.extend(skipped);
        }

        // 新たに skipped と判定されたタスクを DB に登録し、task_runs_map も更新する
        let now = Utc::now();
        for task_name in newly_skipped {
            let result = sqlx::query(
            r#"INSERT INTO task_runs (run_id, task_name, status, attempt, started_at, finished_at, created_at)
               VALUES (?, ?, 'skipped', 1, ?, ?, ?)"#
        )
        .bind(run_id)
        .bind(&task_name)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;

            let tr_id = result.last_insert_id() as i64;
            task_runs_map.insert(task_name.clone(), (tr_id, "skipped".to_string()));
        }

        // 6. 実行可能（Ready）なタスクを判定する（非Sendイテレータの生存期間中にawaitを呼ばないよう回避）
        let mut ready_tasks = Vec::new();
        for (task_name, worker_type) in &task_definitions {
            let (run_id_opt, status) = match task_runs_map.get(task_name) {
                Some((id, s)) => (Some(*id), s.as_str()),
                None => (None, "unexecuted"),
            };

            // 実行候補：未実行、またはすでに queued であるもの
            if status == "unexecuted" || status == "queued" {
                // すべての先行タスクが succeeded であるかチェック
                let mut all_parents_succeeded = true;
                if let Some(parent_list) = parents.get(task_name) {
                    for parent in parent_list {
                        let parent_status = task_runs_map
                            .get(parent)
                            .map(|(_, s)| s.as_str())
                            .unwrap_or("unexecuted");
                        if parent_status != "succeeded" {
                            all_parents_succeeded = false;
                            break;
                        }
                    }
                }

                if all_parents_succeeded {
                    ready_tasks.push((task_name.clone(), run_id_opt, worker_type.clone()));
                }
            }
        }

        // HashMap の非Sendなイテレータから完全に抜けた後、DBへの非同期処理を実行する
        let mut tasks_to_launch = Vec::new();
        for (task_name, run_id_opt, worker_type) in ready_tasks {
            let tr_id = match run_id_opt {
                Some(id) => id,
                None => {
                    let result = sqlx::query(
                        r#"INSERT INTO task_runs (run_id, task_name, status, attempt, created_at)
                       VALUES (?, ?, 'queued', 1, ?)"#,
                    )
                    .bind(run_id)
                    .bind(&task_name)
                    .bind(now)
                    .execute(pool)
                    .await?;
                    result.last_insert_id() as i64
                }
            };
            tasks_to_launch.push((task_name, tr_id, worker_type));
        }

        // 7. 実行可能タスクを非同期起動する
        for (task_name, tr_id, worker_type) in tasks_to_launch {
            let state_clone = state.clone();
            let run_id_clone = run_id;
            let task_name_clone = task_name.clone();
            tokio::spawn(async move {
                if let Err(e) = launch_task_worker(
                    state_clone.clone(),
                    run_id_clone,
                    tr_id,
                    &task_name_clone,
                    worker_type,
                )
                .await
                {
                    tracing::error!(
                        "Failed to launch worker for task {} in DAG run {}: {}",
                        task_name_clone,
                        run_id_clone,
                        e
                    );
                    // 起動失敗時は task_runs を failed にする
                    let now = Utc::now();
                    let error_msg = format!("Failed to launch worker: {}", e);
                    let _ = sqlx::query(
                        r#"UPDATE task_runs 
                       SET status = 'failed', error = ?, finished_at = ?, duration_ms = 0
                       WHERE id = ?"#,
                    )
                    .bind(error_msg)
                    .bind(now)
                    .bind(tr_id)
                    .execute(&state_clone.db)
                    .await;

                    // 失敗したため、DAGの依存関係とスキップを再評価するために resolve_and_dispatch を tokio::spawn 経由で呼び出す（再帰制限回避）
                    let state_clone2 = state_clone.clone();
                    tokio::spawn(async move {
                        let _ = resolve_and_dispatch(state_clone2, run_id_clone).await;
                    });
                }
            });
        }

        // 8. 全体の完了判定
        // 最新のタスクラン情報をロードし直して集計する
        let final_statuses = {
            let final_runs = sqlx::query("SELECT status FROM task_runs WHERE run_id = ?")
                .bind(run_id)
                .fetch_all(pool)
                .await?;

            let mut final_statuses = HashMap::new();
            for row in &final_runs {
                let status: String = row.try_get("status")?;
                *final_statuses.entry(status).or_insert(0) += 1;
            }
            final_statuses
        };

        let succeeded_count = *final_statuses.get("succeeded").unwrap_or(&0);
        let skipped_count = *final_statuses.get("skipped").unwrap_or(&0);
        let failed_count = *final_statuses.get("failed").unwrap_or(&0);
        let running_count = *final_statuses.get("running").unwrap_or(&0)
            + *final_statuses.get("queued").unwrap_or(&0)
            + *final_statuses.get("retrying").unwrap_or(&0);

        if succeeded_count + skipped_count == total_tasks {
            tracing::info!("DAG run {} completed successfully!", run_id);
            complete_dag_run(pool, &run, RunStatus::Succeeded, None).await?;
            let _ = crate::notification::trigger_notifications(&state, &run_id, "succeeded").await;
        } else if running_count == 0 && failed_count > 0 {
            tracing::info!("DAG run {} failed.", run_id);
            let error_msg = format!("DAG failed: {} tasks failed", failed_count);
            complete_dag_run(pool, &run, RunStatus::Failed, Some(&error_msg)).await?;
            let _ = crate::notification::trigger_notifications(&state, &run_id, "failed").await;
        }

        Ok(())
    })
}

fn mark_downstream_skipped(
    failed_task: &str,
    children: &HashMap<String, Vec<String>>,
    task_statuses: &mut HashMap<String, String>,
) -> Vec<String> {
    let mut newly_skipped = Vec::new();
    let mut queue = VecDeque::new();

    if let Some(child_list) = children.get(failed_task) {
        for child in child_list {
            queue.push_back(child.clone());
        }
    }

    while let Some(current) = queue.pop_front() {
        let current_status = task_statuses
            .get(&current)
            .map(|s| s.as_str())
            .unwrap_or("unexecuted");
        if current_status == "unexecuted" {
            task_statuses.insert(current.clone(), "skipped".to_string());
            newly_skipped.push(current.clone());

            if let Some(child_list) = children.get(&current) {
                for child in child_list {
                    queue.push_back(child.clone());
                }
            }
        }
    }

    newly_skipped
}

async fn launch_task_worker(
    state: AppState,
    dag_run_id: i64,
    task_run_id: i64,
    task_name: &str,
    worker_type: WorkerType,
) -> anyhow::Result<()> {
    tracing::info!(
        "Launching DAG task worker for {} (task_run_id: {})",
        task_name,
        task_run_id
    );

    let parent_run = crate::db::runs::get_run(&state.db, &dag_run_id)
        .await
        .ok()
        .flatten();
    let worker_definition_id = parent_run.as_ref().and_then(|r| r.worker_definition_id);
    let job_history_id = parent_run.as_ref().and_then(|r| r.job_history_id);
    let config_version = parent_run.as_ref().and_then(|r| r.config_version);

    // ダミーの JobRun を作成して launch_worker に渡す
    let now = Utc::now();
    let dummy_run = JobRun {
        id: task_run_id,
        job_id: dag_run_id, // 親の job_runs の ID を渡す（ワーカーが親ジョブ定義にアクセスできるように）
        run_number: parent_run.as_ref().map(|r| r.run_number).unwrap_or(0),
        status: RunStatus::Queued,
        worker_type,
        worker_id: None,
        trigger_type: TriggerType::Dependency,
        attempt: 1,
        scheduled_at: Some(now),
        started_at: None,
        finished_at: None,
        next_retry_at: None,
        duration_ms: None,
        log_archive_status: None,
        log_archive_store: None,
        log_archive_key: None,
        log_line_count: None,
        log_archive_bytes: None,
        log_archived_at: None,
        output: None,
        error: None,
        job_history_id,
        worker_definition_id,
        config_version,
        created_at: now,
        updated_at: now,
    };

    let external_id = worker_manager::launch_worker(&state, &dummy_run).await?;

    // 起動に成功したら task_runs を running に更新する
    sqlx::query(
        r#"UPDATE task_runs 
           SET status = 'running', worker_id = ?, started_at = ?
           WHERE id = ?"#,
    )
    .bind(external_id)
    .bind(now)
    .bind(task_run_id.to_string())
    .execute(&state.db)
    .await?;

    Ok(())
}

async fn complete_dag_run(
    pool: &MySqlPool,
    run: &JobRun,
    status: RunStatus,
    error: Option<&str>,
) -> anyhow::Result<()> {
    // 最新のジョブ実行レコードを取得して楽観ロック用のバージョンを得る
    let latest_run_opt = crate::db::runs::get_run(pool, &run.id).await?;
    if let Some(latest) = latest_run_opt {
        if latest.status.is_terminal() {
            return Ok(());
        }

        let duration_ms = latest
            .started_at
            .map(|s| Utc::now().signed_duration_since(s).num_milliseconds());

        crate::db::runs::update_run_status(pool, &run.id, status, None, error, None, duration_ms)
            .await?;
    }
    Ok(())
}
