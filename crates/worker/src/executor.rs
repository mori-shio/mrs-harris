use mrs_harris_common::models::job::ShellPayload;
use mrs_harris_common::models::run::{LogLine, LogStream, RunStatus};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// ジョブ実行結果
#[derive(Debug)]
pub struct ExecutionResult {
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub logs: Vec<LogLine>,
    pub error: Option<String>,
    pub duration_ms: i64,
}

/// シェルコマンドを実行
pub async fn execute_shell_command(task_info: &super::reporter::TaskInfo) -> ExecutionResult {
    execute_shell_command_with_line_callback(task_info, None).await
}

/// シェルコマンドを実行し、stdout/stderr を行単位で callback する
pub async fn execute_shell_command_with_line_callback(
    task_info: &super::reporter::TaskInfo,
    line_callback: Option<super::log_capture::LineCallback>,
) -> ExecutionResult {
    let payload: ShellPayload = match serde_json::from_value(task_info.payload.clone()) {
        Ok(p) => p,
        Err(e) => {
            return ExecutionResult {
                status: RunStatus::Failed,
                exit_code: None,
                logs: vec![],
                error: Some(format!("ペイロードのパースに失敗: {}", e)),
                duration_ms: 0,
            };
        }
    };

    let start = std::time::Instant::now();
    let mut logs = Vec::new();

    // コマンド構築
    let mut cmd = Command::new(&payload.command);
    cmd.args(&payload.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(ref dir) = payload.working_dir {
        cmd.current_dir(dir);
    }

    for (key, value) in &payload.env {
        cmd.env(key, value);
    }

    // プロセス起動
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return ExecutionResult {
                status: RunStatus::Failed,
                exit_code: None,
                logs: vec![],
                error: Some(format!("コマンド起動に失敗: {}", e)),
                duration_ms: start.elapsed().as_millis() as i64,
            };
        }
    };

    // stdout/stderr を非同期で読み取り
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let run_id = task_info.run_id;
    let stdout_callback = line_callback.clone();
    let stderr_callback = line_callback;

    let stdout_handle = tokio::spawn(async move {
        let mut lines = Vec::new();
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            lines.extend(read_stream_lines(
                reader,
                run_id,
                LogStream::Stdout,
                stdout_callback,
            )
            .await);
        }
        lines
    });

    let stderr_handle = tokio::spawn(async move {
        let mut lines = Vec::new();
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            lines.extend(read_stream_lines(
                reader,
                run_id,
                LogStream::Stderr,
                stderr_callback,
            )
            .await);
        }
        lines
    });

    // プロセス完了を待機
    let exit_status = child.wait().await;
    let duration_ms = start.elapsed().as_millis() as i64;

    // ログ収集
    if let Ok(stdout_logs) = stdout_handle.await {
        logs.extend(stdout_logs);
    }
    if let Ok(stderr_logs) = stderr_handle.await {
        logs.extend(stderr_logs);
    }

    // ログを時刻順にソート
    logs.sort_by_key(|l| l.logged_at);

    match exit_status {
        Ok(status) => {
            let exit_code = status.code();
            let run_status = if status.success() {
                RunStatus::Succeeded
            } else {
                RunStatus::Failed
            };

            let error_msg = if !status.success() {
                let last_stderr: Vec<String> = logs
                    .iter()
                    .filter(|l| matches!(l.stream, LogStream::Stderr))
                    .rev()
                    .take(5)
                    .map(|l| l.line.clone())
                    .collect();

                let mut msg = format!("終了コード: {:?}", exit_code);
                if !last_stderr.is_empty() {
                    msg.push_str("\n\n[エラー詳細 (Stderr)]\n");
                    for line in last_stderr.into_iter().rev() {
                        msg.push_str(&line);
                        msg.push('\n');
                    }
                } else {
                    let last_stdout: Vec<String> = logs
                        .iter()
                        .filter(|l| matches!(l.stream, LogStream::Stdout))
                        .rev()
                        .take(5)
                        .map(|l| l.line.clone())
                        .collect();
                    if !last_stdout.is_empty() {
                        msg.push_str("\n\n[直前の出力 (Stdout)]\n");
                        for line in last_stdout.into_iter().rev() {
                            msg.push_str(&line);
                            msg.push('\n');
                        }
                    }
                }
                Some(msg.trim_end().to_string())
            } else {
                None
            };

            ExecutionResult {
                status: run_status,
                exit_code,
                logs,
                error: error_msg,
                duration_ms,
            }
        }
        Err(e) => ExecutionResult {
            status: RunStatus::Failed,
            exit_code: None,
            logs,
            error: Some(format!("プロセス待機エラー: {}", e)),
            duration_ms,
        },
    }
}

async fn read_stream_lines<R>(
    mut reader: BufReader<R>,
    run_id: i64,
    stream: LogStream,
    callback: Option<super::log_capture::LineCallback>,
) -> Vec<LogLine>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = Vec::new();
    let mut buffer = Vec::new();

    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer)
                    .trim_end_matches(&['\r', '\n'][..])
                    .to_string();
                let log_line = LogLine {
                    id: None,
                    run_id,
                    task_name: None,
                    stream: stream.clone(),
                    line,
                    logged_at: chrono::Utc::now(),
                };
                if let Some(callback) = &callback
                    && let Err(error) = callback(log_line.clone()).await
                {
                    tracing::warn!(run_id = %run_id, %error, ?stream, "stream line callback failed");
                }
                lines.push(log_line);
            }
            Err(error) => {
                tracing::warn!(run_id = %run_id, %error, ?stream, "failed to read stream line");
                break;
            }
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::super::log_capture::LineCallback;
    use super::*;
    use std::io::Cursor;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn sample_task_info(command: &str, script: &str) -> super::super::reporter::TaskInfo {
        super::super::reporter::TaskInfo {
            run_id: 42,
            job_id: 7,
            payload: serde_json::json!({
                "command": command,
                "args": ["-c", script],
                "working_dir": null,
                "env": {}
            }),
            timeout_sec: 30,
        }
    }

    #[tokio::test]
    async fn execute_shell_command_invokes_line_callback_for_stdout_and_stderr() {
        let task_info = sample_task_info("/bin/sh", "printf 'out\\n'; printf 'err\\n' >&2");
        let callback_lines = Arc::new(Mutex::new(Vec::new()));
        let callback_lines_for_sink = callback_lines.clone();

        let callback: LineCallback = Arc::new(move |line| {
            let callback_lines = callback_lines_for_sink.clone();
            Box::pin(async move {
                callback_lines.lock().await.push(line);
                Ok(())
            })
        });

        let result = execute_shell_command_with_line_callback(&task_info, Some(callback)).await;

        assert_eq!(result.status, RunStatus::Succeeded);
        assert_eq!(result.logs.len(), 2);

        let callback_lines = callback_lines.lock().await;
        assert_eq!(callback_lines.len(), 2);
        assert!(
            callback_lines
                .iter()
                .any(|line| line.stream == LogStream::Stdout && line.line == "out")
        );
        assert!(
            callback_lines
                .iter()
                .any(|line| line.stream == LogStream::Stderr && line.line == "err")
        );
    }

    #[tokio::test]
    async fn read_stream_lines_keeps_following_lines_after_invalid_utf8() {
        let bytes = b"+ echo 'ok'\n\xff\xfe\n+ sleep 20\n".to_vec();
        let reader = BufReader::new(Cursor::new(bytes));

        let lines = read_stream_lines(reader, 42, LogStream::Stderr, None).await;

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].line, "+ echo 'ok'");
        assert!(lines[1].line.contains('\u{FFFD}'));
        assert_eq!(lines[2].line, "+ sleep 20");
    }
}
