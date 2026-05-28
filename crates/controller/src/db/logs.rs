use mrs_harris_common::models::run::{LogLine, LogStream};
use sqlx::{MySqlPool, Row};

use chrono::{DateTime, Utc};
use std::str::FromStr;

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

/// ログ行を一件追加
pub async fn append_log_line(pool: &MySqlPool, log: &LogLine) -> anyhow::Result<()> {
    let stream_str = log.stream.to_string();
    sqlx::query(
        r#"INSERT INTO job_logs (job_run_id, task_name, stream, line, logged_at)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(log.run_id)
    .bind(&log.task_name)
    .bind(stream_str)
    .bind(&log.line)
    .bind(log.logged_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// 複数ログ行をまとめて追加（MySQL バルクインサート）
pub async fn append_log_lines(pool: &MySqlPool, logs: &[LogLine]) -> anyhow::Result<()> {
    if logs.is_empty() {
        return Ok(());
    }

    // MySQL は bulk insert をネイティブサポートしているため、クエリを作成する
    // sqlx の QueryBuilder を使うこともできますが、手動でバインドするシンプルなやり方、
    // あるいは小分けにしてトランザクションで高速インサートするやり方があります。
    // ここではトランザクションでループインサートするか、手動でバルクインサートクエリを組み立てます。
    // トランザクションインサートの方がSQLインジェクションに強く安全で堅牢です。
    let mut tx = pool.begin().await?;

    for log in logs {
        let stream_str = log.stream.to_string();
        sqlx::query(
            r#"INSERT INTO job_logs (job_run_id, task_name, stream, line, logged_at)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(log.run_id)
        .bind(&log.task_name)
        .bind(&stream_str)
        .bind(&log.line)
        .bind(log.logged_at)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
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
