/// EMA-based utility tracker for per-memory retrieval quality.
///
/// The EMA smooths noisy per-retrieval feedback signals into a stable
/// utility estimate.  Higher alpha values make the EMA more responsive
/// to recent feedback; lower values give more weight to historical data.
#[allow(dead_code)]
pub struct UtilityTracker {
    pub alpha: f64,
}

#[allow(dead_code)]
impl UtilityTracker {
    pub fn new(alpha: f64) -> Self {
        Self { alpha }
    }

    /// Pure EMA update: `new_ema = alpha·feedback + (1−alpha)·old_ema`.
    pub fn update_ema(old_ema: f64, feedback: f64, alpha: f64) -> f64 {
        alpha * feedback + (1.0 - alpha) * old_ema
    }
}
