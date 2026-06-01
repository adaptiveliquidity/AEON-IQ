use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::policy::PolicyParams;
use super::reward::EpisodeMetrics;

/// Fetch the most recent policy for `agent_id`, or `None` if none exists.
pub async fn get_latest_policy(
    pool: &PgPool,
    agent_id: &str,
) -> Result<Option<(Uuid, PolicyParams)>> {
    let row = sqlx::query(
        "SELECT id, pressure_a, pressure_b, kp, ki, graph_bonus_weight, retrieval_threshold
         FROM rmk_policies
         WHERE agent_id = $1
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        (
            r.get("id"),
            PolicyParams {
                pressure_a: r.get("pressure_a"),
                pressure_b: r.get("pressure_b"),
                kp: r.get("kp"),
                ki: r.get("ki"),
                graph_bonus_weight: r.get("graph_bonus_weight"),
                retrieval_threshold: r.get("retrieval_threshold"),
            },
        )
    }))
}

/// Persist a new policy vector and return its UUID.
pub async fn insert_policy(pool: &PgPool, agent_id: &str, policy: &PolicyParams) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO rmk_policies \
         (id, agent_id, pressure_a, pressure_b, kp, ki, graph_bonus_weight, retrieval_threshold) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(agent_id)
    .bind(policy.pressure_a)
    .bind(policy.pressure_b)
    .bind(policy.kp)
    .bind(policy.ki)
    .bind(policy.graph_bonus_weight)
    .bind(policy.retrieval_threshold)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Record one episode's performance metrics and computed reward.
pub async fn insert_episode(
    pool: &PgPool,
    agent_id: &str,
    policy_id: Option<Uuid>,
    metrics: &EpisodeMetrics,
    reward: f64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO rmk_episodes \
         (agent_id, policy_id, task_success, token_savings, retrieval_precision, eviction_cost, reward) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(agent_id)
    .bind(policy_id)
    .bind(metrics.task_success)
    .bind(metrics.token_savings)
    .bind(metrics.retrieval_precision)
    .bind(metrics.eviction_cost)
    .bind(reward)
    .execute(pool)
    .await?;
    Ok(())
}

/// Return the most recent `limit` episode rewards for an agent (newest-first).
pub async fn get_recent_episode_rewards(
    pool: &PgPool,
    agent_id: &str,
    limit: i64,
) -> Result<Vec<f64>> {
    let rows = sqlx::query(
        "SELECT reward FROM rmk_episodes WHERE agent_id = $1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(agent_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.get::<f64, _>("reward")).collect())
}

/// Count all episodes recorded for an agent.
pub async fn count_episodes(pool: &PgPool, agent_id: &str) -> Result<i64> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM rmk_episodes WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// Return every distinct agent_id that has at least one episode recorded.
pub async fn list_all_agent_ids_with_episodes(pool: &PgPool) -> Result<Vec<String>> {
    let rows = sqlx::query("SELECT DISTINCT agent_id FROM rmk_episodes")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.get("agent_id")).collect())
}
