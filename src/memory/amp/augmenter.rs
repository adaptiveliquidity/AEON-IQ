use uuid::Uuid;

use super::co_access::CoAccessGraph;
use super::pressure::PressureManager;

/// Combines pressure filtering and co-access bonuses into adjusted retrieval
/// scores.
///
/// The full implementation should:
/// 1. Fetch pressure state for each candidate from the DB.
/// 2. Filter out soft-evicted memories entirely.
/// 3. Add `graph_bonus_weight × neighbor_weight_sum` to each remaining score.
/// 4. Optionally multiply by `(1 − pressure)` as a soft penalty.
///
/// The placeholder below returns the base similarities unchanged; swap it
/// for the full logic once the AMP background job is wired up.
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

    /// Return adjusted `(memory_id, score)` pairs.
    /// `_candidate_ids` is reserved for future pressure-lookup filtering.
    pub async fn augment_scores(
        &self,
        _candidate_ids: Vec<Uuid>,
        similarities: &[(Uuid, f64)],
    ) -> Vec<(Uuid, f64)> {
        similarities.to_vec()
    }
}
