use crate::app::AppState;
use mrs_harris_common::models::job::WorkerType;
use mrs_harris_common::models::run::JobRun;

pub mod fargate;
pub mod lambda;
pub mod controller_worker;

/// ワーカーを起動してジョブを実行
pub async fn launch_worker(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    match run.worker_type {
        WorkerType::Fargate => fargate::launch(state, run).await,
        WorkerType::Lambda => lambda::launch(state, run).await,
        WorkerType::Controller => controller_worker::launch(state, run).await,
    }
}
