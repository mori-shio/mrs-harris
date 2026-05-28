use thiserror::Error;

/// Mrs. Harris 共通エラー型
#[derive(Error, Debug)]
pub enum AppError {
    #[error("データベースエラー: {0}")]
    Database(#[from] sqlx::Error),

    #[error("設定エラー: {0}")]
    Config(String),

    #[error("シリアライゼーションエラー: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("TOML パースエラー: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("ジョブが見つかりません: {0}")]
    JobNotFound(i64),

    #[error("実行が見つかりません: {0}")]
    RunNotFound(i64),

    #[error("不正な状態遷移: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("DAG検証エラー: {0}")]
    DagValidation(String),

    #[error("認証エラー: {0}")]
    Authentication(String),

    #[error("認可エラー: {0}")]
    Authorization(String),

    #[error("ワーカー起動エラー: {0}")]
    WorkerLaunch(String),

    #[error("通知送信エラー: {0}")]
    Notification(String),

    #[error("タイムアウト: {0}")]
    Timeout(String),

    #[error("内部エラー: {0}")]
    Internal(String),

    #[error("IOエラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTPエラー: {0}")]
    Http(#[from] reqwest::Error),
}

/// axum レスポンスに変換するための型
pub type AppResult<T> = Result<T, AppError>;
