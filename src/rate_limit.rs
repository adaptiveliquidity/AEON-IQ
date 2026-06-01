use dashmap::DashMap;
use std::time::Instant;

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Per-agent token bucket rate limiter.
///
/// Configured via `RATE_LIMIT_RPM` (requests per minute; 0 = disabled)
/// and `RATE_LIMIT_BURST` (max burst; defaults to twice the per-minute rate
/// but at least 1).  Each agent gets its own independent bucket.
pub struct RateLimiter {
    buckets: DashMap<String, Bucket>,
    refill_rate: f64, // tokens per second
    burst: f64,       // bucket capacity
    enabled: bool,
}

impl RateLimiter {
    pub fn new(rpm: u32, burst: u32) -> Self {
        Self {
            buckets: DashMap::new(),
            refill_rate: rpm as f64 / 60.0,
            burst: burst as f64,
            enabled: rpm > 0,
        }
    }

    /// Returns `true` if the agent is within its quota, `false` if rate-limited.
    ///
    /// Thread-safe: each agent's bucket is accessed under a per-key shard lock
    /// from DashMap, so concurrent requests for *different* agents never contend.
    pub fn check_and_consume(&self, agent_id: &str) -> bool {
        if !self.enabled {
            return true;
        }

        let now = Instant::now();
        let mut entry = self
            .buckets
            .entry(agent_id.to_string())
            .or_insert_with(|| Bucket {
                tokens: self.burst,
                last_refill: now,
            });

        // Refill based on elapsed wall time, capped at burst capacity.
        let elapsed = now.duration_since(entry.last_refill).as_secs_f64();
        entry.tokens = (entry.tokens + elapsed * self.refill_rate).min(self.burst);
        entry.last_refill = now;

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_limiter_always_allows() {
        let rl = RateLimiter::new(0, 20);
        for _ in 0..200 {
            assert!(rl.check_and_consume("agent-x"));
        }
    }

    #[test]
    fn burst_capacity_is_enforced() {
        let rl = RateLimiter::new(60, 3);
        assert!(rl.check_and_consume("a"));
        assert!(rl.check_and_consume("a"));
        assert!(rl.check_and_consume("a"));
        assert!(!rl.check_and_consume("a"), "4th request should be rejected");
    }

    #[test]
    fn agents_have_independent_buckets() {
        let rl = RateLimiter::new(60, 1);
        assert!(rl.check_and_consume("alice"));
        assert!(!rl.check_and_consume("alice"), "alice exhausted");
        assert!(
            rl.check_and_consume("bob"),
            "bob unaffected by alice's bucket"
        );
    }

    #[test]
    fn tokens_refill_after_elapsed_time() {
        // 600 RPM = 10 req/sec = 1 token per 100 ms; burst = 1
        let rl = RateLimiter::new(600, 1);
        assert!(rl.check_and_consume("r"));
        assert!(!rl.check_and_consume("r"));
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(
            rl.check_and_consume("r"),
            "should have refilled after 150 ms"
        );
    }
}
