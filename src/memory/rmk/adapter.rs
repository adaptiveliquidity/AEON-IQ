use super::policy::PolicyParams;
use crate::memory::amp::types::{CoAccessParams, ControllerParams, PressureParams};

/// Translates a `PolicyParams` vector into AMP configuration structs.
///
/// All six dimensions of θ are applied in-place so callers can hold
/// per-request copies of the AMP structs without touching global state.
pub struct RmkAdapter;

impl RmkAdapter {
    pub fn apply(
        policy: &PolicyParams,
        pressure_params: &mut PressureParams,
        controller_params: &mut ControllerParams,
        co_access_params: &mut CoAccessParams,
        retrieval_threshold: &mut f64,
    ) {
        pressure_params.a = policy.pressure_a;
        pressure_params.b = policy.pressure_b;
        controller_params.kp = policy.kp;
        controller_params.ki = policy.ki;
        co_access_params.graph_bonus_weight = policy.graph_bonus_weight;
        *retrieval_threshold = policy.retrieval_threshold;
    }
}
