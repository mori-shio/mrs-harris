use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 通知チャネル種別
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Slack,
    Email,
}

/// Slack通知設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub webhook_url: String,
    pub channel: Option<String>,
    pub username: Option<String>,
    pub ssm_parameter_path: Option<String>,
    pub ssm_region: Option<String>,
}

/// メール通知設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub to: Vec<String>,
    pub cc: Option<Vec<String>>,
}

/// 通知チャネル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: i64,
    pub name: String,
    pub channel_type: ChannelType,
    pub config: serde_json::Value,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// ジョブ通知設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobNotification {
    pub job_id: i64,
    pub channel_id: i64,
    pub on_events: Vec<String>,
}

/// 新規通知チャネル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNotificationChannel {
    pub name: String,
    pub channel_type: ChannelType,
    pub config: serde_json::Value,
}
