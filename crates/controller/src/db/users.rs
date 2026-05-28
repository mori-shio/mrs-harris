use sqlx::MySqlPool;
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};

use chrono::{DateTime, Utc};

/// Admin ユーザーを作成
pub async fn create_admin_user(
    pool: &MySqlPool,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("パスワードハッシュエラー: {}", e))?
        .to_string();

    
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"INSERT INTO users (username, password_hash, role, created_at, updated_at)
           VALUES (?, ?, 'admin', ?, ?)
           ON DUPLICATE KEY UPDATE password_hash = VALUES(password_hash), updated_at = VALUES(updated_at)"#
    )
    
    .bind(username)
    .bind(&password_hash)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// ユーザーを取得
pub async fn get_user_by_username(
    pool: &MySqlPool,
    username: &str,
) -> anyhow::Result<Option<mrs_harris_common::models::user::User>> {
    use sqlx::Row;
    use std::str::FromStr;

    let row = sqlx::query("SELECT id, username, password_hash, role, created_at, updated_at FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;

    if let Some(r) = row {
        let id: i64 = r.try_get("id")?;
        let username: String = r.try_get("username")?;
        let password_hash: String = r.try_get("password_hash")?;
        let role_str: String = r.try_get("role")?;
        let role = mrs_harris_common::models::user::UserRole::from_str(&role_str)
            .map_err(|e| anyhow::anyhow!("Invalid UserRole: {}", e))?;
        
        let created_at: DateTime<Utc> = r.try_get("created_at")?;
        let updated_at: DateTime<Utc> = r.try_get("updated_at")?;

        Ok(Some(mrs_harris_common::models::user::User {
            id,
            username,
            password_hash,
            role,
            created_at,
            updated_at,
        }))
    } else {
        Ok(None)
    }
}

/// テーブルが空の場合、初期管理者ユーザー（admin/admin）を作成する
pub async fn seed_default_admin_if_needed(pool: &MySqlPool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;

    if count == 0 {
        tracing::info!("ユーザーテーブルが空です。初期管理者ユーザー 'admin' を作成します...");
        create_admin_user(pool, "admin", "admin").await?;
        tracing::info!("初期管理者ユーザー 'admin' （パスワード: 'admin'）を作成しました。");
    }

    Ok(())
}

