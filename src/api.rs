use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    embeddings::embed_text,
    memory::store,
    models::{ArchivedMemory, Memory, RelationRow},
    AppState,
};

// ── Shared response shapes ────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub memory_count: i64,
}

#[derive(Serialize)]
pub struct AgentListResponse {
    pub agents: Vec<AgentInfo>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct MemoryDto {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub provenance: String,
    pub created_at: String,
    pub session_id: Option<String>,
    pub source_turn: Option<i32>,
    pub importance_score: f32,
    pub importance_source: String,
}

impl From<Memory> for MemoryDto {
    fn from(m: Memory) -> Self {
        Self {
            id: m.id.to_string(),
            content: m.content,
            memory_type: m.memory_type,
            confidence: m.confidence,
            provenance: m.provenance,
            created_at: m.created_at.to_rfc3339(),
            session_id: m.session_id,
            source_turn: m.source_turn,
            importance_score: m.importance_score,
            importance_source: m.importance_source,
        }
    }
}

#[derive(Serialize)]
pub struct MemoryListResponse {
    pub memories: Vec<MemoryDto>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct ArchivedMemoryDto {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub provenance: String,
    pub created_at: String,
    pub session_id: Option<String>,
    pub source_turn: Option<i32>,
    pub importance_score: f32,
    pub importance_source: String,
    pub archived_at: String,
}

impl From<ArchivedMemory> for ArchivedMemoryDto {
    fn from(m: ArchivedMemory) -> Self {
        Self {
            id: m.id.to_string(),
            content: m.content,
            memory_type: m.memory_type,
            confidence: m.confidence,
            provenance: m.provenance,
            created_at: m.created_at.to_rfc3339(),
            session_id: m.session_id,
            source_turn: m.source_turn,
            importance_score: m.importance_score,
            importance_source: m.importance_source,
            archived_at: m.archived_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
pub struct ArchivedMemoryListResponse {
    pub memories: Vec<ArchivedMemoryDto>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub agent_count: i64,
    pub memory_count: i64,
    pub tokens_saved_estimate: i64,
}

// ── Query params / bodies ─────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateMemoryBody {
    pub content: String,
    pub memory_type: Option<String>,
}

// ── Semantic search types ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MemorySearchRequest {
    pub agent_id: String,
    pub query: String,
    /// Maximum results (default 20, max 100).
    pub limit: Option<i64>,
    /// Cosine distance upper bound — lower = more similar (default 0.80).
    pub threshold: Option<f64>,
    /// Filter by memory_type (episodic | semantic | procedural).
    pub memory_type: Option<String>,
    /// Filter to a specific session.
    pub session_id: Option<String>,
    /// When true, include subject–predicate–object relations in the response.
    pub include_relations: Option<bool>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    /// 1.0 = identical, 0.0 = maximally dissimilar.
    pub similarity: f32,
    pub created_at: String,
    pub source_turn: Option<i32>,
    pub session_id: Option<String>,
    pub importance_score: f32,
}

#[derive(Serialize)]
pub struct RelationDto {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f32,
    pub created_at: String,
}

impl From<RelationRow> for RelationDto {
    fn from(r: RelationRow) -> Self {
        Self {
            id: r.id.to_string(),
            subject: r.subject,
            predicate: r.predicate,
            object: r.object,
            confidence: r.confidence,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
pub struct MemorySearchResponse {
    pub results: Vec<SearchResult>,
    pub relations: Vec<RelationDto>,
    pub total: usize,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<AgentListResponse>, (StatusCode, String)> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT agent_id FROM agents ORDER BY created_at DESC LIMIT 200")
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agents = Vec::with_capacity(rows.len());
    for (agent_id,) in &rows {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*)::bigint FROM memories WHERE agent_id = $1")
                .bind(agent_id)
                .fetch_one(&state.db)
                .await
                .unwrap_or((0,));
        agents.push(AgentInfo {
            agent_id: agent_id.clone(),
            memory_count: count.0,
        });
    }

    Ok(Json(AgentListResponse {
        total: agents.len() as i64,
        agents,
    }))
}

pub async fn list_memories(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<MemoryListResponse>, (StatusCode, String)> {
    let limit = pagination.limit.unwrap_or(50).min(200);
    let offset = pagination.offset.unwrap_or(0);

    let memories = store::list_memories_for_agent(&state, &agent_id, limit, offset)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(*)::bigint FROM memories WHERE agent_id = $1")
            .bind(&agent_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or((0,));

    Ok(Json(MemoryListResponse {
        total: total.0,
        memories: memories.into_iter().map(MemoryDto::from).collect(),
    }))
}

pub async fn create_memory(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(body): Json<CreateMemoryBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    store::upsert_agent(&state, &agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let embedding = embed_text(&state, &body.content)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Embedding: {}", e)))?;

    let id = store::store_memory(
        &state,
        &agent_id,
        None,
        &body.content,
        body.memory_type.as_deref().unwrap_or("semantic"),
        1.0,
        embedding,
        None,
        "user_stated",
        1.0_f32,
        "user_stated",
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "id": id.to_string(), "success": true })))
}

pub async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let uuid =
        Uuid::parse_str(&id).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let deleted = store::delete_memory(&state, uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

/// POST /api/v1/memories/search
/// Embed the query text, run pgvector HNSW search, return ranked results + optional graph data.
pub async fn search_memories_semantic(
    State(state): State<AppState>,
    Json(req): Json<MemorySearchRequest>,
) -> Result<Json<MemorySearchResponse>, (StatusCode, String)> {
    let limit = req.limit.unwrap_or(20).min(100);
    let threshold = req.threshold.unwrap_or(state.config.retrieval_threshold);

    let embedding = embed_text(&state, &req.query)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Embedding: {}", e)))?;

    let search_start = std::time::Instant::now();
    let rows = store::search_memories_filtered(
        &state,
        &req.agent_id,
        &embedding,
        limit,
        threshold,
        req.memory_type.as_deref(),
        req.session_id.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    state
        .metrics
        .vector_search_secs
        .observe(search_start.elapsed().as_secs_f64());

    let relations: Vec<RelationDto> = if req.include_relations.unwrap_or(false) {
        store::get_agent_relations(&state, &req.agent_id, 200)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(RelationDto::from)
            .collect()
    } else {
        vec![]
    };

    let total = rows.len();
    let results = rows
        .into_iter()
        .map(|r| SearchResult {
            id: r.id.to_string(),
            content: r.content,
            memory_type: r.memory_type,
            confidence: r.confidence,
            similarity: (1.0 - r.distance.unwrap_or(1.0)) as f32,
            created_at: r.created_at.to_rfc3339(),
            source_turn: r.source_turn,
            session_id: r.session_id,
            importance_score: r.importance_score,
        })
        .collect();

    Ok(Json(MemorySearchResponse {
        results,
        relations,
        total,
    }))
}

pub async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<StatsResponse>, (StatusCode, String)> {
    let agent_count = store::count_agents(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let memory_count = store::count_memories(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(StatsResponse {
        agent_count,
        memory_count,
        tokens_saved_estimate: memory_count * 200,
    }))
}

pub async fn list_archived_memories(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ArchivedMemoryListResponse>, (StatusCode, String)> {
    let limit = pagination.limit.unwrap_or(50).min(200);
    let offset = pagination.offset.unwrap_or(0);

    let memories = store::list_archived_memories(&state, &agent_id, limit, offset)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM memories WHERE agent_id = $1 AND archived_at IS NOT NULL",
    )
    .bind(&agent_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    Ok(Json(ArchivedMemoryListResponse {
        total: total.0,
        memories: memories.into_iter().map(ArchivedMemoryDto::from).collect(),
    }))
}

pub async fn restore_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let uuid = Uuid::parse_str(&id).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let restored = store::restore_memory(&state, uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if restored {
        Ok(Json(serde_json::json!({ "restored": true })))
    } else {
        Err((StatusCode::NOT_FOUND, format!("memory {} not found or not archived", id)))
    }
}
