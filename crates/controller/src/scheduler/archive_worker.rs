use crate::app::AppState;
use crate::log_archive::{LogArchiveStore, local_store_from_config};

pub async fn archive_terminal_runs(state: &AppState) -> anyhow::Result<()> {
    let store = local_store_from_config(&state.config);
    let mut processed = 0usize;

    while let Some(run) = crate::db::runs::claim_terminal_run_for_archival(&state.db).await? {
        archive_claimed_run(state, &store, run).await?;
        processed += 1;
    }

    tracing::info!(processed, "Completed archive worker iteration");

    Ok(())
}

async fn archive_claimed_run(
    state: &AppState,
    store: &impl LogArchiveStore,
    run: mrs_harris_common::models::run::JobRun,
) -> anyhow::Result<()> {
    tracing::info!(run_id = run.id, "Claimed terminal run for archival");
    let logs = crate::db::logs::get_logs(&state.db, &run.id).await?;

    let put_result = match store.put_run_logs(&run, &logs).await {
        Ok(result) => result,
        Err(err) => {
            crate::db::runs::mark_run_log_archive_failed(&state.db, run.id).await?;
            return Err(err.context(format!("failed to archive run {}", run.id)));
        }
    };

    crate::db::runs::mark_run_log_archive_success(&state.db, run.id, &put_result).await?;
    crate::db::logs::delete_logs_for_run(&state.db, &run.id).await?;

    tracing::info!(
        run_id = run.id,
        key = %put_result.key,
        line_count = put_result.line_count,
        "Archived terminal run logs"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use chrono::Utc;
    use mrs_harris_common::config::ControllerConfig;
    use mrs_harris_common::models::job::WorkerType;
    use mrs_harris_common::models::run::{
        LogArchiveStatus, LogLine, LogStream, NewRun, RunStatus, TriggerType,
    };
    use sqlx::Row;
    use std::path::PathBuf;
    use std::sync::Arc;
    use uuid::Uuid;

    #[tokio::test]
    #[ignore = "requires local MySQL controller config"]
    async fn archives_terminal_run_into_local_file_store() {
        let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../config/controller.toml")
            .canonicalize()
            .unwrap();
        let mut config = ControllerConfig::from_file(&config_path).unwrap();
        let archive_dir =
            std::env::temp_dir().join(format!("mrs-harris-archive-worker-{}", Uuid::new_v4()));
        config.log_archive.local_file_base_dir = archive_dir.to_string_lossy().into_owned();

        let pool = crate::db::create_pool(&config.database).await.unwrap();
        let job_id: i64 = sqlx::query("SELECT id FROM jobs ORDER BY id ASC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("id")
            .unwrap();

        let new_run = NewRun {
            job_id,
            worker_type: WorkerType::Fargate,
            trigger_type: TriggerType::Manual,
            scheduled_at: None,
            worker_definition_id: None,
        };
        let run = crate::db::runs::create_run(&pool, &new_run).await.unwrap();
        crate::db::runs::update_run_status(
            &pool,
            &run.id,
            RunStatus::Succeeded,
            None,
            None,
            None,
            Some(1234),
        )
        .await
        .unwrap();

        let log = LogLine {
            id: None,
            run_id: run.id,
            task_name: None,
            stream: LogStream::Stdout,
            line: "archive-me".to_string(),
            logged_at: Utc::now(),
        };
        crate::db::logs::append_log_line(&pool, &log).await.unwrap();

        let state = AppState {
            db: pool.clone(),
            config: Arc::new(config.clone()),
            scheduler_instance_id: Uuid::new_v4().to_string(),
        };

        let claimed_run = crate::db::runs::get_run(&pool, &run.id)
            .await
            .unwrap()
            .unwrap();
        let store = crate::log_archive::LocalFileLogArchiveStore::new(&archive_dir);
        archive_claimed_run(&state, &store, claimed_run)
            .await
            .unwrap();

        let archived_run = crate::db::runs::get_run(&pool, &run.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            archived_run.log_archive_status,
            Some(LogArchiveStatus::Archived)
        );
        assert_eq!(archived_run.log_line_count, Some(1));
        assert_eq!(
            archived_run.log_archive_store,
            Some(config.log_archive.store)
        );

        let remaining_logs = crate::db::logs::get_logs(&pool, &run.id).await.unwrap();
        assert!(remaining_logs.is_empty());

        let archived_key = archived_run.log_archive_key.unwrap();
        let archived_path = archive_dir.join(archived_key);
        assert!(tokio::fs::try_exists(&archived_path).await.unwrap());

        let _ = tokio::fs::remove_file(&archived_path).await;
        let _ = tokio::fs::remove_dir_all(&archive_dir).await;
    }
}
