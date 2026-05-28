use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// ユーザー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// ユーザーロール
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
}

/// ログインリクエスト
#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// JWT クレーム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// ユーザーID
    pub sub: i64,
    pub username: String,
    pub role: UserRole,
    /// 有効期限タイムスタンプ
    pub exp: usize,
    /// 発行時刻
    pub iat: usize,
}
