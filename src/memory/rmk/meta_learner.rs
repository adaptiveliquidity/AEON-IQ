use rand::Rng;

use super::policy::PolicyParams;
use super::reward::EpisodeMetrics;

/// Hard parameter bounds (min, max) for each θ dimension.
/// Values outside these ranges are physically meaningless.
const BOUNDS: [(f64, f64); 6] = [
    (0.001, 0.5), // pressure_a
    (0.05, 2.0),  // pressure_b
    (0.01, 1.0),  // kp
    (0.001, 0.5), // ki
    (0.0, 1.0),   // graph_bonus_weight
    (0.05, 0.99), // retrieval_threshold
];

/// Perturbation magnitude as a fraction of each parameter's full range.
const PERTURBATION_SCALE: f64 = 0.10;

/// Contextual-bandit meta-learner.
///
/// Phase 1: ε-greedy exploration over a perturbation neighbourhood with
/// hill-climbing acceptance (keep the new policy only if reward improved).
/// Phase 2 will replace this with PPO once sufficient episodes accumulate.
///
/// Safety invariants:
/// - Parameters are always clamped to `BOUNDS` after perturbation.
/// - The previous policy is retained for rollback if reward degrades.
/// - Update acceptance is conservative: ties go to the new policy.
pub struct MetaLearner {
    /// Exploration rate: probability of returning a perturbed policy.
    pub epsilon: f64,
    pub current: PolicyParams,
    /// Snapshot before the last update (enables rollback).
    #[allow(dead_code)]
    previous: Option<PolicyParams>,
    /// Reward observed during the episode that produced `current`.
    #[allow(dead_code)]
    last_reward: Option<f64>,
}

impl MetaLearner {
    pub fn new(epsilon: f64, initial: PolicyParams) -> Self {
        Self {
            epsilon,
            current: initial,
            previous: None,
            last_reward: None,
        }
    }

    /// Return the current best-known policy (pure exploitation).
    #[allow(dead_code)]
    pub fn suggest(&self) -> PolicyParams {
        self.current.clone()
    }

    /// ε-greedy: with probability `epsilon` return a uniformly-perturbed
    /// policy bounded by `BOUNDS`; otherwise return the current policy.
    ///
    /// Each perturbed dimension is offset by ±`PERTURBATION_SCALE × range`
    /// where range = hi − lo for that parameter.
    pub fn suggest_explore(&self) -> PolicyParams {
        let mut rng = rand::thread_rng();
        if rng.gen::<f64>() >= self.epsilon {
            return self.current.clone();
        }

        let vals = [
            self.current.pressure_a,
            self.current.pressure_b,
            self.current.kp,
            self.current.ki,
            self.current.graph_bonus_weight,
            self.current.retrieval_threshold,
        ];

        let perturbed: Vec<f64> = vals
            .iter()
            .zip(BOUNDS.iter())
            .map(|(&v, &(lo, hi))| {
                let noise = rng.gen_range(-PERTURBATION_SCALE..PERTURBATION_SCALE) * (hi - lo);
                (v + noise).clamp(lo, hi)
            })
            .collect();

        PolicyParams {
            pressure_a: perturbed[0],
            pressure_b: perturbed[1],
            kp: perturbed[2],
            ki: perturbed[3],
            graph_bonus_weight: perturbed[4],
            retrieval_threshold: perturbed[5],
        }
    }

    /// Accept an observed reward and conditionally adopt `new_policy`.
    ///
    /// Hill-climbing rule: replace `current` with `new_policy` only when
    /// `reward >= last_reward` (non-degrading).  This prevents a bad
    /// perturbation from being locked in even after a negative signal.
    #[allow(dead_code)]
    pub fn update(&mut self, _metrics: &EpisodeMetrics, reward: f64, new_policy: PolicyParams) {
        self.previous = Some(self.current.clone());
        if self.last_reward.is_none_or(|prev| reward >= prev) {
            self.current = new_policy;
        }
        self.last_reward = Some(reward);
    }

    /// Revert to the policy that was active before the last `update` call.
    #[allow(dead_code)]
    pub fn rollback(&mut self) {
        if let Some(prev) = self.previous.take() {
            self.current = prev;
        }
    }
}
