use super::types::{CoAccessParams, ControllerParams, PressureParams};

/// Top-level configuration for the Adaptive Memory Pressure system.
///
/// Set `AMP_ENABLED=true` (or equivalent) to activate the pressure loop.
/// All sub-configs have safe defaults and can be overridden via env vars
/// or the RMK meta-learner.
#[derive(Debug, Clone)]
pub struct AmpConfig {
    /// Master switch.  When false, pressure computation and soft-eviction
    /// are bypassed entirely.
    pub enabled: bool,
    pub pressure_params: PressureParams,
    pub controller_params: ControllerParams,
    pub co_access_params: CoAccessParams,
    /// Target active-memory count per agent.
    pub target_active_count: u64,
    /// Days of grace before a soft-evicted memory is hard-deleted.
    pub hard_delete_grace_days: i32,
    /// EMA alpha for utility tracking.
    pub feedback_ema_alpha: f64,
}

impl Default for AmpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pressure_params: PressureParams::default(),
            controller_params: ControllerParams::default(),
            co_access_params: CoAccessParams::default(),
            target_active_count: 1000,
            hard_delete_grace_days: 7,
            feedback_ema_alpha: 0.2,
        }
    }
}
