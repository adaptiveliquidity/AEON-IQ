use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::reward::{EpisodeMetrics, RewardModel};

/// A single recorded episode.
#[derive(Debug, Clone)]
pub struct Episode {
    pub id: Uuid,
    pub agent_id: String,
    pub metrics: EpisodeMetrics,
    pub reward: f64,
    pub created_at: DateTime<Utc>,
}

/// Fixed-capacity ring-buffer of recent episodes.
///
/// When `max_size` is exceeded the oldest episode is dropped.  The
/// meta-learner samples from this buffer to update its policy estimate.
pub struct EpisodeBuffer {
    episodes: Vec<Episode>,
    max_size: usize,
    reward_model: RewardModel,
}

impl EpisodeBuffer {
    pub fn new(max_size: usize, reward_model: RewardModel) -> Self {
        Self {
            episodes: Vec::new(),
            max_size,
            reward_model,
        }
    }

    /// Record an episode.  The reward is computed internally.
    pub fn record(&mut self, agent_id: String, metrics: EpisodeMetrics) {
        let reward = self.reward_model.compute_reward(&metrics);
        self.episodes.push(Episode {
            id: Uuid::new_v4(),
            agent_id,
            metrics,
            reward,
            created_at: Utc::now(),
        });
        if self.episodes.len() > self.max_size {
            self.episodes.remove(0);
        }
    }

    pub fn latest(&self) -> Option<&Episode> {
        self.episodes.last()
    }

    pub fn all(&self) -> &[Episode] {
        &self.episodes
    }

    pub fn len(&self) -> usize {
        self.episodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.episodes.is_empty()
    }
}
