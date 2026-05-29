use mrs_harris_common::models::run::{LogLine, LogStream};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type LineCallbackFuture = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
pub type LineCallback = Arc<dyn Fn(LogLine) -> LineCallbackFuture + Send + Sync>;

/// ログバッファ — stdout/stderr をキャプチャして LogLine に変換
pub struct LogCapture {
    pub run_id: i64,
    pub task_name: Option<String>,
    pub lines: Vec<LogLine>,
}

impl LogCapture {
    pub fn new(run_id: i64, task_name: Option<String>) -> Self {
        Self {
            run_id,
            task_name,
            lines: Vec::new(),
        }
    }

    pub fn add_line(&mut self, stream: LogStream, line: String) {
        self.lines.push(LogLine {
            id: None,
            run_id: self.run_id,
            task_name: self.task_name.clone(),
            stream,
            line,
            logged_at: chrono::Utc::now(),
        });
    }

    pub fn into_lines(self) -> Vec<LogLine> {
        self.lines
    }
}
