use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// スペース定義（DB行に対応）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 新規スペース作成リクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSpace {
    pub name: String,
    pub description: Option<String>,
}
