use crate::app::AppState;
use mrs_harris_common::models::run::JobRun;

pub async fn launch(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    let task_id = run.id;
    let callback_url = state.config.server.external_url.clone();
    
    // 現在の設計ではAPIキーを特に持たせていないが、将来的に追加可能なように None とする
    let api_key = None; 

    tracing::info!(
        task_id = %task_id,
        "Launching Controller-local worker as a background task"
    );

    // Controllerの中でtokioのバックグラウンドタスクとしてworkerプロセスを直接呼び出す
    tokio::spawn(async move {
        if let Err(e) = mrs_harris_worker::run_worker(task_id, callback_url, api_key).await {
            tracing::error!("Local worker failed for task {}: {}", task_id, e);
        }
    });

    let external_id = format!("controller-local-{}", task_id);
    Ok(external_id)
}
