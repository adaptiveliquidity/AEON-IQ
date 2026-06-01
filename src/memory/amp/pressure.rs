use chrono::{DateTime, Utc};

use super::types::PressureParams;

/// Stateless pressure computation and eviction-decision helpers.
///
/// `pressure = a·days_stale + b·(1 − utility_ema)`, clamped to [0, 1].
pub struct PressureManager {
    params: PressureParams,
}

impl PressureManager {
    pub fn new(params: PressureParams) -> Self {
        Self { params }
    }

    /// Compute unit-interval pressure.  `last_accessed` defaults to `created_at`
    /// when `None` (memory has never been retrieved).
    pub fn compute_pressure(
        &self,
        last_accessed: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        utility_ema: f64,
        now: DateTime<Utc>,
    ) -> f64 {
        let reference = last_accessed.unwrap_or(created_at);
        let days_stale = (now - reference).num_milliseconds() as f64 / (1000.0 * 86_400.0);
        (self.params.a * days_stale + self.params.b * (1.0 - utility_ema)).min(1.0)
    }

    /// Returns `true` when pressure exceeds the PI-controller threshold and the
    /// memory is old enough to be eligible for soft-eviction.
    pub fn should_soft_evict(
        pressure: f64,
        threshold_high: f64,
        created_at: DateTime<Utc>,
        min_age_seconds: i64,
        now: DateTime<Utc>,
    ) -> bool {
        if (now - created_at).num_seconds() < min_age_seconds {
            return false;
        }
        pressure > threshold_high
    }

    /// Returns `true` when pressure has fallen back below the low (restoration)
    /// threshold — used to un-evict memories.
    pub fn should_restore(pressure: f64, threshold_low: f64) -> bool {
        pressure < threshold_low
    }
}
