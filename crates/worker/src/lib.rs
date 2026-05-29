pub mod executor;
pub mod log_capture;
pub mod reporter;

/// Worker コア実行処理
pub async fn run_worker(
    task_id: i64,
    callback_url: String,
    api_key: Option<String>,
) -> anyhow::Result<()> {
    run_worker_with_line_callback(task_id, callback_url, api_key, None).await
}

/// Worker コア実行処理（逐次ログ callback 付き）
pub async fn run_worker_with_line_callback(
    task_id: i64,
    callback_url: String,
    api_key: Option<String>,
    line_callback: Option<log_capture::LineCallback>,
) -> anyhow::Result<()> {
    tracing::info!(
        task_id = %task_id,
        callback_url = %callback_url,
        "Mrs. Harris Worker コアを起動します"
    );

    // タスク情報を Controller から取得
    let task_info = reporter::fetch_task_info(&callback_url, &task_id).await?;

    // ジョブ実行
    let result =
        executor::execute_shell_command_with_line_callback(&task_info, line_callback).await;

    // 結果を Controller に報告
    reporter::report_result(&callback_url, &task_id, result, api_key.as_deref()).await?;

    tracing::info!(task_id = %task_id, "Worker 実行完了");
    Ok(())
}
