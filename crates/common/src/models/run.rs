use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::job::WorkerType;

/// ジョブ実行のステータス
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Scheduled,
    Queued,
    Running,
    Succeeded,
    Failed,
    Retrying,
    Cancelled,
    DeadLetter,
}

impl RunStatus {
    /// 終端状態かどうか
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::DeadLetter
        )
    }
}

/// トリガー種別
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    Scheduled,
    Manual,
    Dependency,
}

/// ジョブ実行履歴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    pub id: Uuid,
    pub job_id: Uuid,
    pub status: RunStatus,
    pub worker_type: WorkerType,
    pub worker_id: Option<String>,
    pub trigger_type: TriggerType,
    pub attempt: u32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub version: u32,
    pub worker_definition_id: Option<Uuid>,
    pub config_version: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 新規実行作成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRun {
    pub job_id: Uuid,
    pub worker_type: WorkerType,
    pub trigger_type: TriggerType,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub worker_definition_id: Option<Uuid>,
}

/// ログストリーム種別
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

/// 実行ログ行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub id: Option<i64>,
    pub run_id: Uuid,
    pub task_name: Option<String>,
    pub stream: LogStream,
    pub line: String,
    pub logged_at: DateTime<Utc>,
}

/// Worker からのコールバックペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCallback {
    pub task_id: Uuid,
    pub status: RunStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub logs: Vec<LogLine>,
    pub duration_ms: Option<i64>,
}
