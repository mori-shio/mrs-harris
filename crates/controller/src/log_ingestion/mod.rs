#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use mrs_harris_common::models::run::{JobRun, LogLine};

use crate::app::AppState;

type LogSourceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone, Default)]
pub struct LogBatch {
    pub lines: Vec<LogLine>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppendResult {
    pub inserted: usize,
    pub skipped_duplicates: usize,
}

pub trait LogSource: Send {
    fn next_batch(&mut self) -> LogSourceFuture<'_, LogBatch>;
    fn is_exhausted(&self) -> LogSourceFuture<'_, bool>;
}

#[derive(Debug, Default)]
pub struct RunLogCollector;

impl RunLogCollector {
    pub async fn spawn_for_run(_state: AppState, _run: JobRun) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mrs_harris_common::models::run::{LogLine, LogStream};

    #[test]
    fn log_batch_reports_empty_state() {
        let batch = LogBatch {
            lines: vec![],
            cursor: None,
            has_more: false,
        };
        assert!(batch.lines.is_empty());
        assert!(batch.cursor.is_none());
        assert!(!batch.has_more);
    }

    #[test]
    fn append_result_tracks_inserted_and_skipped_counts() {
        let result = AppendResult {
            inserted: 2,
            skipped_duplicates: 1,
        };
        assert_eq!(result.inserted, 2);
        assert_eq!(result.skipped_duplicates, 1);
    }

    #[test]
    fn normalized_log_line_keeps_stream_and_message() {
        let line = LogLine {
            id: None,
            run_id: 42,
            task_name: None,
            stream: LogStream::Stdout,
            line: "hello".to_string(),
            logged_at: chrono::Utc::now(),
        };
        assert_eq!(line.run_id, 42);
        assert_eq!(line.stream, LogStream::Stdout);
        assert_eq!(line.line, "hello");
    }
}
