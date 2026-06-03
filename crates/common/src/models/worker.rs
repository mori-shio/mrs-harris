use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 起動されたワーカー実体情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub id: i64,
    pub worker_definition_history_id: i64,
    pub worker_type: super::job::WorkerType,
    /// Fargate Task ARN または Lambda Request ID
    pub external_id: Option<String>,
    pub status: WorkerStatus,
    pub job_run_id: i64,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// ワーカーステータス
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
}

/// ワーカー定義（ノード/スレーブ設定）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDefinition {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub worker_type: super::job::WorkerType,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDefinitionHistoryEntry {
    pub id: i64,
    pub worker_definition_id: i64,
    pub version: u32,
    pub payload: serde_json::Value,
    pub changed_by: String,
    pub changed_at: DateTime<Utc>,
}

/// 新規ワーカー定義作成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkerDefinition {
    pub name: String,
    pub description: Option<String>,
    pub worker_type: super::job::WorkerType,
    pub config: serde_json::Value,
}

/// ワーカー定義更新
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerDefinitionUpdate {
    pub description: Option<String>,
    pub worker_type: Option<super::job::WorkerType>,
    pub config: Option<serde_json::Value>,
}
