use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// ジョブの種類
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    Cron,
    Dag,
    OneShot,
}

/// ワーカーの種類
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    #[default]
    Fargate,
    Lambda,
}

#[cfg(test)]
mod tests {
    use super::WorkerType;
    use std::str::FromStr;

    #[test]
    fn worker_type_does_not_include_controller_variant() {
        assert_eq!(WorkerType::Fargate.to_string(), "fargate");
        assert_eq!(WorkerType::Lambda.to_string(), "lambda");
        assert!(WorkerType::from_str("controller").is_err());
    }
}

/// シェルコマンドのペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellPayload {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    // SSM Parameter Store Settings
    #[serde(default)]
    pub ssm_region: Option<String>,
    #[serde(default)]
    pub ssm_path: Option<String>,
    #[serde(default)]
    pub ssm_recursive: Option<bool>,
}

/// リトライポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_backoff")]
    pub backoff: BackoffStrategy,
    #[serde(default = "default_base_delay")]
    pub base_delay_sec: u64,
}

fn default_max_retries() -> u32 {
    3
}
fn default_backoff() -> BackoffStrategy {
    BackoffStrategy::Exponential
}
fn default_base_delay() -> u64 {
    10
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            backoff: default_backoff(),
            base_delay_sec: default_base_delay(),
        }
    }
}

/// バックオフ戦略
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    Fixed,
    Linear,
    Exponential,
}

/// ジョブ定義（DB行に対応）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub job_type: JobType,
    pub payload: serde_json::Value,
    pub schedule_expr: Option<String>,
    pub worker_type: WorkerType,
    pub retry_policy: RetryPolicy,
    pub timeout_sec: u32,
    pub is_active: bool,
    pub tags: Vec<String>,
    pub worker_definition_id: Option<i64>,
    pub space_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 新規ジョブ作成リクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub name: String,
    pub description: Option<String>,
    pub job_type: JobType,
    pub payload: serde_json::Value,
    pub schedule_expr: Option<String>,
    #[serde(default)]
    pub worker_type: WorkerType,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    #[serde(default = "default_timeout")]
    pub timeout_sec: u32,
    #[serde(default = "default_active")]
    pub is_active: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub worker_definition_id: Option<i64>,
    pub space_id: Option<i64>,
}

fn default_timeout() -> u32 {
    3600
}
fn default_active() -> bool {
    true
}

/// ジョブ更新リクエスト
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobUpdate {
    pub description: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub schedule_expr: Option<Option<String>>,
    pub worker_type: Option<WorkerType>,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout_sec: Option<u32>,
    pub is_active: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub worker_definition_id: Option<Option<i64>>,
    pub space_id: Option<Option<i64>>,
}

/// ジョブフィルタ
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobFilter {
    pub job_type: Option<JobType>,
    pub is_active: Option<bool>,
    pub tag: Option<String>,
    pub search: Option<String>,
    pub space_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
