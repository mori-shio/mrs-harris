use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use mrs_harris_common::models::run::{LogLine, LogStream};

use crate::log_ingestion::locator::CloudWatchLogTarget;
use crate::log_ingestion::{LogBatch, LogSource};

type LogSourceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudWatchEventRecord {
    pub event_id: String,
    pub message: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone)]
pub struct CloudWatchLogSource {
    pub run_id: i64,
    pub target: CloudWatchLogTarget,
    pub next_token: Option<String>,
    pub exhausted: bool,
}

impl CloudWatchLogSource {
    pub fn new(run_id: i64, target: CloudWatchLogTarget) -> Self {
        Self {
            run_id,
            target,
            next_token: None,
            exhausted: false,
        }
    }

    pub fn map_events_to_batch(
        run_id: i64,
        next_token: Option<String>,
        events: Vec<CloudWatchEventRecord>,
    ) -> LogBatch {
        let has_more = next_token.is_some();
        let lines = events
            .into_iter()
            .map(|event| LogLine {
                id: None,
                run_id,
                task_name: None,
                stream: LogStream::Stdout,
                line: event.message,
                logged_at: Utc
                    .timestamp_millis_opt(event.timestamp_ms)
                    .single()
                    .unwrap_or_else(Utc::now),
            })
            .collect();

        LogBatch {
            lines,
            cursor: next_token,
            has_more,
        }
    }
}

impl LogSource for CloudWatchLogSource {
    fn next_batch(&mut self) -> LogSourceFuture<'_, LogBatch> {
        self.exhausted = true;
        let cursor = self.next_token.clone();

        Box::pin(async move {
            Ok(LogBatch {
                lines: Vec::new(),
                cursor,
                has_more: false,
            })
        })
    }

    fn is_exhausted(&self) -> LogSourceFuture<'_, bool> {
        let exhausted = self.exhausted;
        Box::pin(async move { Ok(exhausted) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cloudwatch_log_source_maps_events_to_log_lines() {
        let target = CloudWatchLogTarget {
            region: "ap-northeast-1".to_string(),
            log_group_name: "/aws/test/group".to_string(),
            log_stream_name: Some("stream-1".to_string()),
            stream_prefix: None,
            start_time_hint_ms: Some(1_700_000_000_000),
        };

        let events = vec![CloudWatchEventRecord {
            event_id: "evt-1".to_string(),
            message: "hello".to_string(),
            timestamp_ms: 1_700_000_000_100,
        }];

        let batch =
            CloudWatchLogSource::map_events_to_batch(42, Some("token-1".to_string()), events);

        assert_eq!(target.log_group_name, "/aws/test/group");
        assert_eq!(batch.lines.len(), 1);
        assert_eq!(batch.lines[0].run_id, 42);
        assert_eq!(batch.lines[0].line, "hello");
        assert_eq!(batch.lines[0].stream, LogStream::Stdout);
        assert_eq!(batch.cursor.as_deref(), Some("token-1"));
        assert!(batch.has_more);
    }

    #[test]
    fn cloudwatch_log_source_falls_back_to_now_for_invalid_timestamp() {
        let before = Utc::now();
        let batch = CloudWatchLogSource::map_events_to_batch(
            7,
            None,
            vec![CloudWatchEventRecord {
                event_id: "evt-2".to_string(),
                message: "bad-ts".to_string(),
                timestamp_ms: i64::MAX,
            }],
        );
        let after = Utc::now();

        assert_eq!(batch.lines.len(), 1);
        assert!(batch.lines[0].logged_at >= before);
        assert!(batch.lines[0].logged_at <= after);
        assert!(!batch.has_more);
    }
}
