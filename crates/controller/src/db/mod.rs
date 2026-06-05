use mrs_harris_common::config::DatabaseConfig;
use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;

pub mod jobs;
pub mod leases;
pub mod logs;
pub mod notifications;
pub mod runs;
pub mod step_flows;
pub mod users;
pub mod workers;

/// MySQL 接続プールを作成
pub async fn create_pool(config: &DatabaseConfig) -> anyhow::Result<MySqlPool> {
    let pool = MySqlPoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&config.url)
        .await?;
    tracing::info!("MySQL 接続プール作成完了");
    Ok(pool)
}

/// マイグレーションを実行
pub async fn run_migrations(config: &DatabaseConfig) -> anyhow::Result<()> {
    let pool = create_pool(config).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("マイグレーション完了");
    Ok(())
}
