use mrs_harris_common::models::job::ShellPayload;
use mrs_harris_common::models::run::{LogLine, LogStream, RunStatus};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

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

    let stdout_handle = tokio::spawn(async move {
        let mut lines = Vec::new();
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut line_reader = reader.lines();
            while let Ok(Some(line)) = line_reader.next_line().await {
                lines.push(LogLine {
                    id: None,
                    run_id,
                    task_name: None,
                    stream: LogStream::Stdout,
                    line,
                    logged_at: chrono::Utc::now(),
                });
            }
        }
        lines
    });

    let stderr_handle = tokio::spawn(async move {
        let mut lines = Vec::new();
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut line_reader = reader.lines();
            while let Ok(Some(line)) = line_reader.next_line().await {
                lines.push(LogLine {
                    id: None,
                    run_id,
                    task_name: None,
                    stream: LogStream::Stderr,
                    line,
                    logged_at: chrono::Utc::now(),
                });
            }
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
            ExecutionResult {
                status: run_status,
                exit_code,
                logs,
                error: if !status.success() {
                    Some(format!("終了コード: {:?}", exit_code))
                } else {
                    None
                },
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
