use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::job::WorkerType;

/// ジョブ実行のステータス
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
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

    pub fn label_ja(&self) -> &'static str {
        match self {
            Self::Pending => "保留中",
            Self::Scheduled => "予約済み",
            Self::Queued => "キュー待ち",
            Self::Running => "実行中",
            Self::Succeeded => "成功",
            Self::Failed => "失敗",
            Self::Retrying => "再試行中",
            Self::Cancelled => "キャンセル済み",
            Self::DeadLetter => "失敗 (要確認)",
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Pending | Self::Scheduled | Self::Queued => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed | Self::DeadLetter => "failed",
            Self::Retrying => "retrying",
            Self::Cancelled => "cancelled",
        }
    }
}

impl TriggerType {
    pub fn label_ja(&self) -> &'static str {
        match self {
            Self::Scheduled => "自動スケジュール",
            Self::Manual => "手動実行",
            Self::Dependency => "DAG依存",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RunStatus, TriggerType};

    #[test]
    fn run_status_label_and_badge_are_stable() {
        assert_eq!(RunStatus::Pending.label_ja(), "保留中");
        assert_eq!(RunStatus::Scheduled.label_ja(), "予約済み");
        assert_eq!(RunStatus::Queued.label_ja(), "キュー待ち");
        assert_eq!(RunStatus::Retrying.label_ja(), "再試行中");
        assert_eq!(RunStatus::Cancelled.label_ja(), "キャンセル済み");
        assert_eq!(RunStatus::DeadLetter.label_ja(), "失敗 (要確認)");

        assert_eq!(RunStatus::Pending.badge_class(), "pending");
        assert_eq!(RunStatus::Scheduled.badge_class(), "pending");
        assert_eq!(RunStatus::Queued.badge_class(), "pending");
        assert_eq!(RunStatus::Running.badge_class(), "running");
        assert_eq!(RunStatus::Succeeded.badge_class(), "succeeded");
        assert_eq!(RunStatus::Failed.badge_class(), "failed");
        assert_eq!(RunStatus::DeadLetter.badge_class(), "failed");
        assert_eq!(RunStatus::Retrying.badge_class(), "retrying");
        assert_eq!(RunStatus::Cancelled.badge_class(), "cancelled");
    }

    #[test]
    fn trigger_type_label_is_stable() {
        assert_eq!(TriggerType::Scheduled.label_ja(), "自動スケジュール");
        assert_eq!(TriggerType::Manual.label_ja(), "手動実行");
        assert_eq!(TriggerType::Dependency.label_ja(), "DAG依存");
    }
}

/// トリガー種別
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
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
    pub id: i64,
    pub job_id: i64,
    pub run_number: i64,
    pub status: RunStatus,
    pub worker_type: WorkerType,
    pub worker_id: Option<i64>,
    pub trigger_type: TriggerType,
    pub attempt: u32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub job_history_id: Option<i64>,
    pub worker_definition_id: Option<i64>,
    pub config_version: Option<u32>, // Populated via JOIN with job_history
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 新規実行作成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRun {
    pub job_id: i64,
    pub worker_type: WorkerType,
    pub trigger_type: TriggerType,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub worker_definition_id: Option<i64>,
}

/// ログストリーム種別
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
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
    pub run_id: i64,
    pub task_name: Option<String>,
    pub stream: LogStream,
    pub line: String,
    pub logged_at: DateTime<Utc>,
}

/// Worker からのコールバックペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCallback {
    pub task_id: i64,
    pub status: RunStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub logs: Vec<LogLine>,
    pub duration_ms: Option<i64>,
}
