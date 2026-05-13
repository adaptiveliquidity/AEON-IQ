use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

// ── OpenAI-compatible wire types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ── DB row types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Memory {
    pub id: Uuid,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
    pub source_turn: Option<i32>,
}

/// Internal row returned by vector-similarity search (includes computed distance).
#[derive(Debug, sqlx::FromRow)]
pub struct MemorySearchRow {
    pub id: Uuid,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
    pub source_turn: Option<i32>,
    pub distance: Option<f64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkingMemory {
    pub id: Uuid,
    pub agent_id: String,
    pub session_id: String,
    pub summary: Option<String>,
    pub turn_count: i32,
    pub updated_at: DateTime<Utc>,
}

// ── LLM extraction output ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct ExtractionResult {
    pub facts: Vec<String>,
    pub entities: Vec<EntityExtraction>,
    pub relations: Vec<RelationExtraction>,
    pub updated_summary: String,
    pub memory_type: String,
    pub confidence_low: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EntityExtraction {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub confidence: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RelationExtraction {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}
