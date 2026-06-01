use uuid::Uuid;

use super::co_access::CoAccessGraph;
use super::pressure::PressureManager;

/// Combines pressure filtering and co-access bonuses into adjusted retrieval
/// scores.
///
/// Phase 1 co-access bonus: for each candidate memory, fetch its aggregate
/// neighbour weight from `co_access_edges` and subtract a scaled bonus from
/// the cosine distance.  Memories that frequently appear alongside other
/// retrieved memories are promoted (lower effective distance = higher rank).
///
/// Phase 2 will add pressure-based filtering: soft-evicted memories are
/// removed from the candidate set entirely before scoring.
pub struct RetrievalAugmenter {
    pub pressure_manager: PressureManager,
    pub co_access_graph: CoAccessGraph,
    pub graph_bonus_weight: f64,
}

impl RetrievalAugmenter {
    pub fn new(
        pressure_manager: PressureManager,
        co_access_graph: CoAccessGraph,
        graph_bonus_weight: f64,
    ) -> Self {
        Self {
            pressure_manager,
            co_access_graph,
            graph_bonus_weight,
        }
    }

    /// Return adjusted `(memory_id, distance)` pairs.
    ///
    /// `similarities` carries the vector-search cosine distances
    /// (lower = more similar).  The co-access bonus is subtracted so that
    /// memories with strong graph connections are ranked higher.
    ///
    /// `_candidate_ids` is reserved for the pressure-filtering pass (Phase 2).
    pub async fn augment_scores(
        &self,
        _candidate_ids: Vec<Uuid>,
        similarities: &[(Uuid, f64)],
    ) -> Vec<(Uuid, f64)> {
        if self.graph_bonus_weight <= 0.0 {
            return similarities.to_vec();
        }

        let mut result = Vec::with_capacity(similarities.len());
        for &(id, dist) in similarities {
            let bonus = self.co_access_graph.get_neighbor_weight_sum(id).await
                * self.graph_bonus_weight;
            // Clamp to 0 so we never invert the distance ordering.
            result.push((id, (dist - bonus).max(0.0)));
        }
        result
    }
}
