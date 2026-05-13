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
    models::Memory,
    AppState,
};

// ── Response shapes ───────────────────────────────────────────────────────────

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
    pub created_at: String,
    pub session_id: Option<String>,
    pub source_turn: Option<i32>,
}

impl From<Memory> for MemoryDto {
    fn from(m: Memory) -> Self {
        Self {
            id: m.id.to_string(),
            content: m.content,
            memory_type: m.memory_type,
            confidence: m.confidence,
            created_at: m.created_at.to_rfc3339(),
            session_id: m.session_id,
            source_turn: m.source_turn,
        }
    }
}

#[derive(Serialize)]
pub struct MemoryListResponse {
    pub memories: Vec<MemoryDto>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub agent_count: i64,
    pub memory_count: i64,
    /// Rough estimate: each stored memory avoids re-sending ~200 tokens per
    /// retrieval across the lifetime of the agent.
    pub tokens_saved_estimate: i64,
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateMemoryBody {
    pub content: String,
    pub memory_type: Option<String>,
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Embedding failed: {}", e)))?;

    let id = store::store_memory(
        &state,
        &agent_id,
        None,
        &body.content,
        body.memory_type.as_deref().unwrap_or("semantic"),
        1.0,
        embedding,
        None,
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
