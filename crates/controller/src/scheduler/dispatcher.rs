use crate::app::AppState;
use mrs_harris_common::models::run::RunStatus;

/// 待機中のジョブ実行を検知してワーカーへディスパッチする
pub async fn dispatch_pending_runs(state: &AppState) -> anyhow::Result<()> {
    // pending 状態のものをアトミックに取得して queued にする
    while let Some(run) = crate::db::runs::claim_pending_run(&state.db).await? {
        tracing::info!("Dispatching run {} for job_id {}", run.id, run.job_id);

        // ジョブ定義を取得
        let job_opt = crate::db::jobs::get_job(&state.db, &run.job_id).await?;
        let job = match job_opt {
            Some(j) => j,
            None => {
                tracing::error!("Job not found for run {}", run.id);
                continue;
            }
        };

        // ジョブタイプが DAG の場合、DAG 実行エンジンを起動
        if job.job_type == mrs_harris_common::models::job::JobType::Dag {
            tracing::info!("Starting DAG job run {}", run.id);
            // ステータスを running に更新
            if let Err(e) = crate::db::runs::update_run_status(
                &state.db,
                &run.id,
                RunStatus::Running,
                None,
                None,
                None,
                None,
                
            )
            .await {
                tracing::error!("Failed to update run status to running for DAG run {}: {}", run.id, e);
                continue;
            }

            // 最初のタスク群を解決してディスパッチ
            let state_clone = state.clone();
            let run_id = run.id;
            tokio::spawn(async move {
                if let Err(e) = crate::scheduler::dag_engine::resolve_and_dispatch(state_clone, run_id).await {
                    tracing::error!("Failed to resolve and dispatch DAG initially for run {}: {}", run_id, e);
                }
            });
            continue;
        }

        // ワーカーをデータベースに登録
        let def_id = job.worker_definition_id.unwrap_or(1);
        let worker_res = crate::db::workers::register_worker(
            &state.db,
            &def_id,
            job.worker_type.clone(),
            None,
            &run.id,
            &serde_json::json!({}),
        )
        .await;

        let worker = match worker_res {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to register worker in database for run {}: {}", run.id, e);
                continue;
            }
        };

        // ローカルで worker_id を設定した状態で起動する
        let mut run_clone = run.clone();
        run_clone.worker_id = Some(worker.id);

        // ワーカーを非同期で起動
        match crate::worker_manager::launch_worker(state, &run_clone).await {
            Ok(external_id) => {
                tracing::info!("Successfully launched worker for run {}. External ID: {}", run.id, external_id);
                // workers テーブルの external_id を更新
                let _ = crate::db::workers::update_worker_external_id(&state.db, &worker.id, &external_id).await;

                // ステータスを running に更新、および worker_id に BIGINT を設定
                if let Err(e) = crate::db::runs::update_run_status(
                    &state.db,
                    &run.id,
                    RunStatus::Running,
                    Some(worker.id),
                    None,
                    None,
                    None,
                )
                .await {
                    tracing::error!("Failed to update run status to running for run {}: {}", run.id, e);
                }
            }
            Err(err) => {
                tracing::error!("Failed to launch worker for run {}: {}", run.id, err);
                
                // workers テーブルのステータスを failed に更新
                let _ = crate::db::workers::update_worker_status(
                    &state.db,
                    &worker.id,
                    mrs_harris_common::models::worker::WorkerStatus::Failed,
                )
                .await;

                // 起動失敗として Failed に落とす
                let error_msg = format!("Failed to launch worker: {}", err);
                if let Err(e) = crate::db::runs::update_run_status(
                    &state.db,
                    &run.id,
                    RunStatus::Failed,
                    Some(worker.id),
                    Some(&error_msg),
                    None,
                    None,
                )
                .await {
                    tracing::error!("Failed to update run status to failed for run {}: {}", run.id, e);
                } else {
                    let _ = crate::notification::trigger_notifications(state, &run.id, "failed").await;
                }
            }
        }
    }

    Ok(())
}
