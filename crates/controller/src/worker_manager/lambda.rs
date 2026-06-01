use crate::app::AppState;
use aws_sdk_lambda::Client as LambdaClient;
use aws_sdk_lambda::operation::RequestId;
use aws_sdk_lambda::primitives::Blob;
use aws_sdk_lambda::types::InvocationType;
use mrs_harris_common::models::run::JobRun;

/// Lambda 関数としてジョブを起動
pub async fn launch(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    tracing::info!(run_id = %run.id, "Lambda 関数の起動を開始");

    let mut is_local = state.config.lambda.function_name == "local"
        || state.config.lambda.function_name.is_empty();

    if let Some(def_id) = run.worker_definition_id
        && let Ok(Some(def)) = crate::db::workers::get_worker_definition(&state.db, &def_id).await
        && let Some(val) = def.config.get("function_name").and_then(|v| v.as_str())
    {
        is_local = val == "local" || val.is_empty();
    }

    if !is_local {
        match launch_aws_lambda(state, run).await {
            Ok(req_id) => return Ok(req_id),
            Err(e) => {
                tracing::warn!(
                    "Failed to launch Lambda on AWS: {}. Falling back to local process spawning.",
                    e
                );
            }
        }
    }

    launch_local_process(state, run).await
}

async fn launch_aws_lambda(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    let sdk_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = LambdaClient::new(&sdk_config);

    // デフォルトの設定をロード
    let mut function_name = state.config.lambda.function_name.clone();
    let mut qualifier = state.config.lambda.qualifier.clone();

    // 自作ワーカー定義の設定でオーバーライド
    if let Some(def_id) = run.worker_definition_id
        && let Some(def) = crate::db::workers::get_worker_definition(&state.db, &def_id).await?
    {
        if let Some(val) = def.config.get("function_name").and_then(|v| v.as_str()) {
            function_name = val.to_string();
        }
        if let Some(val) = def.config.get("qualifier").and_then(|v| v.as_str()) {
            qualifier = Some(val.to_string());
        }
    }

    // ジョブ定義を取得して payload を含める
    let job = crate::db::jobs::get_job(&state.db, &run.job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Job definition not found"))?;

    let payload = serde_json::json!({
        "task_id": run.id,
        "callback_url": format!("{}/api/internal/callback", state.config.server.external_url),
        "payload": job.payload,
    });

    let payload_bytes = serde_json::to_vec(&payload)?;

    let mut request = client
        .invoke()
        .function_name(&function_name)
        .invocation_type(InvocationType::Event) // 非同期呼び出し
        .payload(Blob::new(payload_bytes));

    if let Some(ref q) = qualifier {
        request = request.qualifier(q);
    }

    let response = request.send().await?;

    let request_id = response
        .request_id()
        .ok_or_else(|| anyhow::anyhow!("Lambda response has no request ID"))?
        .to_string();

    tracing::info!(
        "Lambda function invoked successfully on AWS. Request ID: {}",
        request_id
    );
    Ok(request_id)
}

async fn launch_local_process(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    crate::worker_manager::local_fallback::launch(state, run, "local-lambda").await
}
