use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod api;
mod app;
mod db;
mod log_ingestion;
mod notification;
mod scheduler;
mod web;
mod worker_manager;

/// Mrs. Harris — ジョブスケジューラ
#[derive(Parser)]
#[command(name = "mrs-harris", version, about = "Mrs. Harris Job Scheduler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Controller モードで起動
    Controller {
        /// 設定ファイルのパス
        #[arg(short, long, default_value = "config/controller.toml")]
        config: PathBuf,
    },
    /// DB マイグレーションを実行
    Migrate {
        /// 設定ファイルのパス
        #[arg(short, long, default_value = "config/controller.toml")]
        config: PathBuf,
    },
    /// 初期 Admin ユーザーを作成
    InitAdmin {
        /// 設定ファイルのパス
        #[arg(short, long, default_value = "config/controller.toml")]
        config: PathBuf,
        /// Admin ユーザー名
        #[arg(long, default_value = "admin")]
        username: String,
        /// Admin パスワード
        #[arg(long)]
        password: String,
    },
    /// Worker モードで起動
    Worker {
        /// 実行するタスクの ID
        #[arg(long)]
        task_id: i64,

        /// Controller のコールバック URL
        #[arg(long)]
        callback_url: String,

        /// Controller API キー（オプション）
        #[arg(long)]
        api_key: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,mrs_harris=debug")),
        )
        .with_target(true)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Controller { config } => {
            tracing::info!("Mrs. Harris Controller を起動します...");
            let config = mrs_harris_common::config::ControllerConfig::from_file(&config)
                .map_err(|e| anyhow::anyhow!("設定ファイルの読み込みに失敗: {}", e))?;
            app::run_controller(config).await?;
        }
        Commands::Migrate { config } => {
            tracing::info!("データベースマイグレーションを実行します...");
            let config = mrs_harris_common::config::ControllerConfig::from_file(&config)
                .map_err(|e| anyhow::anyhow!("設定ファイルの読み込みに失敗: {}", e))?;
            db::run_migrations(&config.database).await?;
            tracing::info!("マイグレーション完了");
        }
        Commands::InitAdmin {
            config,
            username,
            password,
        } => {
            tracing::info!("Admin ユーザーを作成します...");
            let config = mrs_harris_common::config::ControllerConfig::from_file(&config)
                .map_err(|e| anyhow::anyhow!("設定ファイルの読み込みに失敗: {}", e))?;
            let pool = db::create_pool(&config.database).await?;
            db::users::create_admin_user(&pool, &username, &password).await?;
            tracing::info!("Admin ユーザー '{}' を作成しました", username);
        }
        Commands::Worker {
            task_id,
            callback_url,
            api_key,
        } => {
            mrs_harris_worker::run_worker(task_id, callback_url, api_key).await?;
        }
    }

    Ok(())
}
