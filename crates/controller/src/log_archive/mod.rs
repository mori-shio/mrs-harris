#![allow(dead_code)]

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::primitives::ByteStream;
use chrono::Utc;
use mrs_harris_common::config::ControllerConfig;
use mrs_harris_common::models::run::{JobRun, LogArchiveStore as LogArchiveStoreKind, LogLine};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::OnceCell;

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

pub type DynLogArchiveStore = Arc<dyn LogArchiveStore>;

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

pub fn archive_store_from_config(config: &ControllerConfig) -> Result<DynLogArchiveStore> {
    match config.log_archive.store {
        LogArchiveStoreKind::LocalFile => Ok(Arc::new(local_store_from_config(config))),
        LogArchiveStoreKind::S3 => Ok(Arc::new(S3LogArchiveStore::from_config(config)?)),
    }
}

#[derive(Debug)]
pub struct S3LogArchiveStore {
    bucket: String,
    prefix: Option<String>,
    region: Option<String>,
    endpoint_url: Option<String>,
    force_path_style: Option<bool>,
    client: OnceCell<aws_sdk_s3::Client>,
}

impl S3LogArchiveStore {
    pub fn from_config(config: &ControllerConfig) -> Result<Self> {
        let bucket = config
            .log_archive
            .s3_bucket
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("log_archive.s3_bucket is required when store = 's3'")
            })?;

        Ok(Self {
            bucket,
            prefix: normalize_prefix(config.log_archive.s3_prefix.clone()),
            region: config.log_archive.s3_region.clone(),
            endpoint_url: config.log_archive.s3_endpoint_url.clone(),
            force_path_style: config.log_archive.s3_force_path_style,
            client: OnceCell::new(),
        })
    }

    fn archive_key(&self, run: &JobRun) -> String {
        let base = LocalFileLogArchiveStore::archive_relative_path(run)
            .to_string_lossy()
            .into_owned();
        match &self.prefix {
            Some(prefix) => format!("{prefix}/{base}"),
            None => base,
        }
    }

    fn effective_force_path_style(&self) -> bool {
        self.force_path_style
            .unwrap_or(self.endpoint_url.as_ref().is_some())
    }

    async fn client(&self) -> Result<&aws_sdk_s3::Client> {
        self.client
            .get_or_try_init(|| async {
                let mut loader = aws_config::defaults(BehaviorVersion::latest());
                if let Some(region) = &self.region {
                    loader = loader.region(aws_config::Region::new(region.clone()));
                }
                let sdk_config = loader.load().await;
                let mut builder = aws_sdk_s3::config::Builder::from(&sdk_config)
                    .force_path_style(self.effective_force_path_style());
                if let Some(endpoint_url) = &self.endpoint_url {
                    builder = builder.endpoint_url(endpoint_url.clone());
                }
                Ok::<aws_sdk_s3::Client, anyhow::Error>(aws_sdk_s3::Client::from_conf(
                    builder.build(),
                ))
            })
            .await
    }
}

impl LogArchiveStore for S3LogArchiveStore {
    fn put_run_logs<'a>(
        &'a self,
        run: &'a JobRun,
        logs: &'a [LogLine],
    ) -> ArchiveFuture<'a, ArchivePutResult> {
        Box::pin(async move {
            let key = self.archive_key(run);
            let client = self.client().await?;
            let mut payload = Vec::new();

            for log in logs {
                let mut encoded = serde_json::to_vec(log)
                    .with_context(|| format!("failed to serialize log line for run {}", run.id))?;
                encoded.push(b'\n');
                payload.extend_from_slice(&encoded);
            }

            client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .content_type("application/x-ndjson")
                .body(ByteStream::from(payload.clone()))
                .send()
                .await
                .with_context(|| {
                    format!(
                        "failed to put archived logs to s3://{}/{}",
                        self.bucket, key
                    )
                })?;

            Ok(ArchivePutResult {
                store: LogArchiveStoreKind::S3,
                key,
                line_count: logs.len() as i64,
                archive_bytes: payload.len() as i64,
                archived_at: Utc::now(),
            })
        })
    }

    fn get_run_logs<'a>(&'a self, run: &'a JobRun) -> ArchiveFuture<'a, Vec<LogLine>> {
        Box::pin(async move {
            let key = self.archive_key(run);
            let client = self.client().await?;
            let response = client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .with_context(|| {
                    format!(
                        "failed to get archived logs from s3://{}/{}",
                        self.bucket, key
                    )
                })?;

            let body = response
                .body
                .collect()
                .await
                .with_context(|| {
                    format!(
                        "failed to read archived logs body from s3://{}/{}",
                        self.bucket, key
                    )
                })?
                .into_bytes();

            let mut logs = Vec::new();
            for line in body.split(|byte| *byte == b'\n') {
                if line.is_empty() {
                    continue;
                }
                let log = serde_json::from_slice::<LogLine>(line).with_context(|| {
                    format!(
                        "failed to parse archived log line from s3://{}/{}",
                        self.bucket, key
                    )
                })?;
                logs.push(log);
            }

            Ok(logs)
        })
    }

    fn delete_run_logs<'a>(&'a self, run: &'a JobRun) -> ArchiveFuture<'a, ()> {
        Box::pin(async move {
            let key = self.archive_key(run);
            let client = self.client().await?;
            client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .with_context(|| {
                    format!(
                        "failed to delete archived logs from s3://{}/{}",
                        self.bucket, key
                    )
                })?;
            Ok(())
        })
    }
}

fn normalize_prefix(prefix: Option<String>) -> Option<String> {
    prefix.and_then(|value| {
        let trimmed = value.trim().trim_matches('/').to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

impl LogArchiveStore for LocalFileLogArchiveStore {
    fn put_run_logs<'a>(
        &'a self,
        run: &'a JobRun,
        logs: &'a [LogLine],
    ) -> ArchiveFuture<'a, ArchivePutResult> {
        Box::pin(async move {
            let archive_path = self.archive_absolute_path(run);
            tracing::info!(
                run_id = run.id,
                archive_path = %archive_path.display(),
                "Writing archived run logs"
            );
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
    use mrs_harris_common::config::LogArchiveConfig;
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

    #[test]
    fn s3_archive_key_uses_optional_prefix() {
        let store = S3LogArchiveStore {
            bucket: "archive-bucket".to_string(),
            prefix: normalize_prefix(Some("prod/logs/".to_string())),
            region: Some("ap-northeast-1".to_string()),
            endpoint_url: None,
            force_path_style: Some(false),
            client: OnceCell::new(),
        };

        assert_eq!(
            store.archive_key(&sample_run()),
            "prod/logs/job-runs/202/101.jsonl"
        );
    }

    #[test]
    fn s3_store_defaults_force_path_style_for_custom_endpoint() {
        let store = S3LogArchiveStore {
            bucket: "archive-bucket".to_string(),
            prefix: None,
            region: Some("us-east-1".to_string()),
            endpoint_url: Some("http://localhost:4566".to_string()),
            force_path_style: None,
            client: OnceCell::new(),
        };

        assert!(store.effective_force_path_style());
    }

    #[test]
    fn archive_store_from_config_selects_s3_store() {
        let config = ControllerConfig {
            server: mrs_harris_common::config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                external_url: "http://localhost:8080".to_string(),
            },
            database: mrs_harris_common::config::DatabaseConfig {
                url: "mysql://example".to_string(),
                max_connections: 5,
            },
            scheduler: Default::default(),
            fargate: mrs_harris_common::config::FargateConfig {
                cluster_arn: "cluster".to_string(),
                task_definition: "task".to_string(),
                subnets: vec![],
                security_groups: vec![],
                container_name: "container".to_string(),
                assign_public_ip: None,
            },
            lambda: mrs_harris_common::config::LambdaConfig {
                function_name: "function".to_string(),
                qualifier: None,
            },
            log_archive: LogArchiveConfig {
                store: LogArchiveStoreKind::S3,
                local_file_base_dir: "data/log-archives".to_string(),
                s3_bucket: Some("archive-bucket".to_string()),
                s3_prefix: Some("prod".to_string()),
                s3_region: Some("us-east-1".to_string()),
                s3_endpoint_url: Some("http://localhost:4566".to_string()),
                s3_force_path_style: Some(true),
            },
            controller_worker: Default::default(),
            notification: Default::default(),
            auth: Default::default(),
        };

        assert!(archive_store_from_config(&config).is_ok());
    }
}
