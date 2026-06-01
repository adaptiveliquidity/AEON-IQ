//! Adaptive Memory Pressure (AMP)
//!
//! AMP provides a closed-loop system that keeps active memory count near a
//! configurable target by computing per-memory *pressure* and soft-evicting
//! memories whose pressure exceeds a PI-controller-driven threshold.
//!
//! Components:
//! * `types`       — shared parameter structs and DB row types
//! * `pressure`    — stateless pressure computation + eviction decisions
//! * `pi_controller` — PI controller for eviction aggressiveness
//! * `co_access`   — co-access graph: pheromone trails between memories
//! * `utility`     — EMA tracker for per-memory retrieval utility
//! * `augmenter`   — combines pressure + co-access into retrieval scores
//! * `config`      — top-level AMP configuration block

pub mod augmenter;
pub mod co_access;
pub mod config;
pub mod pi_controller;
pub mod pressure;
pub mod types;
pub mod utility;
