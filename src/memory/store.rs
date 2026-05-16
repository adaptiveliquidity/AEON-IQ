use anyhow::Result;
use pgvector::Vector;
use sqlx::QueryBuilder;
use uuid::Uuid;

use crate::{
    models::{Memory, MemorySearchRow, WorkingMemory},
    AppState,
};

// ── Agent ─────────────────────────────────────────────────────────────────────

pub async fn upsert_agent(state: &AppState, agent_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO agents (agent_id) VALUES ($1) ON CONFLICT (agent_id) DO NOTHING",
    )
    .bind(agent_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

pub async fn count_agents(state: &AppState) -> Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*)::bigint FROM agents")
            .fetch_one(&state.db)
            .await?;
    Ok(row.0)
}

// ── Memories ──────────────────────────────────────────────────────────────────

pub async fn store_memory(
    state: &AppState,
    agent_id: &str,
    session_id: Option<&str>,
    content: &str,
    memory_type: &str,
    confidence: f32,
    embedding: Vec<f32>,
    source_turn: Option<i32>,
    provenance: &str,
) -> Result<Uuid> {
    store_memory_with_tier(
        state,
        agent_id,
        session_id,
        content,
        memory_type,
        confidence,
        embedding,
        source_turn,
        "L2",
        provenance,
    )
    .await
}

pub async fn store_memory_with_tier(
    state: &AppState,
    agent_id: &str,
    session_id: Option<&str>,
    content: &str,
    memory_type: &str,
    confidence: f32,
    embedding: Vec<f32>,
    source_turn: Option<i32>,
    tier: &str,
    provenance: &str,
) -> Result<Uuid> {
    let vec = Vector::from(embedding);
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO memories
            (agent_id, session_id, content, memory_type, confidence, embedding, source_turn, tier, provenance)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(agent_id)
    .bind(session_id)
    .bind(content)
    .bind(memory_type)
    .bind(confidence)
    .bind(vec)
    .bind(source_turn)
    .bind(tier)
    .bind(provenance)
    .fetch_one(&state.db)
    .await?;
    Ok(row.0)
}

/// Basic cosine-similarity search — used internally by the proxy.
/// Uses a CTE so the embedding vector is only bound once.
pub async fn search_memories(
    state: &AppState,
    agent_id: &str,
    embedding: &[f32],
    limit: i64,
) -> Result<Vec<MemorySearchRow>> {
    search_memories_filtered(state, agent_id, embedding, limit, 1.0, None, None).await
}

/// Full-featured search with optional filters on memory_type and session_id.
/// The `threshold` is an inclusive upper bound on cosine distance (lower = more similar).
/// Only live (non-tombstoned) memories are returned.
///
/// Decay scoring: `adjusted_dist = cosine_dist * (1 + decay_rate * days_stale)`
/// where `days_stale` is days since last access (or creation if never accessed).
/// When `decay_rate = 0.0` the formula collapses to pure cosine similarity.
pub async fn search_memories_filtered(
    state: &AppState,
    agent_id: &str,
    embedding: &[f32],
    limit: i64,
    threshold: f64,
    memory_type: Option<&str>,
    session_id: Option<&str>,
) -> Result<Vec<MemorySearchRow>> {
    let vec = Vector::from(embedding.to_vec());
    let decay_rate = state.config.memory_decay_rate;

    // Two-CTE pattern:
    //   base  — computes cosine distance and days_stale without binding the
    //           vector twice
    //   ranked — applies decay penalty; outer query filters and orders
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        r#"WITH base AS (
    SELECT id, agent_id, session_id, content, memory_type, confidence, provenance,
           created_at, source_turn,
           (embedding <=> "#,
    );
    qb.push_bind(vec);
    qb.push(
        r#")::double precision AS cosine_dist,
           EXTRACT(EPOCH FROM (NOW() - COALESCE(last_accessed_at, created_at))) / 86400.0 AS days_stale
    FROM memories
    WHERE agent_id = "#,
    );
    qb.push_bind(agent_id);
    qb.push(" AND archived_at IS NULL");

    if let Some(mt) = memory_type {
        qb.push(" AND memory_type = ");
        qb.push_bind(mt);
    }
    if let Some(sid) = session_id {
        qb.push(" AND session_id = ");
        qb.push_bind(sid);
    }

    qb.push("),\nranked AS (\n    SELECT *, cosine_dist * (1.0 + ");
    qb.push_bind(decay_rate);
    qb.push(
        r#" * days_stale) AS distance FROM base
)
SELECT id, agent_id, session_id, content, memory_type, confidence, provenance,
       created_at, source_turn, distance
FROM ranked WHERE distance < "#,
    );
    qb.push_bind(threshold);
    qb.push(" ORDER BY distance LIMIT ");
    qb.push_bind(limit);

    let rows = qb
        .build_query_as::<MemorySearchRow>()
        .fetch_all(&state.db)
        .await?;
    Ok(rows)
}

/// Bump the access counter and record the access timestamp for a list of IDs.
/// Called via `tokio::spawn` from the hot path — failures are silent.
pub async fn bump_access_counts(state: AppState, ids: Vec<Uuid>) {
    for id in ids {
        let _ = sqlx::query(
            "UPDATE memories SET access_count = access_count + 1, last_accessed_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&state.db)
        .await;
    }
}

pub async fn list_memories_for_agent(
    state: &AppState,
    agent_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Memory>> {
    let rows = sqlx::query_as::<_, Memory>(
        r#"
        SELECT id, agent_id, session_id, content, memory_type, confidence, provenance,
               created_at, source_turn
        FROM memories
        WHERE agent_id = $1
          AND archived_at IS NULL
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(agent_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

pub async fn delete_memory(state: &AppState, id: Uuid) -> Result<bool> {
    let r = sqlx::query("DELETE FROM memories WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn count_memories(state: &AppState) -> Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*)::bigint FROM memories WHERE archived_at IS NULL")
            .fetch_one(&state.db)
            .await?;
    Ok(row.0)
}

// ── Working memory (L1 per-session) ──────────────────────────────────────────

pub async fn get_working_memory(
    state: &AppState,
    agent_id: &str,
    session_id: &str,
) -> Result<Option<WorkingMemory>> {
    let wm = sqlx::query_as::<_, WorkingMemory>(
        "SELECT id, agent_id, session_id, summary, turn_count, updated_at
         FROM working_memory
         WHERE agent_id = $1 AND session_id = $2",
    )
    .bind(agent_id)
    .bind(session_id)
    .fetch_optional(&state.db)
    .await?;
    Ok(wm)
}

pub async fn upsert_working_memory(
    state: &AppState,
    agent_id: &str,
    session_id: &str,
    summary: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO working_memory (agent_id, session_id, summary, turn_count)
        VALUES ($1, $2, $3, 1)
        ON CONFLICT (agent_id, session_id) DO UPDATE
            SET summary    = EXCLUDED.summary,
                turn_count = working_memory.turn_count + 1,
                updated_at = NOW()
        "#,
    )
    .bind(agent_id)
    .bind(session_id)
    .bind(summary)
    .execute(&state.db)
    .await?;
    Ok(())
}

// ── Entities ──────────────────────────────────────────────────────────────────

pub async fn upsert_entity(
    state: &AppState,
    agent_id: &str,
    name: &str,
    entity_type: &str,
    confidence: f64,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO entities (agent_id, name, entity_type, confidence)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (agent_id, name) DO UPDATE
            SET entity_type = EXCLUDED.entity_type,
                confidence  = EXCLUDED.confidence,
                updated_at  = NOW()
        "#,
    )
    .bind(agent_id)
    .bind(name)
    .bind(entity_type)
    .bind(confidence as f32)
    .execute(&state.db)
    .await?;
    Ok(())
}

// ── Relations / Knowledge Graph ───────────────────────────────────────────────

pub async fn insert_relation(
    state: &AppState,
    agent_id: &str,
    subject: &str,
    predicate: &str,
    object: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO memory_graph (agent_id, subject, predicate, object)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(agent_id)
    .bind(subject)
    .bind(predicate)
    .bind(object)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Fetch all relations for an agent (used by the graph view).
pub async fn get_agent_relations(
    state: &AppState,
    agent_id: &str,
    limit: i64,
) -> Result<Vec<crate::models::RelationRow>> {
    let rows = sqlx::query_as::<_, crate::models::RelationRow>(
        r#"
        SELECT id, agent_id, subject, predicate, object, confidence, created_at
        FROM memory_graph
        WHERE agent_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(agent_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

// ── Archival helpers ──────────────────────────────────────────────────────────

/// Returns agent IDs that have at least one archivable L2 memory.
pub async fn agents_with_archivable_memories(
    state: &AppState,
    min_age_days: i64,
) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT agent_id
        FROM memories
        WHERE tier = 'L2'
          AND access_count = 0
          AND archived_at IS NULL
          AND created_at < NOW() - INTERVAL '1 day' * $1
        "#,
    )
    .bind(min_age_days)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Fetch up to `limit` archivable L2 memories for one agent.
pub async fn fetch_archivable_memories(
    state: &AppState,
    agent_id: &str,
    min_age_days: i64,
    limit: i64,
) -> Result<Vec<(Uuid, String)>> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, content
        FROM memories
        WHERE agent_id = $1
          AND tier = 'L2'
          AND access_count = 0
          AND archived_at IS NULL
          AND created_at < NOW() - INTERVAL '1 day' * $2
        ORDER BY created_at ASC
        LIMIT $3
        "#,
    )
    .bind(agent_id)
    .bind(min_age_days)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

/// Tombstone a list of memories by setting archived_at = NOW().
/// Originals remain in the DB for audit/lineage; they are excluded from all
/// retrieval queries via `AND archived_at IS NULL`.
pub async fn tombstone_memories(state: &AppState, ids: &[Uuid]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut qb: QueryBuilder<sqlx::Postgres> =
        QueryBuilder::new("UPDATE memories SET archived_at = NOW() WHERE id IN (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(")");
    let r = qb.build().execute(&state.db).await?;
    Ok(r.rows_affected())
}
