use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ワーカー実行のトラッキング情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub id: Uuid,
    pub worker_type: super::job::WorkerType,
    /// Fargate Task ARN または Lambda Request ID
    pub external_id: String,
    pub status: WorkerStatus,
    pub run_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// ワーカーステータス
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
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
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub worker_type: super::job::WorkerType,
    pub config: serde_json::Value,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 新規ワーカー定義作成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkerDefinition {
    pub name: String,
    pub description: Option<String>,
    pub worker_type: super::job::WorkerType,
    pub config: serde_json::Value,
    #[serde(default = "default_active")]
    pub is_active: bool,
}

fn default_active() -> bool {
    true
}

/// ワーカー定義更新
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerDefinitionUpdate {
    pub description: Option<String>,
    pub worker_type: Option<super::job::WorkerType>,
    pub config: Option<serde_json::Value>,
    pub is_active: Option<bool>,
}
