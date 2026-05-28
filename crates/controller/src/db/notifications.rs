use mrs_harris_common::models::notification::{NotificationChannel, ChannelType};
use sqlx::{MySqlPool, Row};

use std::str::FromStr;

/// 特定のジョブとイベントに対応する、アクティブな通知チャネルの一覧を取得する
pub async fn get_notifications_for_job(
    pool: &MySqlPool,
    job_id: &i64,
    event: &str,
) -> anyhow::Result<Vec<NotificationChannel>> {
    let rows = sqlx::query(
        r#"SELECT c.id, c.name, c.channel_type, c.config, c.is_active, c.created_at, jn.on_events
           FROM notification_channels c
           JOIN job_notifications jn ON c.id = jn.channel_id
           WHERE jn.job_id = ? AND c.is_active = 1"#
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    let mut channels = Vec::new();
    for row in rows {
        let on_events_val: serde_json::Value = row.try_get("on_events")?;
        let on_events: Vec<String> = serde_json::from_value(on_events_val)?;
        
        if on_events.iter().any(|e| e == event) {
            let id: i64 = row.try_get("id")?;
            let name: String = row.try_get("name")?;
            let ct_str: String = row.try_get("channel_type")?;
            let channel_type = ChannelType::from_str(&ct_str)
                .map_err(|e| anyhow::anyhow!("Invalid channel type: {}", e))?;
            let config: serde_json::Value = row.try_get("config")?;
            let is_active_val: i8 = row.try_get("is_active")?;
            let is_active = is_active_val != 0;
            let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at")?;

            channels.push(NotificationChannel {
                id,
                name,
                channel_type,
                config,
                is_active,
                created_at,
            });
        }
    }
    Ok(channels)
}

/// 全ての通知チャネルを取得
pub async fn list_channels(pool: &MySqlPool) -> anyhow::Result<Vec<NotificationChannel>> {
    let rows = sqlx::query("SELECT * FROM notification_channels ORDER BY name ASC")
        .fetch_all(pool)
        .await?;

    let mut channels = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let ct_str: String = row.try_get("channel_type")?;
        let channel_type = ChannelType::from_str(&ct_str)
            .map_err(|e| anyhow::anyhow!("Invalid channel type: {}", e))?;
        let config: serde_json::Value = row.try_get("config")?;
        let is_active_val: i8 = row.try_get("is_active")?;
        let is_active = is_active_val != 0;
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at")?;

        channels.push(NotificationChannel {
            id,
            name,
            channel_type,
            config,
            is_active,
            created_at,
        });
    }
    Ok(channels)
}
