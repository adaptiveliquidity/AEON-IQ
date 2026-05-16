mod api;
mod archival;
mod auth;
mod config;
mod db;
mod embeddings;
mod memory;
mod metrics;
mod models;
mod proxy;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use config::Config;
pub use db::DbPool;

#[derive(Clone)]
pub struct AppState {
    pub config:      Arc<Config>,
    pub db:          DbPool,
    pub http_client: reqwest::Client,
    pub metrics:     Arc<metrics::Metrics>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "memoryos_kernel=debug,tower_http=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Arc::new(Config::from_env()?);
    tracing::info!("MemoryOS Kernel starting on port {}", config.port);

    if config.management_api_key.is_none() {
        tracing::warn!(
            "MANAGEMENT_API_KEY is not set — management endpoints are unauthenticated. \
             Set this env var before exposing the service publicly."
        );
    }

    let db = db::connect(&config.database_url).await?;
    db::run_migrations(&db).await?;
    tracing::info!("Database migrations applied");

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let m = metrics::Metrics::new()?;
    let state = AppState {
        config:      config.clone(),
        db,
        http_client,
        metrics:     Arc::new(m),
    };

    // ── Background jobs ───────────────────────────────────────────────────────
    if config.archival_interval_hours > 0 {
        tokio::spawn(archival::run_job(state.clone()));
    }

    // ── Management sub-router (Issue 2: authenticated) ────────────────────────
    let management = Router::new()
        .route("/agents",                     get(api::list_agents))
        .route("/agents/:agent_id/memories",  get(api::list_memories))
        .route("/agents/:agent_id/memories",  post(api::create_memory))
        .route("/memories/search",            post(api::search_memories_semantic))
        .route("/memories/:id",               delete(api::delete_memory))
        .route("/stats",                      get(api::get_stats))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::check_management_key,
        ));

    // ── Root router ───────────────────────────────────────────────────────────
    let app = Router::new()
        // OpenAI-compatible proxy (no management auth — uses upstream key)
        .route("/v1/chat/completions", post(proxy::handle_chat_completions))
        .route("/v1/models",           get(proxy::handle_models))
        // Management API (authenticated)
        .nest("/api/v1", management)
        // Observability (public — typically behind internal network)
        .route("/metrics", get(metrics::handle_metrics))
        .route("/health",  get(|| async { "OK" }))
        // Outer middleware stack
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics::track_request,
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
