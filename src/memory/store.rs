use anyhow::Result;
use pgvector::Vector;
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
) -> Result<Uuid> {
    let vec = Vector::from(embedding);
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO memories
            (agent_id, session_id, content, memory_type, confidence, embedding, source_turn)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
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
    .fetch_one(&state.db)
    .await?;
    Ok(row.0)
}

/// Cosine-similarity search; returns rows ordered by ascending distance.
pub async fn search_memories(
    state: &AppState,
    agent_id: &str,
    embedding: &[f32],
    limit: i64,
) -> Result<Vec<MemorySearchRow>> {
    let vec = Vector::from(embedding.to_vec());
    let rows = sqlx::query_as::<_, MemorySearchRow>(
        r#"
        SELECT
            id, agent_id, session_id, content, memory_type, confidence,
            created_at, source_turn,
            (embedding <=> $2)::double precision AS distance
        FROM memories
        WHERE agent_id = $1
        ORDER BY embedding <=> $2
        LIMIT $3
        "#,
    )
    .bind(agent_id)
    .bind(vec)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

pub async fn list_memories_for_agent(
    state: &AppState,
    agent_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Memory>> {
    let rows = sqlx::query_as::<_, Memory>(
        r#"
        SELECT id, agent_id, session_id, content, memory_type, confidence,
               created_at, source_turn
        FROM memories
        WHERE agent_id = $1
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
    let result = sqlx::query("DELETE FROM memories WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn count_memories(state: &AppState) -> Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*)::bigint FROM memories")
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

// ── Relations ─────────────────────────────────────────────────────────────────

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
