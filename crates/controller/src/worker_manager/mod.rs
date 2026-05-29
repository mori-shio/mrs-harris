use crate::app::AppState;
use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::run::JobRun;

pub mod controller_worker;
pub mod fargate;
pub mod lambda;

/// ワーカーを起動してジョブを実行
pub async fn launch_worker(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    match run.worker_type {
        WorkerType::Fargate => fargate::launch(state, run).await,
        WorkerType::Lambda => lambda::launch(state, run).await,
        WorkerType::Controller => {
            if !state.config.controller_worker.enabled {
                anyhow::bail!(
                    "controller worker type is disabled by configuration; use lambda or fargate"
                );
            }
            controller_worker::launch(state, run).await
        }
    }
}
