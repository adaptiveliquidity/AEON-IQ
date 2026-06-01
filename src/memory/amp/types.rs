use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Pressure state for one memory, fetched from the `memories` table.
///
/// `last_accessed` is renamed from the existing `last_accessed_at` column
/// (migration 0004).  `access_count` maps to the `INTEGER` column added in
/// migration 0002; `utility_ema` and `pressure` were added in 0014.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemoryPressureState {
    pub memory_id: Uuid,
    #[sqlx(rename = "last_accessed_at")]
    pub last_accessed: Option<DateTime<Utc>>,
    pub access_count: i32,
    pub utility_ema: f64,
    pub pressure: f64,
    pub soft_evicted: bool,
    pub soft_evicted_at: Option<DateTime<Utc>>,
}

/// PI-controller tuning parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerParams {
    pub kp: f64,
    pub ki: f64,
    /// Fractional error below which updates are skipped (prevents thrashing).
    pub deadband: f64,
    /// Gap between eviction and restoration thresholds (hysteresis).
    pub delta_hysteresis: f64,
    pub max_change_per_cycle: f64,
    /// Minimum memory age before soft-eviction is permitted (seconds).
    pub min_age_seconds: i64,
}

impl Default for ControllerParams {
    fn default() -> Self {
        Self {
            kp: 0.15,
            ki: 0.02,
            deadband: 0.02,
            delta_hysteresis: 0.05,
            max_change_per_cycle: 0.1,
            min_age_seconds: 24 * 60 * 60,
        }
    }
}

/// Pressure formula coefficients: `pressure = a·days_stale + b·(1 − utility_ema)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PressureParams {
    pub a: f64,
    pub b: f64,
}

impl Default for PressureParams {
    fn default() -> Self {
        Self { a: 0.02, b: 0.4 }
    }
}

/// Co-access graph tuning parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoAccessParams {
    pub decay_per_day: f64,
    pub max_edge_weight: f64,
    pub min_edge_weight: f64,
    pub graph_bonus_weight: f64,
    pub max_bonus: f64,
    pub max_neighbors: usize,
}

impl Default for CoAccessParams {
    fn default() -> Self {
        Self {
            decay_per_day: 0.05,
            max_edge_weight: 5.0,
            min_edge_weight: 0.01,
            graph_bonus_weight: 0.15,
            max_bonus: 1.0,
            max_neighbors: 20,
        }
    }
}
