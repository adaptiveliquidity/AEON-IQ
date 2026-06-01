use super::policy::PolicyParams;
use super::reward::EpisodeMetrics;

/// Contextual-bandit meta-learner stub.
///
/// Phase 1 implementation: holds a single policy and provides suggest/update
/// hooks.  Phase 2 will replace this with ε-greedy exploration over a
/// perturbation neighbourhood, then PPO once sufficient episodes accumulate.
///
/// Safety invariants:
/// - Policy parameters are always kept within hard bounds set at construction.
/// - Updates are rate-limited externally (caller's responsibility).
/// - The previous policy is accessible for rollback if reward degrades.
pub struct MetaLearner {
    /// Exploration rate (unused in phase 1; reserved for bandit phase).
    pub epsilon: f64,
    pub current: PolicyParams,
    /// Snapshot of the policy before the last update (enables rollback).
    previous: Option<PolicyParams>,
}

impl MetaLearner {
    pub fn new(epsilon: f64, initial: PolicyParams) -> Self {
        Self {
            epsilon,
            current: initial,
            previous: None,
        }
    }

    /// Return the current policy for the next episode.
    pub fn suggest(&self) -> PolicyParams {
        self.current.clone()
    }

    /// Accept an observed reward and update the policy.
    ///
    /// The stub records the previous policy and adopts `new_policy` directly.
    /// A real implementation would compute a gradient estimate or bandit update
    /// based on `metrics` and `reward` before accepting the new params.
    pub fn update(&mut self, _metrics: &EpisodeMetrics, _reward: f64, new_policy: PolicyParams) {
        self.previous = Some(self.current.clone());
        self.current = new_policy;
    }

    /// Revert to the policy that was active before the last update.
    pub fn rollback(&mut self) {
        if let Some(prev) = self.previous.take() {
            self.current = prev;
        }
    }
}
