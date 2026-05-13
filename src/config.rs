use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    // ── Core ─────────────────────────────────────────────────────────────────
    pub database_url: String,
    pub upstream_base_url: String,
    pub port: u16,

    // ── Embeddings ────────────────────────────────────────────────────────────
    /// Server-side key used for embeddings and extraction calls.
    pub openai_api_key: Option<String>,
    pub embedding_model: String,
    pub embedding_dimension: i32,

    // ── Extraction (MMU) ──────────────────────────────────────────────────────
    pub extractor_model: String,
    pub extractor_base_url: String,

    // ── Retrieval ─────────────────────────────────────────────────────────────
    /// Cosine distance threshold: memories with distance ≥ this value are
    /// considered irrelevant and filtered out.
    pub retrieval_threshold: f64,

    // ── LTM Archival ──────────────────────────────────────────────────────────
    /// How often the archival job runs (hours). 0 = disabled.
    pub archival_interval_hours: u64,
    /// L2 memories older than this many days without a retrieval hit
    /// become candidates for compaction into L3.
    pub archival_min_age_days: u64,
    /// Minimum candidate count per agent before triggering compaction.
    pub archival_min_memories: usize,
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

            extractor_model: std::env::var("EXTRACTOR_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            extractor_base_url: std::env::var("EXTRACTOR_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com".to_string()),

            retrieval_threshold: std::env::var("RETRIEVAL_THRESHOLD")
                .unwrap_or_else(|_| "0.80".to_string())
                .parse()
                .context("RETRIEVAL_THRESHOLD must be a float")?,

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
        })
    }
}
