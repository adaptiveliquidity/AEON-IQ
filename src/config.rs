use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub upstream_base_url: String,
    pub port: u16,
    /// Server-side key used for embeddings and extraction
    pub openai_api_key: Option<String>,
    pub embedding_model: String,
    pub embedding_dimension: i32,
    pub extractor_model: String,
    pub extractor_base_url: String,
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
        })
    }
}
