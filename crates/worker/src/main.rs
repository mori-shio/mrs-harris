use clap::Parser;
use tracing_subscriber::EnvFilter;


/// Mrs. Harris Worker — ジョブ実行プロセス
#[derive(Parser)]
#[command(name = "mrs-harris-worker", version, about = "Mrs. Harris Job Worker")]
struct Cli {
    /// 実行するタスクの ID
    #[arg(long)]
    task_id: i64,

    /// Controller のコールバック URL
    #[arg(long)]
    callback_url: String,

    /// Controller API キー（オプション）
    #[arg(long)]
    api_key: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let cli = Cli::parse();

    mrs_harris_worker::run_worker(cli.task_id, cli.callback_url, cli.api_key).await?;

    Ok(())
}
