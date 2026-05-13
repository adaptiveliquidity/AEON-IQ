mod api;
mod config;
mod db;
mod embeddings;
mod memory;
mod models;
mod proxy;

use std::sync::Arc;

use axum::{
    routing::{delete, get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use config::Config;
pub use db::DbPool;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: DbPool,
    pub http_client: reqwest::Client,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG")
                    .unwrap_or_else(|_| "memoryos_kernel=debug,tower_http=info".into()),
            ),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Arc::new(Config::from_env()?);
    tracing::info!("MemoryOS Kernel starting on port {}", config.port);

    let db = db::connect(&config.database_url).await?;
    db::run_migrations(&db).await?;
    tracing::info!("Database migrations applied");

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let state = AppState {
        config: config.clone(),
        db,
        http_client,
    };

    let app = Router::new()
        // ── OpenAI-compatible proxy ──────────────────────────────────────────
        .route("/v1/chat/completions", post(proxy::handle_chat_completions))
        .route("/v1/models", get(proxy::handle_models))
        // ── Management REST API ──────────────────────────────────────────────
        .route("/api/v1/agents", get(api::list_agents))
        .route("/api/v1/agents/:agent_id/memories", get(api::list_memories))
        .route("/api/v1/agents/:agent_id/memories", post(api::create_memory))
        .route("/api/v1/memories/:id", delete(api::delete_memory))
        .route("/api/v1/stats", get(api::get_stats))
        // ── Health ───────────────────────────────────────────────────────────
        .route("/health", get(|| async { "OK" }))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
