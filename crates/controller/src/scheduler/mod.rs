use crate::app::AppState;
use std::future::Future;

pub mod archive_worker;
pub mod cron_trigger;
pub mod dag_engine;
pub mod dispatcher;
pub mod reaper;
pub mod retry_manager;

/// スケジューラのメインループ
pub async fn run_scheduler(state: AppState) -> anyhow::Result<()> {
    let poll_interval = std::time::Duration::from_secs(state.config.scheduler.poll_interval_sec);

    tracing::info!("スケジューラを起動しました (間隔: {:?})", poll_interval);

    loop {
        run_scheduler_iteration(&state).await?;
        tokio::time::sleep(poll_interval).await;
    }
}

pub async fn run_scheduler_once(state: AppState) -> anyhow::Result<()> {
    run_scheduler_iteration(&state).await
}

async fn run_scheduler_iteration(state: &AppState) -> anyhow::Result<()> {
    let lease_ttl_seconds = std::cmp::max(state.config.scheduler.poll_interval_sec * 2, 10);

    if let Err(e) = run_with_lease(state, "cron_trigger", lease_ttl_seconds, || {
        cron_trigger::check_cron_jobs(state)
    })
    .await
    {
        tracing::error!("Cron チェックエラー: {}", e);
    }

    if let Err(e) = run_with_lease(state, "retry_manager", lease_ttl_seconds, || {
        retry_manager::check_retries(state)
    })
    .await
    {
        tracing::error!("リトライチェックエラー: {}", e);
    }

    if let Err(e) = dispatcher::dispatch_pending_runs(state).await {
        tracing::error!("ディスパッチャエラー: {}", e);
    }

    if let Err(e) = run_with_lease(state, "reaper", lease_ttl_seconds, || {
        reaper::check_timeouts(state)
    })
    .await
    {
        tracing::error!("リーパーエラー: {}", e);
    }

    if let Err(e) = run_with_lease(state, "log_archive_worker", lease_ttl_seconds, || {
        archive_worker::archive_terminal_runs(state)
    })
    .await
    {
        tracing::error!("ログアーカイブエラー: {}", e);
    }

    Ok(())
}

async fn run_with_lease<F, Fut>(
    state: &AppState,
    lease_name: &str,
    ttl_seconds: u64,
    operation: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let acquired = crate::db::leases::try_acquire_lease(
        &state.db,
        lease_name,
        &state.scheduler_instance_id,
        ttl_seconds,
    )
    .await?;

    if acquired {
        tracing::info!(lease_name, "Scheduler lease acquired");
        operation().await?;
    }

    Ok(())
}
