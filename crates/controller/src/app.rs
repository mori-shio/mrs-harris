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
    pub scheduler_instance_id: String,
}

struct InitOptions {
    run_migrations: bool,
    seed_default_admin: bool,
}

/// 既存互換の Controller モードで起動
pub async fn run_controller(config: ControllerConfig) -> anyhow::Result<()> {
    let state = initialize_state(
        config.clone(),
        InitOptions {
            run_migrations: true,
            seed_default_admin: true,
        },
    )
    .await?;

    let scheduler_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::scheduler::run_scheduler(scheduler_state).await {
            tracing::error!("スケジューラエラー: {}", e);
        }
    });

    serve_web(state).await
}

/// Web/UI と API のみを起動
pub async fn run_web(config: ControllerConfig) -> anyhow::Result<()> {
    let state = initialize_state(
        config,
        InitOptions {
            run_migrations: true,
            seed_default_admin: true,
        },
    )
    .await?;

    serve_web(state).await
}

/// Scheduler のみを起動
pub async fn run_scheduler(config: ControllerConfig) -> anyhow::Result<()> {
    let state = initialize_state(
        config,
        InitOptions {
            run_migrations: false,
            seed_default_admin: false,
        },
    )
    .await?;

    tracing::info!("Mrs. Harris Scheduler を起動します");
    crate::scheduler::run_scheduler(state).await
}

pub async fn run_scheduler_once(config: ControllerConfig) -> anyhow::Result<()> {
    let state = initialize_state(
        config,
        InitOptions {
            run_migrations: false,
            seed_default_admin: false,
        },
    )
    .await?;

    tracing::info!("Mrs. Harris Scheduler を 1 回だけ実行します");
    crate::scheduler::run_scheduler_once(state).await
}

async fn initialize_state(
    config: ControllerConfig,
    options: InitOptions,
) -> anyhow::Result<AppState> {
    let pool = crate::db::create_pool(&config.database).await?;

    if options.run_migrations {
        crate::db::run_migrations(&config.database).await?;
    }

    if options.seed_default_admin {
        crate::db::users::seed_default_admin_if_needed(&pool).await?;
    }

    Ok(AppState {
        db: pool,
        config: Arc::new(config),
        scheduler_instance_id: uuid::Uuid::new_v4().to_string(),
    })
}

async fn serve_web(state: AppState) -> anyhow::Result<()> {
    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);
    let app = build_router(state);

    tracing::info!("Mrs. Harris Web listening on {}", addr);

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
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
