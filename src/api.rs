use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    archival,
    embeddings::embed_text,
    memory::store,
    models::{ArchivalBatch, ArchivedMemory, Memory, MemoryConflict, RelationRow, SessionInfo, WorkingMemory},
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
    pub updated_at: String,
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
            updated_at: m.updated_at.to_rfc3339(),
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
    pub offset: i64,
    pub limit: i64,
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

// ── Archival batch types ──────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ArchivalBatchDto {
    pub id: String,
    pub agent_id: String,
    pub created_at: String,
    pub source_count: i32,
    pub l3_count: i32,
    pub status: String,
}

impl From<ArchivalBatch> for ArchivalBatchDto {
    fn from(b: ArchivalBatch) -> Self {
        Self {
            id: b.id.to_string(),
            agent_id: b.agent_id,
            created_at: b.created_at.to_rfc3339(),
            source_count: b.source_count,
            l3_count: b.l3_count,
            status: b.status,
        }
    }
}

#[derive(Serialize)]
pub struct ArchivalBatchListResponse {
    pub batches: Vec<ArchivalBatchDto>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub agent_count: i64,
    pub memory_count: i64,
    pub tokens_saved_estimate: i64,
}

// ── Session types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SessionDto {
    pub session_id: String,
    pub turn_count: i32,
    pub updated_at: String,
    pub summary_preview: Option<String>,
}

impl From<SessionInfo> for SessionDto {
    fn from(s: SessionInfo) -> Self {
        Self {
            session_id: s.session_id,
            turn_count: s.turn_count,
            updated_at: s.updated_at.to_rfc3339(),
            summary_preview: s.summary_preview,
        }
    }
}

#[derive(Serialize)]
pub struct SessionDetailDto {
    pub session_id: String,
    pub turn_count: i32,
    pub updated_at: String,
    pub summary: Option<String>,
}

impl From<WorkingMemory> for SessionDetailDto {
    fn from(w: WorkingMemory) -> Self {
        Self {
            session_id: w.session_id,
            turn_count: w.turn_count,
            updated_at: w.updated_at.to_rfc3339(),
            summary: w.summary,
        }
    }
}

#[derive(Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionDto>,
    pub total: usize,
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

#[derive(Deserialize)]
pub struct PatchMemoryBody {
    pub content: String,
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

pub async fn delete_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let deleted = store::delete_agent(&state, &agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, format!("agent '{}' not found", agent_id)))
    }
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
        offset,
        limit,
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

pub async fn patch_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PatchMemoryBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let uuid = Uuid::parse_str(&id).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let embedding = embed_text(&state, &body.content)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Embedding: {}", e)))?;

    let updated = store::update_memory_content(&state, uuid, &body.content, embedding)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if updated {
        Ok(Json(serde_json::json!({ "updated": true })))
    } else {
        Err((StatusCode::NOT_FOUND, format!("memory {} not found or archived", id)))
    }
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

// ── Archival batch handlers ───────────────────────────────────────────────────

pub async fn list_archival_batches(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ArchivalBatchListResponse>, (StatusCode, String)> {
    let limit = pagination.limit.unwrap_or(50).min(200);
    let offset = pagination.offset.unwrap_or(0);

    let batches = store::list_archival_batches(&state, &agent_id, limit, offset)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = store::count_archival_batches(&state, &agent_id)
        .await
        .unwrap_or(batches.len() as i64);

    Ok(Json(ArchivalBatchListResponse {
        total,
        batches: batches.into_iter().map(ArchivalBatchDto::from).collect(),
    }))
}

pub async fn restore_archival_batch(
    State(state): State<AppState>,
    Path(batch_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let uuid = Uuid::parse_str(&batch_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let result = store::restore_archival_batch(&state, uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match result {
        Some(r) => Ok(Json(serde_json::json!({
            "restored": true,
            "batch_id": batch_id,
            "l2_restored": r.l2_restored,
            "l3_tombstoned": r.l3_tombstoned,
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("batch {} not found or already restored", batch_id),
        )),
    }
}

// ── Archival trigger ──────────────────────────────────────────────────────────

/// POST /api/v1/agents/:id/archival/trigger
///
/// Runs one compaction cycle for this agent synchronously.  Returns the batch
/// info if compaction ran, or a message explaining why it was skipped.
pub async fn trigger_archival(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let min_age  = state.config.archival_min_age_days as i64;
    let min_mems = state.config.archival_min_memories;

    match archival::archive_agent(&state, &agent_id, min_age, min_mems).await {
        Ok(Some(r)) => Ok(Json(serde_json::json!({
            "batch_id":     r.batch_id.to_string(),
            "source_count": r.source_count,
            "l3_count":     r.l3_count,
            "status":       r.status,
        }))),
        Ok(None) => Ok(Json(serde_json::json!({
            "status":  "skipped",
            "reason":  format!(
                "fewer than {} archivable memories older than {} day(s)",
                min_mems, min_age
            ),
        }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

// ── Session (working memory) handlers ────────────────────────────────────────

/// GET /api/v1/agents/:id/sessions
pub async fn list_sessions(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<SessionListResponse>, (StatusCode, String)> {
    let sessions = store::list_sessions_for_agent(&state, &agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = sessions.len();
    Ok(Json(SessionListResponse {
        total,
        sessions: sessions.into_iter().map(SessionDto::from).collect(),
    }))
}

/// GET /api/v1/agents/:id/sessions/:session_id
pub async fn get_session(
    State(state): State<AppState>,
    Path((agent_id, session_id)): Path<(String, String)>,
) -> Result<Json<SessionDetailDto>, (StatusCode, String)> {
    let wm = store::get_session_detail(&state, &agent_id, &session_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match wm {
        Some(w) => Ok(Json(SessionDetailDto::from(w))),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("no session {} for agent {}", session_id, agent_id),
        )),
    }
}

/// DELETE /api/v1/agents/:id/sessions/:session_id
pub async fn delete_session(
    State(state): State<AppState>,
    Path((agent_id, session_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let deleted = store::delete_session_working_memory(&state, &agent_id, &session_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if deleted {
        Ok(Json(serde_json::json!({ "deleted": true })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            format!("no working memory for session {} / agent {}", session_id, agent_id),
        ))
    }
}

// ── Conflict types & handlers ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConflictDto {
    pub id: String,
    pub memory_a: Option<String>,
    pub memory_b: Option<String>,
    pub reason: String,
    pub resolved_at: Option<String>,
    pub resolution: Option<String>,
    pub created_at: String,
}

impl From<MemoryConflict> for ConflictDto {
    fn from(c: MemoryConflict) -> Self {
        Self {
            id: c.id.to_string(),
            memory_a: c.memory_a.map(|u| u.to_string()),
            memory_b: c.memory_b.map(|u| u.to_string()),
            reason: c.reason,
            resolved_at: c.resolved_at.map(|t| t.to_rfc3339()),
            resolution: c.resolution,
            created_at: c.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
pub struct ConflictListResponse {
    pub conflicts: Vec<ConflictDto>,
    pub total: usize,
}

#[derive(Deserialize)]
pub struct ResolveConflictBody {
    /// One of: keep_a | keep_b | keep_both | dismissed
    pub resolution: String,
}

#[derive(Deserialize, Default)]
pub struct ConflictListParams {
    /// When "true", include already-resolved conflicts.
    pub include_resolved: Option<String>,
}

/// GET /api/v1/agents/:id/conflicts
pub async fn list_conflicts(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<ConflictListParams>,
) -> Result<Json<ConflictListResponse>, (StatusCode, String)> {
    let include_resolved = params
        .include_resolved
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let conflicts = store::list_conflicts(&state, &agent_id, include_resolved)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = conflicts.len();
    Ok(Json(ConflictListResponse {
        total,
        conflicts: conflicts.into_iter().map(ConflictDto::from).collect(),
    }))
}

/// POST /api/v1/conflicts/:id/resolve
pub async fn resolve_conflict(
    State(state): State<AppState>,
    Path(conflict_id): Path<Uuid>,
    Json(body): Json<ResolveConflictBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let valid = ["keep_a", "keep_b", "keep_both", "dismissed"];
    if !valid.contains(&body.resolution.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "resolution must be one of: {}",
                valid.join(", ")
            ),
        ));
    }

    let resolved = store::resolve_conflict(&state, conflict_id, &body.resolution)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if resolved {
        Ok(Json(serde_json::json!({ "resolved": true, "resolution": body.resolution })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            format!("conflict {} not found or already resolved", conflict_id),
        ))
    }
}

// ── Bulk operation types & handler ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BulkOperationRequest {
    /// "archive" (tombstone) or "delete" (hard-delete)
    pub action: String,
    pub filter: BulkFilter,
}

#[derive(Deserialize, Default)]
pub struct BulkFilter {
    pub session_id: Option<String>,
    pub memory_type: Option<String>,
    /// ISO 8601 timestamp — memories created before this are included.
    pub older_than: Option<String>,
    pub importance_below: Option<f32>,
}

/// POST /api/v1/agents/:id/memories/bulk
pub async fn bulk_operation(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(body): Json<BulkOperationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if body.action != "archive" && body.action != "delete" {
        return Err((
            StatusCode::BAD_REQUEST,
            "action must be \"archive\" or \"delete\"".to_string(),
        ));
    }

    let older_than: Option<DateTime<Utc>> = body
        .filter
        .older_than
        .as_deref()
        .map(|s| s.parse::<DateTime<Utc>>())
        .transpose()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid older_than: {}", e)))?;

    let affected = store::bulk_operation_memories(
        &state,
        &agent_id,
        &body.action,
        body.filter.session_id.as_deref(),
        body.filter.memory_type.as_deref(),
        older_than,
        body.filter.importance_below,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "affected": affected })))
}
