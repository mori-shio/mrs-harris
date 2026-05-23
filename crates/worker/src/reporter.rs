use mrs_harris_common::models::run::WorkerCallback;
use uuid::Uuid;

/// Controller から取得するタスク情報
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskInfo {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub payload: serde_json::Value,
    pub timeout_sec: u32,
}

/// Controller からタスク情報を取得
pub async fn fetch_task_info(callback_url: &str, task_id: &Uuid) -> anyhow::Result<TaskInfo> {
    let client = reqwest::Client::new();
    let base_url = callback_url
        .strip_suffix("/api/internal/callback")
        .or_else(|| callback_url.strip_suffix("/internal/callback"))
        .or_else(|| callback_url.strip_suffix("/callback"))
        .unwrap_or(callback_url);
    let url = format!("{}/api/internal/task/{}", base_url, task_id);

    let response = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?;

    let task_info: TaskInfo = response.json().await?;
    Ok(task_info)
}

/// 実行結果を Controller に報告
pub async fn report_result(
    callback_url: &str,
    task_id: &Uuid,
    result: super::executor::ExecutionResult,
    api_key: Option<&str>,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = if callback_url.contains("/callback") {
        callback_url.to_string()
    } else {
        let base_url = callback_url
            .strip_suffix("/api/internal/callback")
            .or_else(|| callback_url.strip_suffix("/internal/callback"))
            .unwrap_or(callback_url);
        format!("{}/api/internal/callback", base_url)
    };


    let callback = WorkerCallback {
        task_id: *task_id,
        status: result.status,
        output: result.exit_code.map(|c| serde_json::json!({ "exit_code": c })),
        error: result.error,
        logs: result.logs,
        duration_ms: Some(result.duration_ms),
    };

    let mut request = client.post(&url).json(&callback);

    if let Some(key) = api_key {
        request = request.header("X-API-Key", key);
    }

    request.send().await?.error_for_status()?;

    tracing::info!(task_id = %task_id, "結果を Controller に報告しました");
    Ok(())
}
