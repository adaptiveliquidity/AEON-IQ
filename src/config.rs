use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    // ── Core ─────────────────────────────────────────────────────────────────
    pub database_url: String,
    pub upstream_base_url: String,
    pub port: u16,

    // ── Embeddings ────────────────────────────────────────────────────────────
    /// Server-side key for embedding and extraction calls.
    pub openai_api_key: Option<String>,
    pub embedding_model: String,
    pub embedding_dimension: i32,
    /// Base URL for the embeddings endpoint.  Defaults to `upstream_base_url`.
    /// Override when the LLM provider (e.g. Anthropic) doesn't serve embeddings
    /// and you want to point at OpenAI or a local model instead.
    pub embedding_base_url: String,

    // ── Extraction (MMU) ──────────────────────────────────────────────────────
    pub extractor_model: String,
    pub extractor_base_url: String,

    // ── Retrieval ─────────────────────────────────────────────────────────────
    /// Cosine distance upper bound; memories with distance ≥ this are dropped.
    pub retrieval_threshold: f64,
    /// Decay rate per day applied to cosine distance during retrieval.
    /// `adjusted_dist = cosine_dist * (1 + decay_rate * days_since_last_access)`
    /// Set to 0.0 (default) to disable decay and use pure cosine similarity.
    pub memory_decay_rate: f64,
    /// Importance weight in retrieval formula. 0.0 = disabled (default).
    /// adjusted_dist *= (1 + importance_boost_factor * (1 - importance_score))
    pub importance_boost_factor: f64,
    /// Per-retrieval boost added to importance_score (spacing-effect refresh). 0.0 = disabled.
    pub importance_refresh_boost: f32,

    // ── Management API security ───────────────────────────────────────────────
    /// When set, all /api/v1/* routes require this key via
    /// X-Management-Key or Authorization: Bearer headers.
    pub management_api_key: Option<String>,

    // ── LTM Archival ──────────────────────────────────────────────────────────
    /// How often the archival job runs in hours. 0 = disabled.
    pub archival_interval_hours: u64,
    /// L2 memories older than this many days with zero retrieval hits are
    /// candidates for compaction into L3.
    pub archival_min_age_days: u64,
    /// Minimum candidate count per agent before triggering compaction.
    pub archival_min_memories: usize,

    // ── Provider ──────────────────────────────────────────────────────────────
    /// Upstream LLM provider: "openai" (default) | "anthropic" | "gemini".
    /// Controls request translation and response parsing.
    pub upstream_provider: String,

    // ── Rate limiting ─────────────────────────────────────────────────────────
    /// Max proxy requests per minute per agent. 0 = disabled.
    pub rate_limit_rpm: u32,
    /// Token bucket burst size (max instantaneous quota).
    pub rate_limit_burst: u32,

    // ── Graph retrieval ───────────────────────────────────────────────────────
    /// When true, retrieval augments vector search with a graph walk over
    /// `memory_entity_links` and `memory_graph`. Default: false.
    pub graph_retrieval_enabled: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL is required")?,
            upstream_base_url: std::env::var("UPSTREAM_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com".to_string()),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("PORT must be a valid number")?,

            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            embedding_model: std::env::var("EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".to_string()),
            embedding_dimension: std::env::var("EMBEDDING_DIMENSION")
                .unwrap_or_else(|_| "1536".to_string())
                .parse()
                .context("EMBEDDING_DIMENSION must be a number")?,
            embedding_base_url: std::env::var("EMBEDDING_BASE_URL")
                .unwrap_or_else(|_| {
                    std::env::var("UPSTREAM_BASE_URL")
                        .unwrap_or_else(|_| "https://api.openai.com".to_string())
                }),

            extractor_model: std::env::var("EXTRACTOR_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            extractor_base_url: std::env::var("EXTRACTOR_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com".to_string()),

            retrieval_threshold: std::env::var("RETRIEVAL_THRESHOLD")
                .unwrap_or_else(|_| "0.80".to_string())
                .parse()
                .context("RETRIEVAL_THRESHOLD must be a float")?,
            memory_decay_rate: std::env::var("MEMORY_DECAY_RATE")
                .unwrap_or_else(|_| "0.0".to_string())
                .parse()
                .context("MEMORY_DECAY_RATE must be a float")?,
            importance_boost_factor: std::env::var("IMPORTANCE_BOOST_FACTOR")
                .unwrap_or_else(|_| "0.0".to_string())
                .parse()
                .context("IMPORTANCE_BOOST_FACTOR must be a float")?,
            importance_refresh_boost: std::env::var("IMPORTANCE_REFRESH_BOOST")
                .unwrap_or_else(|_| "0.05".to_string())
                .parse()
                .context("IMPORTANCE_REFRESH_BOOST must be a float")?,

            management_api_key: std::env::var("MANAGEMENT_API_KEY").ok(),

            archival_interval_hours: std::env::var("ARCHIVAL_INTERVAL_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .context("ARCHIVAL_INTERVAL_HOURS must be a number")?,
            archival_min_age_days: std::env::var("ARCHIVAL_MIN_AGE_DAYS")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .context("ARCHIVAL_MIN_AGE_DAYS must be a number")?,
            archival_min_memories: std::env::var("ARCHIVAL_MIN_MEMORIES")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .context("ARCHIVAL_MIN_MEMORIES must be a number")?,

            upstream_provider: std::env::var("UPSTREAM_PROVIDER")
                .unwrap_or_else(|_| "openai".to_string()),

            rate_limit_rpm: std::env::var("RATE_LIMIT_RPM")
                .unwrap_or_else(|_| "0".to_string())
                .parse()
                .context("RATE_LIMIT_RPM must be a non-negative integer")?,
            rate_limit_burst: std::env::var("RATE_LIMIT_BURST")
                .unwrap_or_else(|_| "20".to_string())
                .parse()
                .context("RATE_LIMIT_BURST must be a non-negative integer")?,

            graph_retrieval_enabled: std::env::var("GRAPH_RETRIEVAL_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
        })
    }
}
