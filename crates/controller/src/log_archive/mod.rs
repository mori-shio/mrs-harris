#![allow(dead_code)]

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::{Context, Result};
use chrono::Utc;
use mrs_harris_common::config::ControllerConfig;
use mrs_harris_common::models::run::{JobRun, LogArchiveStore as LogArchiveStoreKind, LogLine};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

type ArchiveFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePutResult {
    pub store: LogArchiveStoreKind,
    pub key: String,
    pub line_count: i64,
    pub archive_bytes: i64,
    pub archived_at: chrono::DateTime<Utc>,
}

pub trait LogArchiveStore: Send + Sync {
    fn put_run_logs<'a>(
        &'a self,
        run: &'a JobRun,
        logs: &'a [LogLine],
    ) -> ArchiveFuture<'a, ArchivePutResult>;

    fn get_run_logs<'a>(&'a self, run: &'a JobRun) -> ArchiveFuture<'a, Vec<LogLine>>;

    fn delete_run_logs<'a>(&'a self, run: &'a JobRun) -> ArchiveFuture<'a, ()>;
}

#[derive(Debug, Clone)]
pub struct LocalFileLogArchiveStore {
    base_dir: PathBuf,
}

impl LocalFileLogArchiveStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn archive_relative_path(run: &JobRun) -> PathBuf {
        PathBuf::from("job-runs")
            .join(run.job_id.to_string())
            .join(format!("{}.jsonl", run.id))
    }

    fn archive_absolute_path(&self, run: &JobRun) -> PathBuf {
        self.base_dir.join(Self::archive_relative_path(run))
    }

    async fn ensure_parent_dir(path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("archive path has no parent: {}", path.display()))?;
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create archive dir {}", parent.display()))
    }
}

pub fn local_store_from_config(config: &ControllerConfig) -> LocalFileLogArchiveStore {
    LocalFileLogArchiveStore::new(&config.log_archive.local_file_base_dir)
}

impl LogArchiveStore for LocalFileLogArchiveStore {
    fn put_run_logs<'a>(
        &'a self,
        run: &'a JobRun,
        logs: &'a [LogLine],
    ) -> ArchiveFuture<'a, ArchivePutResult> {
        Box::pin(async move {
            let archive_path = self.archive_absolute_path(run);
            Self::ensure_parent_dir(&archive_path).await?;

            let mut file = tokio::fs::File::create(&archive_path)
                .await
                .with_context(|| format!("failed to create {}", archive_path.display()))?;

            let mut archive_bytes = 0_i64;
            for log in logs {
                let mut encoded = serde_json::to_vec(log)
                    .with_context(|| format!("failed to serialize log line for run {}", run.id))?;
                encoded.push(b'\n');
                archive_bytes += encoded.len() as i64;
                file.write_all(&encoded)
                    .await
                    .with_context(|| format!("failed to write {}", archive_path.display()))?;
            }

            file.flush()
                .await
                .with_context(|| format!("failed to flush {}", archive_path.display()))?;

            Ok(ArchivePutResult {
                store: LogArchiveStoreKind::LocalFile,
                key: Self::archive_relative_path(run)
                    .to_string_lossy()
                    .into_owned(),
                line_count: logs.len() as i64,
                archive_bytes,
                archived_at: Utc::now(),
            })
        })
    }

    fn get_run_logs<'a>(&'a self, run: &'a JobRun) -> ArchiveFuture<'a, Vec<LogLine>> {
        Box::pin(async move {
            let archive_path = self.archive_absolute_path(run);
            let file = tokio::fs::File::open(&archive_path)
                .await
                .with_context(|| format!("failed to open {}", archive_path.display()))?;
            let mut reader = BufReader::new(file).lines();
            let mut logs = Vec::new();

            while let Some(line) = reader
                .next_line()
                .await
                .with_context(|| format!("failed to read {}", archive_path.display()))?
            {
                let log = serde_json::from_str::<LogLine>(&line).with_context(|| {
                    format!("failed to parse log line from {}", archive_path.display())
                })?;
                logs.push(log);
            }

            Ok(logs)
        })
    }

    fn delete_run_logs<'a>(&'a self, run: &'a JobRun) -> ArchiveFuture<'a, ()> {
        Box::pin(async move {
            let archive_path = self.archive_absolute_path(run);
            if tokio::fs::try_exists(&archive_path)
                .await
                .with_context(|| format!("failed to stat {}", archive_path.display()))?
            {
                tokio::fs::remove_file(&archive_path)
                    .await
                    .with_context(|| format!("failed to delete {}", archive_path.display()))?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mrs_harris_common::models::job::WorkerType;
    use mrs_harris_common::models::run::{LogStream, RunStatus, TriggerType};
    use uuid::Uuid;

    fn sample_run() -> JobRun {
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 34, 56).unwrap();
        JobRun {
            id: 101,
            job_id: 202,
            run_number: 3,
            status: RunStatus::Succeeded,
            worker_type: WorkerType::Controller,
            worker_id: None,
            trigger_type: TriggerType::Manual,
            attempt: 1,
            scheduled_at: None,
            started_at: Some(now),
            finished_at: Some(now),
            next_retry_at: None,
            duration_ms: Some(1234),
            log_archive_status: None,
            log_archive_store: None,
            log_archive_key: None,
            log_line_count: None,
            log_archive_bytes: None,
            log_archived_at: None,
            output: None,
            error: None,
            job_history_id: Some(1),
            worker_definition_id: Some(2),
            config_version: Some(1),
            created_at: now,
            updated_at: now,
        }
    }

    fn sample_logs() -> Vec<LogLine> {
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 35, 0).unwrap();
        vec![
            LogLine {
                id: None,
                run_id: 101,
                task_name: None,
                stream: LogStream::Stdout,
                line: "hello".to_string(),
                logged_at: now,
            },
            LogLine {
                id: None,
                run_id: 101,
                task_name: None,
                stream: LogStream::System,
                line: "done".to_string(),
                logged_at: now,
            },
        ]
    }

    #[tokio::test]
    async fn local_file_archive_store_round_trips_logs() {
        let dir = std::env::temp_dir().join(format!("mrs-harris-archive-{}", Uuid::new_v4()));
        let store = LocalFileLogArchiveStore::new(&dir);
        let run = sample_run();
        let logs = sample_logs();

        let put = store.put_run_logs(&run, &logs).await.unwrap();
        assert_eq!(put.store, LogArchiveStoreKind::LocalFile);
        assert_eq!(put.key, "job-runs/202/101.jsonl");
        assert_eq!(put.line_count, 2);
        assert!(put.archive_bytes > 0);

        let restored = store.get_run_logs(&run).await.unwrap();
        assert_eq!(restored.len(), logs.len());
        assert_eq!(restored[0].line, "hello");
        assert_eq!(restored[1].stream, LogStream::System);

        store.delete_run_logs(&run).await.unwrap();
        assert!(
            !tokio::fs::try_exists(dir.join("job-runs/202/101.jsonl"))
                .await
                .unwrap()
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
