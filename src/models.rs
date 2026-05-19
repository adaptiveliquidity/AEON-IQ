use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

// ── OpenAI-compatible wire types (Issue 5) ────────────────────────────────────
//
// MessageContent is an untagged union so the proxy transparently passes through
// both simple string content and multimodal content arrays (image_url, tool
// results, function messages, etc.) without deserializing them.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain-text message — the common case.
    Text(String),
    /// Multimodal content array (images, tool results, etc.).
    /// Kept as raw JSON values so every future OpenAI content part type
    /// passes through without needing matching Rust types.
    Parts(Vec<serde_json::Value>),
}

impl MessageContent {
    /// Extract the plain-text portions for embedding / extraction.
    /// For multimodal messages, only `type: "text"` parts are included.
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Parts(parts) => parts
                .iter()
                .filter_map(|p| {
                    if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                        p.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self { Self::Text(s) }
}
impl From<&str> for MessageContent {
    fn from(s: &str) -> Self { Self::Text(s.to_string()) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self { role: "system".to_string(), content: content.into(), name: None }
    }

    pub fn content_text(&self) -> String {
        self.content.as_text()
    }
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
    pub provenance: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source_turn: Option<i32>,
    pub importance_score: f32,
    pub importance_source: String,
}

/// A memory row for export — includes `tier` (omitted from `Memory` for live queries).
/// The embedding vector is intentionally excluded: it must be re-computed on import.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MemoryExportRow {
    pub id: Uuid,
    pub session_id: Option<String>,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub provenance: String,
    pub tier: String,
    pub importance_score: f32,
    pub importance_source: String,
    pub created_at: DateTime<Utc>,
}

/// A tombstoned memory row — same as `Memory` plus the `archived_at` timestamp.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArchivedMemory {
    pub id: Uuid,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub provenance: String,
    pub created_at: DateTime<Utc>,
    pub source_turn: Option<i32>,
    pub importance_score: f32,
    pub importance_source: String,
    pub archived_at: DateTime<Utc>,
}

/// A row from the `archival_batches` table — one per L2→L3 compaction run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArchivalBatch {
    pub id: Uuid,
    pub agent_id: String,
    pub created_at: DateTime<Utc>,
    pub source_count: i32,
    pub l3_count: i32,
    pub status: String,
}

/// Row returned by vector-similarity search — includes the computed distance.
#[derive(Debug, sqlx::FromRow)]
pub struct MemorySearchRow {
    pub id: Uuid,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub provenance: String,
    pub created_at: DateTime<Utc>,
    pub source_turn: Option<i32>,
    pub importance_score: f32,
    pub importance_source: String,
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

/// A single row from the `sessions` table (one per active session).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionInfo {
    pub session_id: String,
    pub agent_id: String,
    pub turn_count: i32,
    pub updated_at: DateTime<Utc>,
    pub summary_preview: Option<String>,
}

/// A row from the `memory_conflicts` table — a flagged contradiction between two memories.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemoryConflict {
    pub id: Uuid,
    pub agent_id: String,
    pub memory_a: Option<Uuid>,
    pub memory_b: Option<Uuid>,
    pub reason: String,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A row from the `memory_graph` table (subject–predicate–object triple).
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct RelationRow {
    pub id: Uuid,
    pub agent_id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
}

// ── LLM extraction output (Issues 4 + 3) ─────────────────────────────────────

/// A single fact with its provenance label.
/// Provenance determines trust level and storage confidence.
#[derive(Debug, Deserialize, Serialize)]
pub struct ExtractedFact {
    pub content: String,
    /// `user_stated` — explicitly stated by the user (highest trust).
    /// `assistant_derived` — from the assistant response (lower trust,
    ///    may be hallucinated).
    /// `inferred` — implied but not directly stated (lowest trust).
    pub provenance: String,
    /// Line number in the numbered transcript where this fact is cited.
    pub cited_line: Option<u32>,
    pub confidence: f64,
    pub importance_score: Option<f64>,   // 0.0–1.0; None → default 0.5
    pub importance_source: Option<String>, // 'extractor' | 'user_stated' | 'agent_marked'
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ExtractionResult {
    pub facts: Vec<ExtractedFact>,
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
