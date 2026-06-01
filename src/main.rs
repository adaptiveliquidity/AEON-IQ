mod api;
mod archival;
mod auth;
mod config;
mod db;
mod embeddings;
mod memory;
mod metrics;
mod models;
mod providers;
mod proxy;
mod rate_limit;
mod rmk_worker;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Router,
};
use axum::extract::DefaultBodyLimit;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use config::Config;
pub use db::DbPool;

#[derive(Clone)]
pub struct AppState {
    pub config:       Arc<Config>,
    pub db:           DbPool,
    pub http_client:  reqwest::Client,
    pub metrics:      Arc<metrics::Metrics>,
    pub provider:     providers::Provider,
    pub rate_limiter: Arc<rate_limit::RateLimiter>,
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

    // Refuse to start if the management API would be unauthenticated without
    // an explicit operator acknowledgement.  This prevents accidental exposure
    // of /api/v1/* routes in production when the env var is simply forgotten.
    if config.management_api_key.is_none() {
        if config.allow_unauth_management {
            tracing::warn!(
                "MANAGEMENT_API_KEY is not set and ALLOW_UNAUTH_MANAGEMENT=true — \
                 management endpoints are unauthenticated. Do not expose this service \
                 publicly without a management key."
            );
        } else {
            anyhow::bail!(
                "MANAGEMENT_API_KEY is not set. Either:\n  \
                 • Set MANAGEMENT_API_KEY=<secret> to require authentication, or\n  \
                 • Set ALLOW_UNAUTH_MANAGEMENT=true to explicitly allow unauthenticated \
                 access (development only)."
            );
        }
    }

    let provider = providers::Provider::from_str(&config.upstream_provider);
    tracing::info!("Upstream provider: {:?}", provider);

    if config.rate_limit_rpm > 0 {
        tracing::info!(
            "Rate limiting enabled: {} RPM per agent, burst {}",
            config.rate_limit_rpm, config.rate_limit_burst,
        );
    }

    let db = db::connect(
        &config.database_url,
        config.db_max_connections,
        config.db_acquire_timeout_secs,
        config.db_idle_timeout_secs,
    )
    .await?;
    db::run_migrations(&db).await?;
    tracing::info!("Database migrations applied");

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let m = metrics::Metrics::new()?;
    let rate_limiter = Arc::new(rate_limit::RateLimiter::new(
        config.rate_limit_rpm,
        config.rate_limit_burst,
    ));

    let state = AppState {
        config:      config.clone(),
        db,
        http_client,
        metrics:     Arc::new(m),
        provider,
        rate_limiter,
    };

    // ── Background jobs ───────────────────────────────────────────────────────
    if config.archival_interval_hours > 0 {
        tokio::spawn(archival::run_job(state.clone()));
    }
    if config.rmk_config.enabled {
        tokio::spawn(rmk_worker::run_policy_update_job(state.clone()));
        tokio::spawn(rmk_worker::run_co_access_decay_job(state.clone()));
        tokio::spawn(rmk_worker::run_pressure_sweep_job(state.clone()));
    } else if config.amp_config.enabled {
        // AMP co-access decay and pressure sweep run even without RMK.
        tokio::spawn(rmk_worker::run_co_access_decay_job(state.clone()));
        tokio::spawn(rmk_worker::run_pressure_sweep_job(state.clone()));
    }

    // ── Management sub-router (authenticated) ─────────────────────────────────
    let management = Router::new()
        .route("/agents",                                       get(api::list_agents))
        .route("/agents/:agent_id",                             delete(api::delete_agent))
        .route("/agents/:agent_id/memories",                    get(api::list_memories))
        .route("/agents/:agent_id/memories",                    post(api::create_memory))
        .route("/agents/:agent_id/memories/at",                 get(api::memories_at_timestamp))
        .route("/agents/:agent_id/memories/diff",               get(api::memories_diff))
        .route("/agents/:agent_id/memories/bulk",               post(api::bulk_operation))
        .route("/agents/:agent_id/memories/archived",           get(api::list_archived_memories))
        .route("/agents/:agent_id/archival/batches",            get(api::list_archival_batches))
        .route("/agents/:agent_id/archival/trigger",            post(api::trigger_archival))
        .route("/agents/:agent_id/export",                      get(api::export_memories))
        .route("/agents/:agent_id/import",                      post(api::import_memories))
        .route("/agents/:agent_id/sessions",                    get(api::list_sessions))
        .route("/agents/:agent_id/sessions/:session_id",        get(api::get_session))
        .route("/agents/:agent_id/sessions/:session_id",        delete(api::delete_session))
        .route("/agents/:agent_id/retrievals",                  get(api::list_retrievals))
        .route("/agents/:agent_id/conflicts",                   get(api::list_conflicts))
        .route("/conflicts/:conflict_id/resolve",               post(api::resolve_conflict))
        .route("/memories/search",                              post(api::search_memories_semantic))
        .route("/memories/:id",                                 patch(api::patch_memory))
        .route("/memories/:id",                                 delete(api::delete_memory))
        .route("/memories/:id/restore",                         post(api::restore_memory))
        .route("/memories/:id/versions",                        get(api::list_memory_versions))
        .route("/memories/:id/status",                          patch(api::patch_memory_status))
        .route("/memories/:id/sensitivity",                     patch(api::patch_memory_sensitivity))
        .route("/archival/batches/:batch_id/restore",           post(api::restore_archival_batch))
        .route("/stats",                                        get(api::get_stats))
        .route("/feedback",                                     post(api::post_feedback))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::check_management_key,
        ));

    // ── Root router ───────────────────────────────────────────────────────────
    let app = Router::new()
        .route("/v1/chat/completions", post(proxy::handle_chat_completions))
        .route("/v1/models",           get(proxy::handle_models))
        .nest("/api/v1", management)
        .route("/metrics", get(metrics::handle_metrics))
        .route("/health",  get(|| async { "OK" }))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics::track_request,
        ))
        .layer(DefaultBodyLimit::max(config.max_body_bytes))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
