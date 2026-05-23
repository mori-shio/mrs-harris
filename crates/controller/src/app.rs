use axum::Router;
use mrs_harris_common::config::ControllerConfig;
use sqlx::MySqlPool;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// アプリケーション共有状態
#[derive(Clone)]
pub struct AppState {
    pub db: MySqlPool,
    pub config: Arc<ControllerConfig>,
}

/// Controller を起動
pub async fn run_controller(config: ControllerConfig) -> anyhow::Result<()> {
    // DB接続プール作成
    let pool = crate::db::create_pool(&config.database).await?;

    // マイグレーション実行
    crate::db::run_migrations(&config.database).await?;

    // 初期管理者ユーザーの自動作成 (テーブルが空の場合)
    crate::db::users::seed_default_admin_if_needed(&pool).await?;

    let state = AppState {
        db: pool.clone(),
        config: Arc::new(config.clone()),
    };

    // スケジューラをバックグラウンドで起動
    let scheduler_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::scheduler::run_scheduler(scheduler_state).await {
            tracing::error!("スケジューラエラー: {}", e);
        }
    });

    // Axum ルーター構築
    let app = build_router(state.clone());

    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("Mrs. Harris Controller listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        // API routes
        .nest("/api", crate::api::router())
        // Web dashboard routes
        .merge(crate::web::router())
        // Static files
        .nest_service(
            "/static",
            tower_http::services::ServeDir::new("static"),
        )
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
