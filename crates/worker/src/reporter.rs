use mrs_harris_common::models::run::WorkerCallback;

/// Controller から取得するタスク情報
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskInfo {
    pub run_id: i64,
    pub job_id: i64,
    pub payload: serde_json::Value,
    pub timeout_sec: u32,
}

/// Controller からタスク情報を取得
pub async fn fetch_task_info(callback_url: &str, task_id: &i64) -> anyhow::Result<TaskInfo> {
    let client = reqwest::Client::new();
    let base_url = callback_url
        .strip_suffix("/api/internal/callback")
        .or_else(|| callback_url.strip_suffix("/internal/callback"))
        .or_else(|| callback_url.strip_suffix("/callback"))
        .unwrap_or(callback_url);
    let url = format!("{}/api/internal/task/{}", base_url, task_id);

    let response = client.get(&url).send().await?.error_for_status()?;

    let task_info: TaskInfo = response.json().await?;
    Ok(task_info)
}

/// 実行結果を Controller に報告
pub async fn report_result(
    callback_url: &str,
    task_id: &i64,
    result: super::executor::ExecutionResult,
    api_key: Option<&str>,
    include_logs: bool,
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

    let callback = build_worker_callback(*task_id, result, include_logs);

    let mut request = client.post(&url).json(&callback);

    if let Some(key) = api_key {
        request = request.header("X-API-Key", key);
    }

    request.send().await?.error_for_status()?;

    tracing::info!(task_id = %task_id, "結果を Controller に報告しました");
    Ok(())
}

fn build_worker_callback(
    task_id: i64,
    result: super::executor::ExecutionResult,
    include_logs: bool,
) -> WorkerCallback {
    WorkerCallback {
        task_id,
        status: result.status,
        output: result
            .exit_code
            .map(|c| serde_json::json!({ "exit_code": c })),
        error: result.error,
        logs: if include_logs { result.logs } else { vec![] },
        duration_ms: Some(result.duration_ms),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use mrs_harris_common::models::run::{LogLine, LogStream, RunStatus};

    use super::build_worker_callback;

    fn sample_result() -> super::super::executor::ExecutionResult {
        super::super::executor::ExecutionResult {
            status: RunStatus::Succeeded,
            exit_code: Some(0),
            logs: vec![LogLine {
                id: None,
                run_id: 42,
                task_name: None,
                stream: LogStream::Stdout,
                line: "hello".to_string(),
                logged_at: Utc::now(),
            }],
            error: None,
            duration_ms: 12,
        }
    }

    #[test]
    fn build_worker_callback_keeps_logs_when_requested() {
        let callback = build_worker_callback(42, sample_result(), true);

        assert_eq!(callback.task_id, 42);
        assert_eq!(callback.logs.len(), 1);
        assert_eq!(callback.logs[0].line, "hello");
    }

    #[test]
    fn build_worker_callback_omits_logs_when_already_streamed() {
        let callback = build_worker_callback(42, sample_result(), false);

        assert_eq!(callback.task_id, 42);
        assert!(callback.logs.is_empty());
    }
}
