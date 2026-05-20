use anyhow::Result;
use pgvector::Vector;
use sqlx::QueryBuilder;
use uuid::Uuid;

use crate::{
    models::{ArchivalBatch, ArchivedMemory, Memory, MemorySearchRow, WorkingMemory},
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
    importance_score: f32,
    importance_source: &str,
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
        importance_score,
        importance_source,
        None,
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
    importance_score: f32,
    importance_source: &str,
    archival_batch_id: Option<Uuid>,
) -> Result<Uuid> {
    let vec = Vector::from(embedding);
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO memories
            (agent_id, session_id, content, memory_type, confidence, embedding,
             source_turn, tier, provenance, importance_score, importance_source,
             archival_batch_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
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
    .bind(importance_score)
    .bind(importance_source)
    .bind(archival_batch_id)
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
/// Three-factor scoring:
///   `adjusted_dist = cosine_dist
///       * (1 + decay_rate * days_stale)
///       * (1 + importance_boost * (1 - importance_score))`
/// When `decay_rate = 0.0` and `importance_boost = 0.0` (defaults) the formula
/// collapses to pure cosine similarity.
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
    let importance_boost = state.config.importance_boost_factor;

    // Two-CTE pattern:
    //   base   — computes cosine distance, days_stale, and surfaces importance columns
    //   ranked — applies decay + importance penalty; outer query filters and orders
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        r#"WITH base AS (
    SELECT id, agent_id, session_id, content, memory_type, confidence, provenance,
           created_at, source_turn, importance_score, importance_source,
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

    qb.push("),\nranked AS (\n    SELECT *,\n           cosine_dist\n           * (1.0 + ");
    qb.push_bind(decay_rate);
    qb.push(" * days_stale)\n           * (1.0 + ");
    qb.push_bind(importance_boost);
    qb.push(
        r#" * (1.0 - importance_score::double precision))
           AS distance
    FROM base
)
SELECT id, agent_id, session_id, content, memory_type, confidence, provenance,
       created_at, source_turn, importance_score, importance_source, distance
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

/// Bump the access counter, record the access timestamp, and apply a small
/// importance refresh boost for a list of IDs.
/// Called via `tokio::spawn` from the hot path — failures are silent.
pub async fn bump_access_counts(state: AppState, ids: Vec<Uuid>) {
    let refresh_boost = state.config.importance_refresh_boost;
    for id in ids {
        let _ = sqlx::query(
            "UPDATE memories \
             SET access_count = access_count + 1, \
                 last_accessed_at = NOW(), \
                 importance_score = LEAST(1.0, importance_score + $2) \
             WHERE id = $1",
        )
        .bind(id)
        .bind(refresh_boost)
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
               created_at, source_turn, importance_score, importance_source
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

/// List tombstoned (archived) memories for an agent, newest-archived first.
pub async fn list_archived_memories(
    state: &AppState,
    agent_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<crate::models::ArchivedMemory>> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, agent_id, session_id, content, memory_type, confidence, provenance,
               created_at, source_turn, importance_score, importance_source, archived_at
        FROM memories
        WHERE agent_id = $1
          AND archived_at IS NOT NULL
        ORDER BY archived_at DESC
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

/// Restore a tombstoned memory by clearing `archived_at`.
/// Returns `true` if the memory existed and was archived, `false` otherwise.
pub async fn restore_memory(state: &AppState, id: Uuid) -> Result<bool> {
    let r = sqlx::query(
        "UPDATE memories SET archived_at = NULL WHERE id = $1 AND archived_at IS NOT NULL",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(r.rows_affected() > 0)
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

/// Upsert an entity, returning its UUID.
///
/// Disambiguation (4.2): before inserting, check whether any existing entity
/// for this agent has a name within Levenshtein distance ≤ 2.  If so, merge
/// into the existing entity by using its canonical name — preventing "Alex"
/// and "Alexander" from being stored as two separate nodes.
pub async fn upsert_entity(
    state: &AppState,
    agent_id: &str,
    name: &str,
    entity_type: &str,
    confidence: f64,
) -> Result<Uuid> {
    // Check for a near-duplicate entity name (case-insensitive Levenshtein).
    let similar: Option<(Uuid, String)> = sqlx::query_as(
        r#"SELECT id, name FROM entities
           WHERE agent_id = $1
             AND name != $2
             AND levenshtein(LOWER(name), LOWER($2)) <= 2
           ORDER BY levenshtein(LOWER(name), LOWER($2)) ASC
           LIMIT 1"#,
    )
    .bind(agent_id)
    .bind(name)
    .fetch_optional(&state.db)
    .await?;

    let canonical = if let Some((_, existing_name)) = similar {
        tracing::info!(
            agent_id = %agent_id,
            "Entity disambiguation: '{}' merged into '{}'",
            name, existing_name
        );
        existing_name
    } else {
        name.to_string()
    };

    let row: (Uuid,) = sqlx::query_as(
        r#"INSERT INTO entities (agent_id, name, entity_type, confidence)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (agent_id, name) DO UPDATE
               SET entity_type = EXCLUDED.entity_type,
                   confidence  = EXCLUDED.confidence,
                   updated_at  = NOW()
           RETURNING id"#,
    )
    .bind(agent_id)
    .bind(&canonical)
    .bind(entity_type)
    .bind(confidence as f32)
    .fetch_one(&state.db)
    .await?;

    Ok(row.0)
}

/// Link a memory to an entity extracted in the same turn.
pub async fn link_memory_entity(
    state: &AppState,
    memory_id: Uuid,
    entity_id: Uuid,
    agent_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO memory_entity_links (memory_id, entity_id, agent_id)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(memory_id)
    .bind(entity_id)
    .bind(agent_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Return all entity names known for this agent (used for query-entity matching).
pub async fn get_entity_names(state: &AppState, agent_id: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM entities WHERE agent_id = $1 ORDER BY name")
            .bind(agent_id)
            .fetch_all(&state.db)
            .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Walk the knowledge graph one hop: given a set of matched entity names,
/// return all entity names that appear on the other side of any relation.
pub async fn walk_graph_for_entities(
    state: &AppState,
    agent_id: &str,
    entity_names: &[String],
) -> Result<Vec<String>> {
    if entity_names.is_empty() {
        return Ok(Vec::new());
    }
    let lower_names: Vec<String> = entity_names.iter().map(|n| n.to_lowercase()).collect();

    // Two-direction UNION: object side of matching subjects + subject side of matching objects
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT DISTINCT subject AS name FROM memory_graph WHERE agent_id = ",
    );
    qb.push_bind(agent_id);
    qb.push(" AND LOWER(object) IN (");
    let mut sep = qb.separated(", ");
    for n in &lower_names {
        sep.push_bind(n);
    }
    qb.push(") UNION SELECT DISTINCT object AS name FROM memory_graph WHERE agent_id = ");
    qb.push_bind(agent_id);
    qb.push(" AND LOWER(subject) IN (");
    let mut sep2 = qb.separated(", ");
    for n in &lower_names {
        sep2.push_bind(n);
    }
    qb.push(")");

    let rows: Vec<(String,)> = qb.build_query_as().fetch_all(&state.db).await?;
    let matched: std::collections::HashSet<String> = lower_names.into_iter().collect();
    Ok(rows
        .into_iter()
        .map(|(n,)| n)
        .filter(|n| !matched.contains(&n.to_lowercase()))
        .collect())
}

/// Fetch live memories linked to any of the given entity names via the
/// `memory_entity_links` join table, excluding already-retrieved IDs.
pub async fn get_memories_for_entities(
    state: &AppState,
    agent_id: &str,
    entity_names: &[String],
    exclude_ids: &[Uuid],
    limit: i64,
) -> Result<Vec<Memory>> {
    if entity_names.is_empty() {
        return Ok(Vec::new());
    }
    let lower_names: Vec<String> = entity_names.iter().map(|n| n.to_lowercase()).collect();

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        r#"SELECT DISTINCT m.id, m.agent_id, m.session_id, m.content, m.memory_type,
                  m.confidence, m.provenance, m.created_at, m.source_turn,
                  m.importance_score, m.importance_source
           FROM memories m
           JOIN memory_entity_links mel ON m.id = mel.memory_id
           JOIN entities e ON mel.entity_id = e.id
           WHERE m.agent_id = "#,
    );
    qb.push_bind(agent_id);
    qb.push(" AND m.archived_at IS NULL AND LOWER(e.name) IN (");
    let mut sep = qb.separated(", ");
    for n in &lower_names {
        sep.push_bind(n);
    }
    qb.push(")");

    if !exclude_ids.is_empty() {
        qb.push(" AND m.id NOT IN (");
        let mut sep2 = qb.separated(", ");
        for id in exclude_ids {
            sep2.push_bind(*id);
        }
        qb.push(")");
    }

    qb.push(" ORDER BY m.importance_score DESC, m.created_at DESC LIMIT ");
    qb.push_bind(limit);

    qb.build_query_as::<Memory>()
        .fetch_all(&state.db)
        .await
        .map_err(Into::into)
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
/// High-importance memories (score >= 0.9) are protected from archival.
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
          AND importance_score < 0.9
          AND created_at < NOW() - INTERVAL '1 day' * $1
        "#,
    )
    .bind(min_age_days)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Fetch up to `limit` archivable L2 memories for one agent.
/// High-importance memories (score >= 0.9) are protected from archival.
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
          AND importance_score < 0.9
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

// ── Archival batch versioning ─────────────────────────────────────────────────

/// Create an archival batch record before compacting. Returns the new batch id.
pub async fn create_archival_batch(
    state: &AppState,
    agent_id: &str,
    source_count: i32,
    l3_count: i32,
) -> Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO archival_batches (agent_id, source_count, l3_count)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(agent_id)
    .bind(source_count)
    .bind(l3_count)
    .fetch_one(&state.db)
    .await?;
    Ok(row.0)
}

/// Tombstone L2 memories and tag them with the batch that archived them.
pub async fn tombstone_memories_with_batch(
    state: &AppState,
    ids: &[Uuid],
    batch_id: Uuid,
) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "UPDATE memories SET archived_at = NOW(), archival_batch_id = ",
    );
    qb.push_bind(batch_id);
    qb.push(" WHERE id IN (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(")");
    let r = qb.build().execute(&state.db).await?;
    Ok(r.rows_affected())
}

/// List archival batches for an agent, newest first.
pub async fn list_archival_batches(
    state: &AppState,
    agent_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ArchivalBatch>> {
    sqlx::query_as(
        "SELECT id, agent_id, created_at, source_count, l3_count, status
         FROM archival_batches
         WHERE agent_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(agent_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(Into::into)
}

/// Count archival batches for an agent.
pub async fn count_archival_batches(state: &AppState, agent_id: &str) -> Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*)::bigint FROM archival_batches WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_one(&state.db)
            .await?;
    Ok(row.0)
}

pub struct RestoreResult {
    pub l2_restored: u64,
    pub l3_tombstoned: u64,
}

/// Restore an archival batch:
///   - un-tombstones the L2 source memories (clears archived_at)
///   - tombstones the L3 compressed facts that replaced them
///   - marks the batch status = 'restored'
///
/// Returns `None` if the batch does not exist or is already restored.
pub async fn restore_archival_batch(
    state: &AppState,
    batch_id: Uuid,
) -> Result<Option<RestoreResult>> {
    // Guard: only restore a completed batch
    let updated = sqlx::query(
        "UPDATE archival_batches SET status = 'restored'
         WHERE id = $1 AND status = 'completed'",
    )
    .bind(batch_id)
    .execute(&state.db)
    .await?;

    if updated.rows_affected() == 0 {
        return Ok(None);
    }

    let l2 = sqlx::query(
        "UPDATE memories SET archived_at = NULL
         WHERE archival_batch_id = $1 AND tier = 'L2' AND archived_at IS NOT NULL",
    )
    .bind(batch_id)
    .execute(&state.db)
    .await?;

    let l3 = sqlx::query(
        "UPDATE memories SET archived_at = NOW()
         WHERE archival_batch_id = $1 AND tier = 'L3' AND archived_at IS NULL",
    )
    .bind(batch_id)
    .execute(&state.db)
    .await?;

    Ok(Some(RestoreResult {
        l2_restored: l2.rows_affected(),
        l3_tombstoned: l3.rows_affected(),
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────
//
// Integration tests use `#[sqlx::test]` which creates an isolated Postgres
// database per test, runs all migrations, then drops the database when done.
//
// Requirements:
//   - DATABASE_URL must point to a pgvector-enabled Postgres instance where
//     the user has CREATE DATABASE privilege (or set PGCREATEDB=true).
//   - Locally: `docker compose up postgres` then run `cargo test`.
//   - In CI: use the `pgvector/pgvector:pg16` image.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config, metrics::Metrics, providers::Provider, rate_limit::RateLimiter,
    };
    use std::sync::Arc;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Unit vector with a 1.0 at `hot` and 0.0 everywhere else.
    fn unit_vec(hot: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; 1536];
        v[hot] = 1.0;
        v
    }

    /// Two-component unit vector (for decay tests where cosine dist != 0).
    /// Caller is responsible for ensuring a² + b² ≈ 1.
    fn two_hot(dim_a: usize, a: f32, dim_b: usize, b: f32) -> Vec<f32> {
        let mut v = vec![0.0f32; 1536];
        v[dim_a] = a;
        v[dim_b] = b;
        v
    }

    fn build_state(pool: sqlx::PgPool, decay_rate: f64) -> crate::AppState {
        build_state_with_importance(pool, decay_rate, 0.0)
    }

    fn build_state_with_importance(pool: sqlx::PgPool, decay_rate: f64, importance_boost_factor: f64) -> crate::AppState {
        crate::AppState {
            config: Arc::new(Config {
                database_url: String::new(),
                upstream_base_url: "http://localhost".into(),
                port: 8080,
                openai_api_key: None,
                embedding_model: "test".into(),
                embedding_dimension: 1536,
                embedding_base_url: "http://localhost".into(),
                extractor_model: "test".into(),
                extractor_base_url: "http://localhost".into(),
                retrieval_threshold: 0.80,
                memory_decay_rate: decay_rate,
                importance_boost_factor,
                importance_refresh_boost: 0.05,
                management_api_key: None,
                archival_interval_hours: 0,
                archival_min_age_days: 7,
                archival_min_memories: 10,
                upstream_provider: "openai".into(),
                rate_limit_rpm: 0,
                rate_limit_burst: 20,
                graph_retrieval_enabled: false,
            }),
            db: pool,
            http_client: reqwest::Client::new(),
            metrics: Arc::new(Metrics::new().unwrap()),
            provider: Provider::OpenAI,
            rate_limiter: Arc::new(RateLimiter::new(0, 0)),
        }
    }

    // ── Provenance ────────────────────────────────────────────────────────────

    /// Provenance label and confidence value survive a round-trip through the DB.
    #[sqlx::test(migrations = "./migrations")]
    async fn provenance_round_trips(pool: sqlx::PgPool) {
        let state = build_state(pool, 0.0);
        const AGENT: &str = "prov-agent";

        let id = store_memory(
            &state, AGENT, None, "user stated a fact",
            "episodic", 0.85, unit_vec(0), None, "user_stated",
            0.5_f32, "extractor",
        )
        .await
        .unwrap();

        let rows = list_memories_for_agent(&state, AGENT, 10, 0).await.unwrap();
        let m = rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(m.provenance, "user_stated");
        assert!((m.confidence - 0.85).abs() < 1e-5, "confidence mismatch: {}", m.confidence);
    }

    // ── Tombstone ─────────────────────────────────────────────────────────────

    /// Tombstoned memories are excluded from both vector search and list queries.
    #[sqlx::test(migrations = "./migrations")]
    async fn tombstoned_memories_excluded(pool: sqlx::PgPool) {
        let state = build_state(pool, 0.0);
        const AGENT: &str = "tomb-agent";

        let id = store_memory(
            &state, AGENT, None, "soon to be tombstoned",
            "episodic", 0.9, unit_vec(0), None, "user_stated",
            0.5_f32, "extractor",
        )
        .await
        .unwrap();

        // Reachable before tombstone
        let before = search_memories_filtered(&state, AGENT, &unit_vec(0), 5, 0.01, None, None)
            .await
            .unwrap();
        assert_eq!(before.len(), 1, "should find the memory before tombstone");

        tombstone_memories(&state, &[id]).await.unwrap();

        // Excluded after tombstone
        let after = search_memories_filtered(&state, AGENT, &unit_vec(0), 5, 0.01, None, None)
            .await
            .unwrap();
        assert!(after.is_empty(), "tombstoned memory must be excluded from search");

        let listed = list_memories_for_agent(&state, AGENT, 10, 0).await.unwrap();
        assert!(listed.is_empty(), "tombstoned memory must be excluded from list");
    }

    // ── 20-turn retention ─────────────────────────────────────────────────────

    /// A fact stored at turn 3 is still retrievable by vector similarity at turn
    /// 20, even after 17 unrelated facts have been stored in between.
    #[sqlx::test(migrations = "./migrations")]
    async fn retention_across_20_turns(pool: sqlx::PgPool) {
        let state = build_state(pool, 0.0);
        const AGENT: &str = "retention-agent";

        // Turn 3: key fact — embedding lives at dim 0
        store_memory(
            &state, AGENT, Some("s1"), "Alex is building NovaPay",
            "episodic", 0.95, unit_vec(0), Some(3), "user_stated",
            0.5_f32, "extractor",
        )
        .await
        .unwrap();

        // Turn 5: second key fact — embedding at dim 1
        store_memory(
            &state, AGENT, Some("s1"), "NovaPay processes cross-border payments",
            "semantic", 0.95, unit_vec(1), Some(5), "user_stated",
            0.5_f32, "extractor",
        )
        .await
        .unwrap();

        // Turns 1-2, 4, 6-20: noise in unrelated dimensions
        for turn in (1i32..=20).filter(|&t| t != 3 && t != 5) {
            store_memory(
                &state, AGENT, Some("s1"),
                &format!("Noise chatter at turn {}", turn),
                "episodic", 0.80,
                unit_vec(((turn as usize) % 1500) + 20),
                Some(turn),
                "assistant_derived",
                0.5_f32, "extractor",
            )
            .await
            .unwrap();
        }

        // At turn 20: query using the NovaPay vector (dim 0, exact match)
        let results =
            search_memories_filtered(&state, AGENT, &unit_vec(0), 5, 0.10, None, None)
                .await
                .unwrap();

        assert_eq!(results.len(), 1, "should find exactly the NovaPay fact");
        assert!(
            results[0].content.contains("NovaPay"),
            "top result should be the turn-3 NovaPay fact, got: {:?}",
            results[0].content
        );
        assert_eq!(results[0].source_turn, Some(3));
    }

    // ── Decay scoring ─────────────────────────────────────────────────────────
    //
    // Setup:
    //   query  = [1.0, 0.0, ...]           (unit vector at dim 0)
    //   fresh  = [0.8, 0.6,  0.0, ...]     cosine dist ≈ 0.20 from query
    //   stale  = [0.9, 0.0,  0.4359, ...]  cosine dist ≈ 0.10 from query
    //
    // Without decay: stale ranks first (lower cosine dist 0.10 < 0.20).
    // With decay_rate=0.5 and stale aged 30 days:
    //   adjusted_dist(stale) = 0.10 * (1 + 0.5 * 30) = 1.60
    //   adjusted_dist(fresh) = 0.20 * (1 + 0)        = 0.20  ← wins

    #[sqlx::test(migrations = "./migrations")]
    async fn decay_reorders_stale_memories(pool: sqlx::PgPool) {
        const AGENT: &str = "decay-agent";
        // sqrt(1 - 0.81) = sqrt(0.19) ≈ 0.4359
        let fresh_emb = two_hot(0, 0.8, 1, 0.6);
        let stale_emb = two_hot(0, 0.9, 2, 0.4359);
        let query = unit_vec(0);

        let stale_id = {
            let state = build_state(pool.clone(), 0.0);
            store_memory(
                &state, AGENT, None, "fresh fact",
                "episodic", 0.9, fresh_emb, None, "user_stated",
                0.5_f32, "extractor",
            )
            .await
            .unwrap();
            store_memory(
                &state, AGENT, None, "stale fact",
                "episodic", 0.9, stale_emb, None, "user_stated",
                0.5_f32, "extractor",
            )
            .await
            .unwrap()
        };

        // Artificially age the stale memory by 30 days
        sqlx::query(
            "UPDATE memories SET last_accessed_at = NOW() - INTERVAL '30 days' WHERE id = $1",
        )
        .bind(stale_id)
        .execute(&pool)
        .await
        .unwrap();

        // Without decay: stale (cosine dist 0.10) ranks ahead of fresh (0.20)
        let no_decay = build_state(pool.clone(), 0.0);
        let results = search_memories_filtered(&no_decay, AGENT, &query, 2, 1.0, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].content, "stale fact",
            "without decay: lower cosine dist should rank first"
        );

        // With decay (rate 0.5): stale adjusted dist = 1.60, fresh = 0.20
        let with_decay = build_state(pool, 0.5);
        let results = search_memories_filtered(&with_decay, AGENT, &query, 2, 5.0, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].content, "fresh fact",
            "with decay: fresh should rank first despite higher base cosine distance"
        );
    }

    // ── Importance-weighted retrieval ─────────────────────────────────────────
    //
    // Same embedding, different importance scores:
    //   high-importance (score=0.9) vs low-importance (score=0.1)
    //   With boost_factor=2.0:
    //     high: dist_adj = D * (1 + 2*0.1) = D*1.2  ← ranks first
    //     low:  dist_adj = D * (1 + 2*0.9) = D*2.8  ← ranks second
    // Both have the same base cosine distance so the importance factor decides.

    #[sqlx::test(migrations = "./migrations")]
    async fn importance_weighted_retrieval(pool: sqlx::PgPool) {
        const AGENT: &str = "test-importance";
        let emb_hi = two_hot(0, 0.8, 1, 0.6);
        let emb_lo = two_hot(0, 0.8, 1, 0.6); // same embedding — only importance differs
        let query = unit_vec(0);

        {
            let state = build_state(pool.clone(), 0.0);
            store_memory(
                &state, AGENT, None, "high importance fact", "episodic", 0.9,
                emb_hi, None, "user_stated", 0.9_f32, "user_stated",
            ).await.unwrap();
            store_memory(
                &state, AGENT, None, "low importance fact", "episodic", 0.9,
                emb_lo, None, "user_stated", 0.1_f32, "extractor",
            ).await.unwrap();
        }

        // Without importance boost: both have same adjusted dist — both present
        {
            let state = build_state(pool.clone(), 0.0);
            let results = search_memories_filtered(&state, AGENT, &query, 2, 1.0, None, None)
                .await.unwrap();
            assert_eq!(results.len(), 2, "both facts should be returned without boost");
        }

        // With importance boost=2.0: high-importance should rank first
        {
            let state = build_state_with_importance(pool.clone(), 0.0, 2.0);
            let results = search_memories_filtered(&state, AGENT, &query, 2, 5.0, None, None)
                .await.unwrap();
            assert_eq!(results.len(), 2, "both facts should be returned with boost");
            assert_eq!(
                results[0].content, "high importance fact",
                "high-importance should rank first with boost enabled"
            );
        }
    }

    // ── Archival batch round-trip ─────────────────────────────────────────────
    //
    // Simulates the full L2→L3 compaction + restore cycle:
    //   1. Store two L2 memories.
    //   2. Create an archival batch and tombstone the L2 memories.
    //   3. Store two L3 facts tagged with the batch_id.
    //   4. Verify the L2 memories are excluded from live search/list.
    //   5. Call restore_archival_batch.
    //   6. Verify L2 memories are live again and L3 facts are tombstoned.

    #[sqlx::test(migrations = "./migrations")]
    async fn archival_batch_roundtrip(pool: sqlx::PgPool) {
        let state = build_state(pool.clone(), 0.0);
        const AGENT: &str = "batch-roundtrip-agent";

        upsert_agent(&state, AGENT).await.unwrap();

        let id_a = store_memory(
            &state, AGENT, None, "L2 fact A",
            "episodic", 0.9, unit_vec(0), None, "user_stated", 0.5, "extractor",
        ).await.unwrap();
        let id_b = store_memory(
            &state, AGENT, None, "L2 fact B",
            "episodic", 0.9, unit_vec(1), None, "user_stated", 0.5, "extractor",
        ).await.unwrap();

        // Both L2 memories are live
        let before = list_memories_for_agent(&state, AGENT, 10, 0).await.unwrap();
        assert_eq!(before.len(), 2, "both L2 facts should be live before archival");

        let batch_id = create_archival_batch(&state, AGENT, 2, 2).await.unwrap();

        // Tombstone L2 memories with batch
        tombstone_memories_with_batch(&state, &[id_a, id_b], batch_id).await.unwrap();

        // Store L3 compressed facts tagged with the same batch
        let l3_a = store_memory_with_tier(
            &state, AGENT, None, "L3 compressed fact A",
            "semantic", 0.7, unit_vec(10), None, "L3", "inferred", 0.5, "extractor",
            Some(batch_id),
        ).await.unwrap();
        let l3_b = store_memory_with_tier(
            &state, AGENT, None, "L3 compressed fact B",
            "semantic", 0.7, unit_vec(11), None, "L3", "inferred", 0.5, "extractor",
            Some(batch_id),
        ).await.unwrap();

        // L2 memories excluded after tombstone
        let after_archival = list_memories_for_agent(&state, AGENT, 10, 0).await.unwrap();
        assert!(
            after_archival.iter().all(|m| !["L2 fact A", "L2 fact B"].contains(&m.content.as_str())),
            "tombstoned L2 facts must not appear in live list"
        );

        // Restore the batch
        let result = restore_archival_batch(&state, batch_id).await.unwrap();
        let result = result.expect("restore should succeed for a completed batch");
        assert_eq!(result.l2_restored, 2, "should restore 2 L2 memories");
        assert_eq!(result.l3_tombstoned, 2, "should tombstone 2 L3 facts");

        // L2 memories are live again
        let after_restore = list_memories_for_agent(&state, AGENT, 10, 0).await.unwrap();
        let contents: Vec<&str> = after_restore.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.contains(&"L2 fact A"), "L2 fact A must be live after restore");
        assert!(contents.contains(&"L2 fact B"), "L2 fact B must be live after restore");

        // L3 facts are tombstoned (not in live list)
        assert!(
            after_restore.iter().all(|m| m.id != l3_a && m.id != l3_b),
            "L3 facts must be tombstoned after restore"
        );

        // Batch status is 'restored'
        let batch: Vec<ArchivalBatch> = list_archival_batches(&state, AGENT, 10, 0).await.unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].status, "restored");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn restore_already_restored_batch_returns_none(pool: sqlx::PgPool) {
        let state = build_state(pool, 0.0);
        const AGENT: &str = "double-restore-agent";

        upsert_agent(&state, AGENT).await.unwrap();

        let id = store_memory(
            &state, AGENT, None, "L2 memory",
            "episodic", 0.9, unit_vec(0), None, "user_stated", 0.5, "extractor",
        ).await.unwrap();

        let batch_id = create_archival_batch(&state, AGENT, 1, 1).await.unwrap();
        tombstone_memories_with_batch(&state, &[id], batch_id).await.unwrap();
        store_memory_with_tier(
            &state, AGENT, None, "L3 fact", "semantic", 0.7,
            unit_vec(5), None, "L3", "inferred", 0.5, "extractor", Some(batch_id),
        ).await.unwrap();

        // First restore succeeds
        let first = restore_archival_batch(&state, batch_id).await.unwrap();
        assert!(first.is_some(), "first restore must succeed");

        // Second restore returns None (already restored)
        let second = restore_archival_batch(&state, batch_id).await.unwrap();
        assert!(second.is_none(), "restoring an already-restored batch must return None");
    }
}
