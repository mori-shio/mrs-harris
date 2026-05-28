use crate::app::AppState;

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
        // Cron ジョブのチェック
        if let Err(e) = cron_trigger::check_cron_jobs(&state).await {
            tracing::error!("Cron チェックエラー: {}", e);
        }

        // リトライ待ちジョブのチェック
        if let Err(e) = retry_manager::check_retries(&state).await {
            tracing::error!("リトライチェックエラー: {}", e);
        }

        // ディスパッチャの起動
        if let Err(e) = dispatcher::dispatch_pending_runs(&state).await {
            tracing::error!("ディスパッチャエラー: {}", e);
        }

        // リーパー（タイムアウト検出）
        if let Err(e) = reaper::check_timeouts(&state).await {
            tracing::error!("リーパーエラー: {}", e);
        }

        tokio::time::sleep(poll_interval).await;
    }
}
