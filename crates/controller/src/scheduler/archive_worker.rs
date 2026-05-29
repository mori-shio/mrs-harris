use crate::app::AppState;
use crate::log_archive::{LogArchiveStore, local_store_from_config};

pub async fn archive_terminal_runs(state: &AppState) -> anyhow::Result<()> {
    let store = local_store_from_config(&state.config);
    let mut processed = 0usize;

    while let Some(run) = crate::db::runs::claim_terminal_run_for_archival(&state.db).await? {
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
        processed += 1;
    }

    tracing::info!(processed, "Completed archive worker iteration");

    Ok(())
}
