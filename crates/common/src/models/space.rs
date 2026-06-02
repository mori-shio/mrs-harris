use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// スペース定義（DB行に対応）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 新規スペース作成リクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSpace {
    pub name: String,
    pub description: Option<String>,
    pub priority: i32,
}
