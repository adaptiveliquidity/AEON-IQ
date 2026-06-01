//! Reflexive Memory Kernel (RMK)
//!
//! RMK adds a meta-learning loop on top of AMP.  Instead of hand-tuned static
//! coefficients, each agent's memory behaviour is governed by a policy vector
//! θ = [pressure_a, pressure_b, kp, ki, graph_bonus_weight, retrieval_threshold]
//! that is updated from episode rewards.
//!
//! Components:
//! * `policy`        — the tunable parameter vector θ
//! * `reward`        — episode metrics → scalar reward
//! * `buffer`        — fixed-capacity in-memory episode ring-buffer
//! * `meta_learner`  — contextual bandit stub (→ PPO in v2)
//! * `adapter`       — applies θ to AMP config structs
//! * `config`        — top-level RMK configuration

pub mod adapter;
pub mod buffer;
pub mod config;
pub mod meta_learner;
pub mod policy;
pub mod reward;
