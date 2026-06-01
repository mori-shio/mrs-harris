use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::models::run::LogArchiveStore;

/// Controller 設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub scheduler: SchedulerConfig,
    pub fargate: FargateConfig,
    pub lambda: LambdaConfig,
    #[serde(default)]
    pub log_archive: LogArchiveConfig,
    #[serde(default)]
    pub notification: NotificationConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

/// サーバー設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Controller 自身の外部URL（Worker からのコールバック先）
    pub external_url: String,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}

/// データベース接続設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 {
    20
}

/// スケジューラ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// スケジューラのポーリング間隔（秒）
    #[serde(default = "default_poll_interval")]
    pub poll_interval_sec: u64,
    /// リーパーのチェック間隔（秒）
    #[serde(default = "default_reaper_interval")]
    pub reaper_interval_sec: u64,
    /// ワーカータイムアウト（秒）
    #[serde(default = "default_worker_timeout")]
    pub worker_timeout_sec: u64,
}

fn default_poll_interval() -> u64 {
    5
}
fn default_reaper_interval() -> u64 {
    30
}
fn default_worker_timeout() -> u64 {
    3600
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            poll_interval_sec: default_poll_interval(),
            reaper_interval_sec: default_reaper_interval(),
            worker_timeout_sec: default_worker_timeout(),
        }
    }
}

/// 実行ログアーカイブ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogArchiveConfig {
    #[serde(default = "default_log_archive_store")]
    pub store: LogArchiveStore,
    #[serde(default = "default_local_file_base_dir")]
    pub local_file_base_dir: String,
    #[serde(default)]
    pub s3_bucket: Option<String>,
    #[serde(default)]
    pub s3_prefix: Option<String>,
    #[serde(default)]
    pub s3_region: Option<String>,
    #[serde(default)]
    pub s3_endpoint_url: Option<String>,
    #[serde(default)]
    pub s3_force_path_style: Option<bool>,
}

fn default_log_archive_store() -> LogArchiveStore {
    LogArchiveStore::LocalFile
}

fn default_local_file_base_dir() -> String {
    "data/log-archives".to_string()
}

impl Default for LogArchiveConfig {
    fn default() -> Self {
        Self {
            store: default_log_archive_store(),
            local_file_base_dir: default_local_file_base_dir(),
            s3_bucket: None,
            s3_prefix: None,
            s3_region: None,
            s3_endpoint_url: None,
            s3_force_path_style: None,
        }
    }
}

/// Fargate 実行設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FargateConfig {
    pub cluster_arn: String,
    pub task_definition: String,
    pub subnets: Vec<String>,
    #[serde(default)]
    pub security_groups: Vec<String>,
    #[serde(default = "default_container_name")]
    pub container_name: String,
    pub assign_public_ip: Option<bool>,
}

fn default_container_name() -> String {
    "mrs-harris-worker".to_string()
}

/// Lambda 実行設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaConfig {
    pub function_name: String,
    #[serde(default)]
    pub qualifier: Option<String>,
}

/// 通知グローバル設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    pub slack: Option<SlackGlobalConfig>,
    pub email: Option<EmailGlobalConfig>,
}

/// Slack グローバル設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackGlobalConfig {
    pub default_webhook_url: Option<String>,
}

/// メール送信グローバル設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailGlobalConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub from_address: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// 認証設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT シークレットキー
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    /// JWT 有効期限（時間）
    #[serde(default = "default_jwt_expiry_hours")]
    pub jwt_expiry_hours: u64,
}

fn default_jwt_secret() -> String {
    "change-me-in-production".to_string()
}
fn default_jwt_expiry_hours() -> u64 {
    24
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: default_jwt_secret(),
            jwt_expiry_hours: default_jwt_expiry_hours(),
        }
    }
}

/// Worker 設定（環境変数またはコマンドライン引数から取得）
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub task_id: i64,
    pub callback_url: String,
    pub controller_api_key: Option<String>,
}

impl ControllerConfig {
    /// TOML ファイルから設定を読み込む
    pub fn from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        config.warn_insecure_defaults();
        Ok(config)
    }

    fn warn_insecure_defaults(&self) {
        if self.auth.jwt_secret == "change-me-in-production"
            || self.auth.jwt_secret == "change-me-in-production-use-a-long-random-string"
            || self.auth.jwt_secret == "REPLACE_WITH_A_LONG_RANDOM_STRING"
        {
            eprintln!(
                "WARNING: JWT secret がデフォルト値のままです。本番環境では必ず安全なランダム文字列に変更してください。"
            );
        }
    }
}
