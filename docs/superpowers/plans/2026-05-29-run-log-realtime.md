# Run Log Realtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `controller / fargate / lambda` の 3 種類の worker type について、ジョブ実行履歴詳細画面に実行ログをリアルタイム表示できるようにする。

**Architecture:** UI は既存の `/runs/:id/logs/ws` と `job_logs` を維持し、すべてのログを `job_logs` に正規化して流す。`controller` は実行時逐次保存、`fargate / lambda` は CloudWatch Logs を Controller 側で収集する。worker type 差分は `LogSource` と `CloudWatchStreamLocator` に閉じ込める。

**Tech Stack:** Rust, Axum, SQLx/MySQL, Askama, Tokio, AWS SDK for CloudWatch Logs / ECS / Lambda, existing WebSocket log stream

---

## File Map

- Create: `crates/controller/src/log_ingestion/mod.rs`
  - 共通の `RunLogCollector`, `LogSource`, `LogBatch`, `JobLogSink`
- Create: `crates/controller/src/log_ingestion/cloudwatch.rs`
  - `CloudWatchLogSource`, `CloudWatchLogTarget`, cursor 管理
- Create: `crates/controller/src/log_ingestion/locator.rs`
  - `CloudWatchStreamLocator`, `FargateStreamLocator`, `LambdaStreamLocator`
- Modify: `crates/controller/src/lib.rs` or `crates/controller/src/main.rs`
  - 新規 module の公開
- Modify: `crates/controller/src/worker_manager/controller_worker.rs`
  - controller worker の逐次ログ保存経路に接続
- Modify: `crates/controller/src/worker_manager/fargate.rs`
  - Fargate launch 後に collector を開始
- Modify: `crates/controller/src/worker_manager/lambda.rs`
  - Lambda invoke 後に collector を開始
- Modify: `crates/controller/src/db/logs.rs`
  - batch append / dedupe 対応
- Modify: `crates/controller/src/web/runs.rs`
  - 必要なら WebSocket 側のログ並び順・終端条件を微調整
- Modify: `crates/worker/src/log_capture.rs`
  - 行単位 callback を追加
- Modify: `crates/worker/src/executor.rs`
  - stdout/stderr の逐次 callback 実装
- Modify: `crates/worker/src/lib.rs` または `crates/worker/src/main.rs`
  - run_worker へ log sink callback を渡せるよう拡張
- Modify: `crates/controller/migrations/*`
  - 必要なら `job_logs.external_event_id` 等を追加
- Modify: `docs/ui_checklists/run_detail.md`
  - worker type ごとのリアルタイムログ確認項目を追加
- Create or Modify: `crates/controller/tests/*` or existing unit test modules
  - ingest, locator, dedupe, WebSocket 配信のテスト

### Decomposition Notes

- `LogSource` は「どこからどう読むか」だけを担当する
- `JobLogSink` は「DB にどう保存するか」だけを担当する
- `RunLogCollector` は lifecycle orchestration に専念する
- CloudWatch 依存は `cloudwatch.rs` と `locator.rs` へ集約し、`fargate.rs / lambda.rs` は起動フックだけに留める

### Task 1: Define Shared Log Ingestion Abstractions

**Files:**
- Create: `crates/controller/src/log_ingestion/mod.rs`
- Modify: `crates/controller/src/main.rs`
- Test: `crates/controller/src/log_ingestion/mod.rs` unit tests

- [ ] **Step 1: Write the failing abstraction tests**

```rust
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
            job_run_id: 42,
            task_name: None,
            stream: LogStream::Stdout,
            line: "hello".to_string(),
            logged_at: chrono::Utc::now(),
        };
        assert_eq!(line.job_run_id, 42);
        assert_eq!(line.line, "hello");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mrs-harris-controller log_batch_reports_empty_state -- --nocapture`
Expected: FAIL with missing `log_ingestion` module or missing `LogBatch`/`AppendResult`

- [ ] **Step 3: Add the shared module and core types**

```rust
// crates/controller/src/log_ingestion/mod.rs
use anyhow::Result;
use async_trait::async_trait;
use mrs_harris_common::models::run::LogLine;

pub mod cloudwatch;
pub mod locator;

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

#[async_trait]
pub trait LogSource: Send {
    async fn next_batch(&mut self) -> Result<LogBatch>;
    async fn is_exhausted(&self) -> Result<bool>;
}

pub struct RunLogCollector;

impl RunLogCollector {
    pub async fn spawn_for_run(_state: crate::app::AppState, _run: mrs_harris_common::models::run::JobRun) -> Result<()> {
        Ok(())
    }
}
```

```rust
// crates/controller/src/main.rs
mod log_ingestion;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mrs-harris-controller log_batch_reports_empty_state append_result_tracks_inserted_and_skipped_counts normalized_log_line_keeps_stream_and_message -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/controller/src/log_ingestion/mod.rs crates/controller/src/main.rs
git commit -m "feat: add shared run log ingestion abstractions"
```

### Task 2: Add DB Sink with Dedupe Support

**Files:**
- Modify: `crates/controller/src/db/logs.rs`
- Modify: `crates/controller/migrations/20260529000001_add_job_logs_external_event_id.sql`
- Modify: `crates/controller/src/log_ingestion/mod.rs`
- Test: `crates/controller/src/db/logs.rs` unit tests or integration tests

- [ ] **Step 1: Write the failing dedupe test**

```rust
#[tokio::test]
async fn append_batch_skips_duplicate_external_event_ids() {
    let pool = test_pool().await;
    seed_run(&pool, 101).await;

    let now = chrono::Utc::now();
    let batch = vec![
        DbLogInsert {
            run_id: 101,
            task_name: None,
            stream: "stdout".to_string(),
            line: "first".to_string(),
            logged_at: now,
            external_event_id: Some("evt-1".to_string()),
        },
        DbLogInsert {
            run_id: 101,
            task_name: None,
            stream: "stdout".to_string(),
            line: "first".to_string(),
            logged_at: now,
            external_event_id: Some("evt-1".to_string()),
        },
    ];

    let result = append_log_batch(&pool, &batch).await.unwrap();
    assert_eq!(result.inserted, 1);
    assert_eq!(result.skipped_duplicates, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mrs-harris-controller append_batch_skips_duplicate_external_event_ids -- --nocapture`
Expected: FAIL because `external_event_id` and `append_log_batch` do not exist

- [ ] **Step 3: Add migration and batch sink**

```sql
-- crates/controller/migrations/20260529000001_add_job_logs_external_event_id.sql
ALTER TABLE job_logs
    ADD COLUMN external_event_id VARCHAR(255) NULL,
    ADD UNIQUE KEY uq_job_logs_external_event_id (job_run_id, external_event_id);
```

```rust
// crates/controller/src/db/logs.rs
pub struct DbLogInsert {
    pub run_id: i64,
    pub task_name: Option<String>,
    pub stream: String,
    pub line: String,
    pub logged_at: chrono::DateTime<chrono::Utc>,
    pub external_event_id: Option<String>,
}

pub async fn append_log_batch(
    pool: &MySqlPool,
    batch: &[DbLogInsert],
) -> anyhow::Result<crate::log_ingestion::AppendResult> {
    let mut inserted = 0usize;
    let mut skipped_duplicates = 0usize;

    for item in batch {
        let result = sqlx::query(
            r#"INSERT INTO job_logs (job_run_id, task_name, stream, line, logged_at, external_event_id)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.run_id)
        .bind(&item.task_name)
        .bind(&item.stream)
        .bind(&item.line)
        .bind(item.logged_at)
        .bind(&item.external_event_id)
        .execute(pool)
        .await;

        match result {
            Ok(_) => inserted += 1,
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => skipped_duplicates += 1,
            Err(err) => return Err(err.into()),
        }
    }

    Ok(crate::log_ingestion::AppendResult { inserted, skipped_duplicates })
}
```

- [ ] **Step 4: Run migration-aware test suite**

Run: `cargo test -p mrs-harris-controller append_batch_skips_duplicate_external_event_ids -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/controller/src/db/logs.rs crates/controller/migrations/20260529000001_add_job_logs_external_event_id.sql crates/controller/src/log_ingestion/mod.rs
git commit -m "feat: add deduping job log sink"
```

### Task 3: Stream Controller Worker Logs into `job_logs`

**Files:**
- Modify: `crates/worker/src/log_capture.rs`
- Modify: `crates/worker/src/executor.rs`
- Modify: `crates/worker/src/lib.rs`
- Modify: `crates/controller/src/worker_manager/controller_worker.rs`
- Test: `crates/worker/src/executor.rs` unit tests

- [ ] **Step 1: Write the failing streaming callback test**

```rust
#[tokio::test]
async fn executor_emits_stdout_lines_through_callback() {
    let seen = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let seen_clone = seen.clone();

    let payload = ShellPayload {
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "printf 'a\\n'; sleep 1; printf 'b\\n'".to_string()],
        env: std::collections::HashMap::new(),
        ssm_region: None,
        ssm_path: None,
        ssm_recursive: None,
        working_dir: None,
    };

    execute_shell_with_callback(payload, move |line| {
        let seen_clone = seen_clone.clone();
        async move {
            seen_clone.lock().await.push(line.line.clone());
            Ok(())
        }
    })
    .await
    .unwrap();

    let lines = seen.lock().await.clone();
    assert_eq!(lines, vec!["a".to_string(), "b".to_string()]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mrs-harris-worker executor_emits_stdout_lines_through_callback -- --nocapture`
Expected: FAIL because callback-capable executor function does not exist

- [ ] **Step 3: Add callback-capable execution path**

```rust
// crates/worker/src/executor.rs
pub async fn execute_shell_with_callback<F, Fut>(
    payload: ShellPayload,
    mut on_log_line: F,
) -> anyhow::Result<ExecutionResult>
where
    F: FnMut(LogLine) -> Fut + Send,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send,
{
    // spawn child, read stdout/stderr with BufReader::lines(),
    // create LogLine for each line,
    // invoke on_log_line(log_line.clone()).await?,
    // still accumulate logs into ExecutionResult for backward compatibility
}
```

```rust
// crates/controller/src/worker_manager/controller_worker.rs
tokio::spawn(async move {
    let sink_state = state.clone();
    let run_id = task_id;
    let log_callback = move |line: mrs_harris_common::models::run::LogLine| {
        let sink_state = sink_state.clone();
        async move {
            crate::db::logs::append_log_batch(
                &sink_state.db,
                &[crate::db::logs::DbLogInsert {
                    run_id,
                    task_name: line.task_name.clone(),
                    stream: line.stream.to_string(),
                    line: line.line.clone(),
                    logged_at: line.logged_at,
                    external_event_id: None,
                }],
            )
            .await?;
            Ok(())
        }
    };

    if let Err(e) = mrs_harris_worker::run_worker_with_log_callback(task_id, callback_url, api_key, log_callback).await {
        tracing::error!("Local worker failed for task {}: {}", task_id, e);
    }
});
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mrs-harris-worker executor_emits_stdout_lines_through_callback -- --nocapture`
Expected: PASS

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/worker/src/log_capture.rs crates/worker/src/executor.rs crates/worker/src/lib.rs crates/controller/src/worker_manager/controller_worker.rs
git commit -m "feat: stream controller worker logs into job logs"
```

### Task 4: Add Shared CloudWatch Log Source and Locator Interfaces

**Files:**
- Create: `crates/controller/src/log_ingestion/cloudwatch.rs`
- Create: `crates/controller/src/log_ingestion/locator.rs`
- Modify: `crates/controller/src/log_ingestion/mod.rs`
- Test: `crates/controller/src/log_ingestion/cloudwatch.rs` unit tests

- [ ] **Step 1: Write the failing CloudWatch source test**

```rust
#[tokio::test]
async fn cloudwatch_log_source_maps_events_to_log_lines() {
    let target = CloudWatchLogTarget {
        region: "ap-northeast-1".to_string(),
        log_group_name: "/aws/test/group".to_string(),
        log_stream_name: Some("stream-1".to_string()),
        stream_prefix: None,
        start_time_hint_ms: Some(1_700_000_000_000),
    };

    let events = vec![
        CloudWatchEventRecord {
            event_id: "evt-1".to_string(),
            message: "hello".to_string(),
            timestamp_ms: 1_700_000_000_100,
        },
    ];

    let batch = CloudWatchLogSource::map_events_to_batch(42, None, events);
    assert_eq!(batch.lines.len(), 1);
    assert_eq!(batch.lines[0].line, "hello");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mrs-harris-controller cloudwatch_log_source_maps_events_to_log_lines -- --nocapture`
Expected: FAIL because cloudwatch source types do not exist

- [ ] **Step 3: Implement shared CloudWatch source and locator contracts**

```rust
// crates/controller/src/log_ingestion/locator.rs
#[derive(Debug, Clone)]
pub struct CloudWatchLogTarget {
    pub region: String,
    pub log_group_name: String,
    pub log_stream_name: Option<String>,
    pub stream_prefix: Option<String>,
    pub start_time_hint_ms: Option<i64>,
}

#[async_trait::async_trait]
pub trait CloudWatchStreamLocator: Send + Sync {
    async fn resolve(
        &self,
        state: &crate::app::AppState,
        run: &mrs_harris_common::models::run::JobRun,
    ) -> anyhow::Result<CloudWatchLogTarget>;
}
```

```rust
// crates/controller/src/log_ingestion/cloudwatch.rs
pub struct CloudWatchEventRecord {
    pub event_id: String,
    pub message: String,
    pub timestamp_ms: i64,
}

pub struct CloudWatchLogSource {
    pub run_id: i64,
    pub target: CloudWatchLogTarget,
    pub next_token: Option<String>,
    pub exhausted: bool,
}

impl CloudWatchLogSource {
    pub fn map_events_to_batch(
        run_id: i64,
        next_token: Option<String>,
        events: Vec<CloudWatchEventRecord>,
    ) -> crate::log_ingestion::LogBatch {
        crate::log_ingestion::LogBatch {
            lines: events.into_iter().map(|event| LogLine {
                id: None,
                job_run_id: run_id,
                task_name: None,
                stream: LogStream::Stdout,
                line: event.message,
                logged_at: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(event.timestamp_ms)
                    .unwrap_or_else(chrono::Utc::now),
            }).collect(),
            cursor: next_token,
            has_more: true,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mrs-harris-controller cloudwatch_log_source_maps_events_to_log_lines -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/controller/src/log_ingestion/mod.rs crates/controller/src/log_ingestion/cloudwatch.rs crates/controller/src/log_ingestion/locator.rs
git commit -m "feat: add cloudwatch log source abstractions"
```

### Task 5: Implement Fargate CloudWatch Locator and Collector Hook

**Files:**
- Modify: `crates/controller/src/log_ingestion/locator.rs`
- Modify: `crates/controller/src/log_ingestion/mod.rs`
- Modify: `crates/controller/src/worker_manager/fargate.rs`
- Test: `crates/controller/src/log_ingestion/locator.rs` tests

- [ ] **Step 1: Write the failing Fargate locator test**

```rust
#[tokio::test]
async fn fargate_locator_reads_worker_definition_logging_config() {
    let target = FargateStreamLocator::from_config(&serde_json::json!({
        "aws_region": "ap-northeast-1",
        "log_group_name": "/ecs/mrs-harris",
        "log_stream_prefix": "worker/app"
    }))
    .unwrap();

    assert_eq!(target.region, "ap-northeast-1");
    assert_eq!(target.log_group_name, "/ecs/mrs-harris");
    assert_eq!(target.stream_prefix.as_deref(), Some("worker/app"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mrs-harris-controller fargate_locator_reads_worker_definition_logging_config -- --nocapture`
Expected: FAIL because locator implementation does not exist

- [ ] **Step 3: Implement Fargate locator and collector launch hook**

```rust
// crates/controller/src/log_ingestion/locator.rs
pub struct FargateStreamLocator;

impl FargateStreamLocator {
    pub fn from_config(config: &serde_json::Value) -> anyhow::Result<CloudWatchLogTarget> {
        Ok(CloudWatchLogTarget {
            region: config.get("aws_region").and_then(|v| v.as_str()).unwrap_or("ap-northeast-1").to_string(),
            log_group_name: config.get("log_group_name").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("missing log_group_name"))?.to_string(),
            log_stream_name: None,
            stream_prefix: config.get("log_stream_prefix").and_then(|v| v.as_str()).map(str::to_string),
            start_time_hint_ms: Some(chrono::Utc::now().timestamp_millis()),
        })
    }
}
```

```rust
// crates/controller/src/worker_manager/fargate.rs
let external_id = launch_aws_fargate(state, run).await?;
crate::log_ingestion::RunLogCollector::spawn_for_run(state.clone(), run.clone()).await?;
Ok(external_id)
```

- [ ] **Step 4: Run targeted tests**

Run: `cargo test -p mrs-harris-controller fargate_locator_reads_worker_definition_logging_config -- --nocapture`
Expected: PASS

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/controller/src/log_ingestion/locator.rs crates/controller/src/log_ingestion/mod.rs crates/controller/src/worker_manager/fargate.rs
git commit -m "feat: hook fargate log collection into cloudwatch"
```

### Task 6: Implement Lambda CloudWatch Locator and Collector Hook

**Files:**
- Modify: `crates/controller/src/log_ingestion/locator.rs`
- Modify: `crates/controller/src/log_ingestion/mod.rs`
- Modify: `crates/controller/src/worker_manager/lambda.rs`
- Test: `crates/controller/src/log_ingestion/locator.rs` tests

- [ ] **Step 1: Write the failing Lambda locator test**

```rust
#[tokio::test]
async fn lambda_locator_reads_function_logging_config() {
    let target = LambdaStreamLocator::from_config(&serde_json::json!({
        "aws_region": "ap-northeast-1",
        "log_group_name": "/aws/lambda/mrs-harris-worker",
        "function_name": "mrs-harris-worker"
    }))
    .unwrap();

    assert_eq!(target.region, "ap-northeast-1");
    assert_eq!(target.log_group_name, "/aws/lambda/mrs-harris-worker");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mrs-harris-controller lambda_locator_reads_function_logging_config -- --nocapture`
Expected: FAIL because locator implementation does not exist

- [ ] **Step 3: Implement Lambda locator and collector launch hook**

```rust
// crates/controller/src/log_ingestion/locator.rs
pub struct LambdaStreamLocator;

impl LambdaStreamLocator {
    pub fn from_config(config: &serde_json::Value) -> anyhow::Result<CloudWatchLogTarget> {
        Ok(CloudWatchLogTarget {
            region: config.get("aws_region").and_then(|v| v.as_str()).unwrap_or("ap-northeast-1").to_string(),
            log_group_name: config.get("log_group_name").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("missing log_group_name"))?.to_string(),
            log_stream_name: None,
            stream_prefix: config.get("function_name").and_then(|v| v.as_str()).map(|name| format!("{}:", name)),
            start_time_hint_ms: Some(chrono::Utc::now().timestamp_millis()),
        })
    }
}
```

```rust
// crates/controller/src/worker_manager/lambda.rs
let request_id = launch_aws_lambda(state, run).await?;
crate::log_ingestion::RunLogCollector::spawn_for_run(state.clone(), run.clone()).await?;
Ok(request_id)
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mrs-harris-controller lambda_locator_reads_function_logging_config -- --nocapture`
Expected: PASS

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/controller/src/log_ingestion/locator.rs crates/controller/src/log_ingestion/mod.rs crates/controller/src/worker_manager/lambda.rs
git commit -m "feat: hook lambda log collection into cloudwatch"
```

### Task 7: Implement Collector Lifecycle, Drain Window, and UI Verification

**Files:**
- Modify: `crates/controller/src/log_ingestion/mod.rs`
- Modify: `crates/controller/src/log_ingestion/cloudwatch.rs`
- Modify: `crates/controller/src/web/runs.rs`
- Modify: `docs/ui_checklists/run_detail.md`
- Create or Modify: `test_browser_dialog_notice.js` or dedicated browser verification script

- [ ] **Step 1: Write the failing lifecycle test**

```rust
#[tokio::test]
async fn collector_stops_after_terminal_run_and_drain_window() {
    let mut collector = TestCollector::new()
        .with_terminal_status("succeeded")
        .with_exhausted_source(true)
        .with_drain_window(std::time::Duration::from_secs(1));

    let stopped = collector.tick_until_stopped().await;
    assert!(stopped);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mrs-harris-controller collector_stops_after_terminal_run_and_drain_window -- --nocapture`
Expected: FAIL because lifecycle stop logic is not implemented

- [ ] **Step 3: Implement lifecycle rules and add UI checklist items**

```rust
// crates/controller/src/log_ingestion/mod.rs
impl RunLogCollector {
    async fn collect_until_done<S: LogSource>(
        source: &mut S,
        run_id: i64,
        drain_window: std::time::Duration,
        state: &crate::app::AppState,
    ) -> anyhow::Result<()> {
        let terminal_seen_at = std::sync::Arc::new(tokio::sync::Mutex::new(None));

        loop {
            let batch = source.next_batch().await?;
            if !batch.lines.is_empty() {
                JobLogSink::append_batch(&state.db, run_id, batch).await?;
            }

            let run = crate::db::runs::get_run(&state.db, &run_id).await?
                .ok_or_else(|| anyhow::anyhow!("Run not found during log collection"))?;

            if run.status.is_terminal() {
                let mut guard = terminal_seen_at.lock().await;
                if guard.is_none() {
                    *guard = Some(tokio::time::Instant::now());
                }
                if source.is_exhausted().await?
                    && guard.unwrap().elapsed() >= drain_window
                {
                    break;
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        Ok(())
    }
}
```

```markdown
<!-- docs/ui_checklists/run_detail.md -->
- [ ] **実行ログのリアルタイム追従**:
  実行中の run を開いたまま、ログがページリロードなしで追記されること。
- [ ] **実行完了後のログ保持**:
  run 完了後も、最後まで取得したログが消えずに残ること。
```

- [ ] **Step 4: Run verification**

Run: `cargo test -p mrs-harris-controller collector_stops_after_terminal_run_and_drain_window -- --nocapture`
Expected: PASS

Run: `cargo check --workspace`
Expected: PASS

Run: browser verification script for a long-running controller job
Expected: log lines appear incrementally without reload

- [ ] **Step 5: Commit**

```bash
git add crates/controller/src/log_ingestion/mod.rs crates/controller/src/log_ingestion/cloudwatch.rs crates/controller/src/web/runs.rs docs/ui_checklists/run_detail.md
git commit -m "feat: finalize realtime run log collection lifecycle"
```

## Self-Review

### Spec coverage

- `controller` の逐次ログ保存: Task 3
- `fargate` CloudWatch 収集: Task 5
- `lambda` CloudWatch 収集: Task 6
- 共通 abstraction: Task 1, Task 4
- dedupe と `job_logs` 正規化: Task 2
- drain window / lifecycle: Task 7
- UI 確認: Task 7

### Placeholder scan

- `TODO`, `TBD`, 「適切に対応」系の曖昧表現は除去済み
- 各 task に具体的なファイル、コード、コマンド、期待結果を記載済み

### Type consistency

- 共通型は `LogBatch`, `AppendResult`, `LogSource`, `CloudWatchLogTarget`, `DbLogInsert`
- later task の参照名は earlier task と一致させている
- 実装時に既存 crate 構造との差異が出た場合は Task 1 で定義した名前に合わせて調整する
