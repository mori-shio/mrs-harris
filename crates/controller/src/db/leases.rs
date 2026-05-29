use chrono::{Duration, Utc};
use sqlx::{MySqlPool, Row};

pub async fn try_acquire_lease(
    pool: &MySqlPool,
    lease_name: &str,
    owner_id: &str,
    ttl_seconds: u64,
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let now = Utc::now();
    let expires_at = now + Duration::seconds(ttl_seconds as i64);

    let row = sqlx::query(
        "SELECT owner_id, expires_at FROM scheduler_leases WHERE lease_name = ? FOR UPDATE",
    )
    .bind(lease_name)
    .fetch_optional(&mut *tx)
    .await?;

    let acquired = match row {
        Some(row) => {
            let current_owner: String = row.try_get("owner_id")?;
            let current_expires_at: chrono::DateTime<chrono::Utc> = row.try_get("expires_at")?;
            if current_owner == owner_id || current_expires_at <= now {
                sqlx::query(
                    "UPDATE scheduler_leases SET owner_id = ?, expires_at = ? WHERE lease_name = ?",
                )
                .bind(owner_id)
                .bind(expires_at)
                .bind(lease_name)
                .execute(&mut *tx)
                .await?;
                true
            } else {
                false
            }
        }
        None => {
            sqlx::query(
                "INSERT INTO scheduler_leases (lease_name, owner_id, expires_at) VALUES (?, ?, ?)",
            )
            .bind(lease_name)
            .bind(owner_id)
            .bind(expires_at)
            .execute(&mut *tx)
            .await?;
            true
        }
    };

    tx.commit().await?;
    Ok(acquired)
}
