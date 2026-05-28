use serde::{Deserialize, Serialize};

use super::job::{RetryPolicy, WorkerType};

/// DAGタスク定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagTask {
    pub id: i64,
    pub dag_id: i64,
    pub task_name: String,
    pub payload: serde_json::Value,
    pub worker_type: WorkerType,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout_sec: Option<u32>,
}

/// DAGエッジ定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagEdge {
    pub id: i64,
    pub dag_id: i64,
    pub from_task: String,
    pub to_task: String,
}

/// DAGタスク実行履歴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub id: i64,
    pub run_id: i64,
    pub task_name: String,
    pub status: super::run::RunStatus,
    pub worker_id: Option<String>,
    pub attempt: u32,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: Option<i64>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// TOML ジョブ定義内のDAGタスク（インポート用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagTaskDefinition {
    pub name: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub worker_type: WorkerType,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout_sec: Option<u32>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}
