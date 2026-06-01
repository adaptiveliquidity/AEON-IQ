use serde::{Deserialize, Serialize};

/// Linear weights for the reward function.
///
/// `R = task·task_success + token_savings·token_savings + precision·retrieval_precision
///      + eviction_cost·eviction_cost`
///
/// `eviction_cost` is typically negative to penalise excessive evictions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardWeights {
    pub task: f64,
    pub token_savings: f64,
    pub precision: f64,
    pub eviction_cost: f64,
}

impl Default for RewardWeights {
    fn default() -> Self {
        Self {
            task: 1.0,
            token_savings: 0.5,
            precision: 1.0,
            eviction_cost: -0.1,
        }
    }
}

/// Performance metrics captured for one conversational episode.
///
/// All values should be normalised to [0, 1] before reward computation
/// so that weights have comparable magnitude.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeMetrics {
    /// Task-completion signal (0 = failure, 1 = success, fractional for partial).
    pub task_success: f64,
    /// Relative token saving vs full-context baseline (0 = none, 1 = max).
    pub token_savings: f64,
    /// Retrieval precision (e.g. hit\@5), normalised to [0, 1].
    pub retrieval_precision: f64,
    /// Aggregate eviction cost (positive; penalised via a negative weight).
    pub eviction_cost: f64,
}

/// Computes a scalar reward from episode metrics via a weighted linear sum.
pub struct RewardModel {
    pub weights: RewardWeights,
}

impl RewardModel {
    pub fn new(weights: RewardWeights) -> Self {
        Self { weights }
    }

    pub fn compute_reward(&self, m: &EpisodeMetrics) -> f64 {
        self.weights.task * m.task_success
            + self.weights.token_savings * m.token_savings
            + self.weights.precision * m.retrieval_precision
            + self.weights.eviction_cost * m.eviction_cost
    }
}
