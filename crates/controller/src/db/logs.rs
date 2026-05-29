use mrs_harris_common::models::run::{LogLine, LogStream};
use sqlx::{MySqlPool, Row};

use chrono::{DateTime, Utc};
use std::str::FromStr;

use crate::log_ingestion::AppendResult;

const INSERT_JOB_LOG_SQL: &str = r#"INSERT INTO job_logs (job_run_id, task_name, stream, line, logged_at, external_event_id)
       VALUES (?, ?, ?, ?, ?, ?)"#;

#[derive(Debug, Clone)]
pub struct DbLogInsert {
    pub run_id: i64,
    pub task_name: Option<String>,
    pub stream: String,
    pub line: String,
    pub logged_at: chrono::DateTime<chrono::Utc>,
    pub external_event_id: Option<String>,
}

fn map_row_to_log(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<LogLine> {
    let id_u64: u64 = row.try_get("id")?;
    let id = id_u64 as i64;

    let run_id: i64 = row.try_get("job_run_id")?;

    let task_name: Option<String> = row.try_get("task_name")?;

    let stream_str: String = row.try_get("stream")?;
    let stream = LogStream::from_str(&stream_str)
        .map_err(|e| anyhow::anyhow!("Invalid LogStream: {}", e))?;

    let line: String = row.try_get("line")?;
    let logged_at: DateTime<Utc> = row.try_get("logged_at")?;

    Ok(LogLine {
        id: Some(id),
        run_id,
        task_name,
        stream,
        line,
        logged_at,
    })
}

fn to_db_log_insert(log: &LogLine) -> DbLogInsert {
    DbLogInsert {
        run_id: log.run_id,
        task_name: log.task_name.clone(),
        stream: log.stream.to_string(),
        line: log.line.clone(),
        logged_at: log.logged_at,
        external_event_id: None,
    }
}

fn is_duplicate_key_error(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db_err) if db_err.is_unique_violation())
}

/// ログ行を一件追加
pub async fn append_log_line(pool: &MySqlPool, log: &LogLine) -> anyhow::Result<()> {
    let batch = [to_db_log_insert(log)];
    append_log_batch(pool, &batch).await?;
    Ok(())
}

/// 複数ログ行をまとめて追加（MySQL バルクインサート）
pub async fn append_log_lines(pool: &MySqlPool, logs: &[LogLine]) -> anyhow::Result<()> {
    let batch: Vec<DbLogInsert> = logs.iter().map(to_db_log_insert).collect();
    append_log_batch(pool, &batch).await?;
    Ok(())
}

/// 複数ログ行をまとめて追加し、重複スキップ数も返す
pub async fn append_log_batch(
    pool: &MySqlPool,
    batch: &[DbLogInsert],
) -> anyhow::Result<AppendResult> {
    if batch.is_empty() {
        return Ok(AppendResult::default());
    }

    let mut tx = pool.begin().await?;
    let mut result = AppendResult::default();

    for item in batch {
        let insert_result = sqlx::query(INSERT_JOB_LOG_SQL)
            .bind(item.run_id)
            .bind(&item.task_name)
            .bind(&item.stream)
            .bind(&item.line)
            .bind(item.logged_at)
            .bind(&item.external_event_id)
            .execute(&mut *tx)
            .await;

        match insert_result {
            Ok(_) => result.inserted += 1,
            Err(err) if is_duplicate_key_error(&err) => result.skipped_duplicates += 1,
            Err(err) => return Err(err.into()),
        }
    }

    tx.commit().await?;
    Ok(result)
}

/// 実行ログを取得
pub async fn get_logs(pool: &MySqlPool, run_id: &i64) -> anyhow::Result<Vec<LogLine>> {
    let rows =
        sqlx::query("SELECT * FROM job_logs WHERE job_run_id = ? ORDER BY logged_at ASC, id ASC")
            .bind(run_id)
            .fetch_all(pool)
            .await?;

    let mut logs = Vec::new();
    for r in rows {
        logs.push(map_row_to_log(&r)?);
    }
    Ok(logs)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sqlx::error::{DatabaseError, ErrorKind};
    use std::borrow::Cow;
    use std::error::Error as StdError;
    use std::fmt;

    use super::*;

    #[test]
    fn db_log_insert_keeps_run_id_and_external_event_id() {
        let insert = DbLogInsert {
            run_id: 101,
            task_name: None,
            stream: "stdout".to_string(),
            line: "hello".to_string(),
            logged_at: Utc::now(),
            external_event_id: Some("evt-1".to_string()),
        };

        assert_eq!(insert.run_id, 101);
        assert_eq!(insert.external_event_id.as_deref(), Some("evt-1"));
    }

    #[test]
    fn duplicate_database_errors_are_classified_as_skipped() {
        let err = sqlx::Error::Database(Box::new(FakeDatabaseError::unique("duplicate key")));

        assert!(is_duplicate_key_error(&err));
    }

    #[test]
    fn non_duplicate_database_errors_are_not_classified_as_skipped() {
        let err = sqlx::Error::Database(Box::new(FakeDatabaseError::other("other db error")));

        assert!(!is_duplicate_key_error(&err));
    }

    #[derive(Debug)]
    struct FakeDatabaseError {
        message: &'static str,
        unique: bool,
    }

    impl FakeDatabaseError {
        fn unique(message: &'static str) -> Self {
            Self {
                message,
                unique: true,
            }
        }

        fn other(message: &'static str) -> Self {
            Self {
                message,
                unique: false,
            }
        }
    }

    impl fmt::Display for FakeDatabaseError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl StdError for FakeDatabaseError {}

    impl DatabaseError for FakeDatabaseError {
        fn message(&self) -> &str {
            self.message
        }

        fn code(&self) -> Option<Cow<'_, str>> {
            None
        }

        fn as_error(&self) -> &(dyn StdError + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn StdError + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn StdError + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> ErrorKind {
            if self.unique {
                ErrorKind::UniqueViolation
            } else {
                ErrorKind::Other
            }
        }
    }
}
