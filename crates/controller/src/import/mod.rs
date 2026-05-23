use std::str::FromStr;
use uuid::Uuid;
use sqlx::{MySqlPool, Row};
use mrs_harris_common::models::job::{JobType, WorkerType, RetryPolicy, BackoffStrategy};

#[derive(Debug, serde::Deserialize)]
struct TomlJobFile {
    job: TomlJob,
}

#[derive(Debug, serde::Deserialize)]
struct TomlJob {
    name: String,
    description: Option<String>,
    job_type: String,
    schedule: Option<String>,
    #[serde(default)]
    worker_type: Option<String>,
    payload: Option<serde_json::Value>,
    #[serde(default)]
    tasks: Vec<TomlDagTask>,
    retry: Option<TomlRetryPolicy>,
    timeout: Option<TomlTimeout>,
    notifications: Option<TomlNotifications>,
}

#[derive(Debug, serde::Deserialize)]
struct TomlDagTask {
    name: String,
    #[serde(default)]
    worker_type: Option<String>,
    payload: serde_json::Value,
    #[serde(default)]
    depends_on: Vec<String>,
    retry: Option<TomlRetryPolicy>,
    timeout: Option<TomlTimeout>,
}

#[derive(Debug, serde::Deserialize)]
struct TomlRetryPolicy {
    max_retries: u32,
    backoff: String,
    base_delay_sec: u64,
}

#[derive(Debug, serde::Deserialize)]
struct TomlTimeout {
    seconds: u32,
}

#[derive(Debug, serde::Deserialize)]
struct TomlNotifications {
    channels: Vec<String>,
    on_events: Vec<String>,
}

/// TOML ファイルからジョブのインポートを実行する
pub async fn import_job_from_toml(
    pool: &MySqlPool,
    toml_str: &str,
) -> anyhow::Result<Uuid> {
    // 1. TOML のパース
    let parsed: TomlJobFile = toml::from_str(toml_str)
        .map_err(|e| anyhow::anyhow!("TOML parse error: {}", e))?;
    let toml_job = parsed.job;

    // 2. 基本フィールドのマッピング
    let job_type = match toml_job.job_type.as_str() {
        "cron" => JobType::Cron,
        "dag" => JobType::Dag,
        "one_shot" => JobType::OneShot,
        other => return Err(anyhow::anyhow!("Unknown job_type: {}", other)),
    };

    let worker_type = match toml_job.worker_type.as_deref().unwrap_or("fargate") {
        "fargate" => WorkerType::Fargate,
        "lambda" => WorkerType::Lambda,
        other => return Err(anyhow::anyhow!("Unknown worker_type: {}", other)),
    };

    let retry_policy = if let Some(ref r) = toml_job.retry {
        let backoff = BackoffStrategy::from_str(&r.backoff)
            .map_err(|e| anyhow::anyhow!("Invalid backoff strategy: {}", e))?;
        RetryPolicy {
            max_retries: r.max_retries,
            backoff,
            base_delay_sec: r.base_delay_sec,
        }
    } else {
        RetryPolicy::default()
    };

    let timeout_sec = toml_job.timeout.as_ref().map(|t| t.seconds).unwrap_or(3600);
    
    // ペイロード（DAGの場合は空の JSON）
    let parent_payload = if job_type == JobType::Dag {
        serde_json::json!({})
    } else {
        toml_job.payload.clone().unwrap_or_else(|| serde_json::json!({}))
    };

    // 3. トランザクションの開始
    let mut tx = pool.begin().await?;

    // 同名ジョブの既存チェック＆カスケード削除
    let existing_row = sqlx::query("SELECT id FROM jobs WHERE name = ?")
        .bind(&toml_job.name)
        .fetch_optional(&mut *tx)
        .await?;

    if let Some(row) = existing_row {
        let id_str: String = row.try_get("id")?;
        tracing::info!("Existing job '{}' found (ID: {}). Overwriting...", toml_job.name, id_str);
        sqlx::query("DELETE FROM jobs WHERE id = ?")
            .bind(id_str)
            .execute(&mut *tx)
            .await?;
    }

    // ジョブの新規作成
    let job_id = Uuid::new_v4();
    let retry_policy_json = serde_json::to_value(&retry_policy)?;
    let tags_json = serde_json::to_value(&Vec::<String>::new())?;

    sqlx::query(
        r#"INSERT INTO jobs (id, name, description, job_type, payload, schedule_expr, worker_type, retry_policy, timeout_sec, is_active, tags)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)"#
    )
    .bind(job_id.to_string())
    .bind(&toml_job.name)
    .bind(&toml_job.description)
    .bind(job_type.to_string())
    .bind(&parent_payload)
    .bind(&toml_job.schedule)
    .bind(worker_type.to_string())
    .bind(retry_policy_json)
    .bind(timeout_sec)
    .bind(tags_json)
    .execute(&mut *tx)
    .await?;

    // 4. DAGタスクとエッジの登録
    if job_type == JobType::Dag {
        if toml_job.tasks.is_empty() {
            return Err(anyhow::anyhow!("DAG job must contain at least one task"));
        }

        for task in &toml_job.tasks {
            let task_id = Uuid::new_v4();
            let t_worker_type = match task.worker_type.as_deref().unwrap_or("fargate") {
                "fargate" => WorkerType::Fargate,
                "lambda" => WorkerType::Lambda,
                other => return Err(anyhow::anyhow!("Unknown task worker_type: {}", other)),
            };

            let t_retry_policy = if let Some(ref r) = task.retry {
                let backoff = BackoffStrategy::from_str(&r.backoff)
                    .map_err(|e| anyhow::anyhow!("Invalid task backoff strategy: {}", e))?;
                Some(RetryPolicy {
                    max_retries: r.max_retries,
                    backoff,
                    base_delay_sec: r.base_delay_sec,
                })
            } else {
                None
            };
            let t_retry_json = t_retry_policy.as_ref().map(|rp| serde_json::to_value(rp).unwrap());

            let t_timeout_sec = task.timeout.as_ref().map(|t| t.seconds);

            sqlx::query(
                r#"INSERT INTO dag_tasks (id, dag_id, task_name, payload, worker_type, retry_policy, timeout_sec)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#
            )
            .bind(task_id.to_string())
            .bind(job_id.to_string())
            .bind(&task.name)
            .bind(&task.payload)
            .bind(t_worker_type.to_string())
            .bind(t_retry_json)
            .bind(t_timeout_sec)
            .execute(&mut *tx)
            .await?;

            // 依存関係（エッジ）の登録
            for dep in &task.depends_on {
                let edge_id = Uuid::new_v4();
                sqlx::query(
                    r#"INSERT INTO dag_edges (id, dag_id, from_task, to_task)
                       VALUES (?, ?, ?, ?)"#
                )
                .bind(edge_id.to_string())
                .bind(job_id.to_string())
                .bind(dep)
                .bind(&task.name)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    // 5. 通知紐付け設定
    if let Some(ref notif) = toml_job.notifications {
        let events_json = serde_json::to_value(&notif.on_events)?;

        for channel_name in &notif.channels {
            // チャネルがすでにDBに存在するかチェック
            let channel_row = sqlx::query("SELECT id FROM notification_channels WHERE name = ?")
                .bind(channel_name)
                .fetch_optional(&mut *tx)
                .await?;

            let channel_id = match channel_row {
                Some(row) => {
                    let id_str: String = row.try_get("id")?;
                    Uuid::parse_str(&id_str)?
                }
                None => {
                    // 存在しない場合は、プレースホルダーチャネルを自動作成して警告する
                    let c_id = Uuid::new_v4();
                    let is_slack = channel_name.to_lowercase().contains("slack");
                    let channel_type_str = if is_slack { "slack" } else { "email" };
                    let config_json = if is_slack {
                        serde_json::json!({
                            "webhook_url": "",
                            "channel": null,
                            "username": "Mrs. Harris"
                        })
                    } else {
                        serde_json::json!({
                            "to": [],
                            "cc": null
                        })
                    };

                    tracing::warn!(
                        "Notification channel '{}' not found in database. Creating placeholder channel. Please configure it in the dashboard.",
                        channel_name
                    );

                    sqlx::query(
                        r#"INSERT INTO notification_channels (id, name, channel_type, config, is_active)
                           VALUES (?, ?, ?, ?, 1)"#
                    )
                    .bind(c_id.to_string())
                    .bind(channel_name)
                    .bind(channel_type_str)
                    .bind(config_json)
                    .execute(&mut *tx)
                    .await?;

                    c_id
                }
            };

            // ジョブ通知の紐付けを挿入
            sqlx::query(
                r#"INSERT INTO job_notifications (job_id, channel_id, on_events)
                   VALUES (?, ?, ?)"#
            )
            .bind(job_id.to_string())
            .bind(channel_id.to_string())
            .bind(&events_json)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    tracing::info!("Successfully imported job '{}' (ID: {})", toml_job.name, job_id);

    Ok(job_id)
}
