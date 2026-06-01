use serde::{Deserialize, Serialize};

use super::reward::RewardWeights;

/// Top-level RMK configuration.
///
/// Set `RMK_ENABLED=true` to activate the meta-learning loop.
/// When disabled, AMP operates with the static defaults from `AmpConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RmkConfig {
    /// Master switch.
    pub enabled: bool,
    /// ε-greedy exploration rate (phase 2+).
    pub epsilon: f64,
    /// Reward function weights.
    pub reward_weights: RewardWeights,
    /// Maximum episodes retained in the in-memory buffer.
    pub buffer_size: usize,
    /// Minimum episodes required before the first policy update.
    pub min_episodes_before_update: usize,
    /// Minimum wall-clock seconds between policy updates (rate limiting).
    pub update_cooldown_secs: u64,
}

impl Default for RmkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            epsilon: 0.1,
            reward_weights: RewardWeights::default(),
            buffer_size: 100,
            min_episodes_before_update: 20,
            update_cooldown_secs: 3600,
        }
    }
}
