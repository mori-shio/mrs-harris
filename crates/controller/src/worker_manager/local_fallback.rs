use crate::app::AppState;
use mrs_harris_common::models::run::JobRun;
use std::sync::Arc;

pub async fn launch(
    state: &AppState,
    run: &JobRun,
    external_id_prefix: &str,
) -> anyhow::Result<String> {
    let task_id = run.id;
    let callback_url = state.config.server.external_url.clone();
    let db = state.db.clone();

    tracing::info!(
        task_id = %task_id,
        worker_type = %run.worker_type,
        "Launching local fallback worker as a background task"
    );

    tokio::spawn(async move {
        let line_callback: mrs_harris_worker::log_capture::LineCallback = Arc::new(move |line| {
            let db = db.clone();
            Box::pin(async move {
                crate::db::logs::append_log_line(&db, &line).await?;
                Ok(())
            })
        });

        if let Err(e) = mrs_harris_worker::run_worker_with_line_callback(
            task_id,
            callback_url,
            None,
            Some(line_callback),
        )
        .await
        {
            tracing::error!("Local fallback worker failed for task {}: {}", task_id, e);
        }
    });

    Ok(format!("{external_id_prefix}-{}", task_id))
}
