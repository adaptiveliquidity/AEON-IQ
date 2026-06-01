use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use super::types::CoAccessParams;

/// Co-access graph backed by the `co_access_edges` table.
///
/// Edges are undirected and accumulate weight when two memories appear
/// together in the same retrieved context.  A decay pass periodically
/// reduces all weights; edges that drop below `min_edge_weight` are pruned.
pub struct CoAccessGraph {
    params: CoAccessParams,
    pool: PgPool,
}

impl CoAccessGraph {
    pub fn new(pool: PgPool, params: CoAccessParams) -> Self {
        Self { pool, params }
    }

    /// Record a co-occurrence between two memories.  The pair is normalised to
    /// `(min, max)` order to satisfy the `order_agnostic` DB constraint.
    pub async fn record_co_access(&self, memory_a: Uuid, memory_b: Uuid) -> anyhow::Result<()> {
        let (a, b) = if memory_a < memory_b {
            (memory_a, memory_b)
        } else {
            (memory_b, memory_a)
        };
        sqlx::query(
            "INSERT INTO co_access_edges (memory_a, memory_b, weight, last_co_occurred)
             VALUES ($1, $2, 1.0, NOW())
             ON CONFLICT (memory_a, memory_b)
             DO UPDATE SET
                 weight           = LEAST(co_access_edges.weight + 1.0, $3),
                 last_co_occurred = NOW()",
        )
        .bind(a)
        .bind(b)
        .bind(self.params.max_edge_weight)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Decay all edge weights by `decay_per_day`, then prune sub-threshold edges.
    pub async fn decay_all(&self) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE co_access_edges
             SET weight = weight * (1.0 - $1)
             WHERE weight > $2",
        )
        .bind(self.params.decay_per_day)
        .bind(self.params.min_edge_weight)
        .execute(&self.pool)
        .await?;
        sqlx::query("DELETE FROM co_access_edges WHERE weight <= $1")
            .bind(self.params.min_edge_weight)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Return the capped sum of co-access weights for the top-N neighbours of
    /// `memory_id`.  Used to compute the co-access bonus during retrieval.
    #[allow(dead_code)]
    pub async fn get_neighbor_weight_sum(&self, memory_id: Uuid) -> f64 {
        let sum: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(weight), 0.0)
             FROM (
                 SELECT weight
                 FROM co_access_edges
                 WHERE memory_a = $1 OR memory_b = $1
                 ORDER BY last_co_occurred DESC
                 LIMIT $2
             ) AS neighbors",
        )
        .bind(memory_id)
        .bind(self.params.max_neighbors as i32)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0.0);

        // Cap the bonus contribution
        sum.min(self.params.max_bonus / self.params.graph_bonus_weight.max(f64::EPSILON))
    }

    /// Batch version: returns the capped neighbour-weight sum for every ID
    /// in `memory_ids` in a single query instead of N individual lookups.
    ///
    /// Edges are undirected so each side is counted separately via UNION ALL.
    /// IDs not present in `co_access_edges` are omitted from the result (the
    /// caller should treat missing entries as 0.0).
    pub async fn get_neighbor_weight_sums(&self, memory_ids: &[Uuid]) -> HashMap<Uuid, f64> {
        if memory_ids.is_empty() {
            return HashMap::new();
        }

        let cap = self.params.max_bonus / self.params.graph_bonus_weight.max(f64::EPSILON);

        let rows = sqlx::query(
            "SELECT target_id, SUM(weight) AS total_weight
             FROM (
                 SELECT memory_a AS target_id, weight
                 FROM co_access_edges
                 WHERE memory_a = ANY($1)
                 UNION ALL
                 SELECT memory_b AS target_id, weight
                 FROM co_access_edges
                 WHERE memory_b = ANY($1)
             ) t
             GROUP BY target_id",
        )
        .bind(memory_ids)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();

        use sqlx::Row;
        rows.into_iter()
            .map(|r| {
                let id: Uuid = r.get("target_id");
                let w: f64 = r.get::<f64, _>("total_weight").min(cap);
                (id, w)
            })
            .collect()
    }
}
