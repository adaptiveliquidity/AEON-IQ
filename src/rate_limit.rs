use dashmap::DashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(60 * 60);
const DEFAULT_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const MAX_SWEEP_SCAN: usize = 1024;

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
    idle_ttl: Duration,
    sweep_interval: Duration,
    last_sweep: Mutex<Instant>,
}

impl RateLimiter {
    pub fn new(rpm: u32, burst: u32) -> Self {
        Self::with_ttl(rpm, burst, DEFAULT_IDLE_TTL)
    }

    pub fn with_ttl(rpm: u32, burst: u32, idle_ttl: Duration) -> Self {
        Self {
            buckets: DashMap::new(),
            refill_rate: rpm as f64 / 60.0,
            burst: burst as f64,
            enabled: rpm > 0,
            idle_ttl,
            sweep_interval: sweep_interval_for(idle_ttl),
            last_sweep: Mutex::new(Instant::now()),
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
        self.evict_stale_buckets(now);

        let mut entry = self
            .buckets
            .entry(agent_id.to_string())
            .or_insert_with(|| Bucket {
                tokens: self.burst,
                last_refill: now,
            });

        // Refill based on elapsed wall time, capped at burst capacity.
        let elapsed = now
            .saturating_duration_since(entry.last_refill)
            .as_secs_f64();
        entry.tokens = (entry.tokens + elapsed * self.refill_rate).min(self.burst);
        entry.last_refill = now;

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn evict_stale_buckets(&self, now: Instant) {
        if self.buckets.is_empty() {
            return;
        }

        let Ok(mut last_sweep) = self.last_sweep.try_lock() else {
            return;
        };
        if now.saturating_duration_since(*last_sweep) < self.sweep_interval {
            return;
        }
        *last_sweep = now;

        // Evicting an idle bucket does not weaken rate limiting: if the same
        // agent returns, its re-created bucket starts full (= burst), which is
        // exactly what a long-idle bucket would have refilled to after capping.
        let stale_keys = self
            .buckets
            .iter()
            .take(MAX_SWEEP_SCAN)
            .filter(|entry| now.saturating_duration_since(entry.last_refill) >= self.idle_ttl)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();

        for key in stale_keys {
            self.buckets.remove_if(&key, |_, bucket| {
                now.saturating_duration_since(bucket.last_refill) >= self.idle_ttl
            });
        }
    }
}

fn sweep_interval_for(idle_ttl: Duration) -> Duration {
    idle_ttl.min(DEFAULT_SWEEP_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age_bucket(rl: &RateLimiter, agent_id: &str, age: Duration) {
        let mut bucket = rl.buckets.get_mut(agent_id).expect("bucket should exist");
        bucket.last_refill = Instant::now()
            .checked_sub(age)
            .expect("test age should fit in Instant range");
    }

    fn make_sweep_due(rl: &RateLimiter) {
        let mut last_sweep = rl
            .last_sweep
            .lock()
            .expect("sweep lock should be available");
        *last_sweep = Instant::now()
            .checked_sub(rl.sweep_interval + Duration::from_millis(1))
            .expect("test sweep interval should fit in Instant range");
    }

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

    #[test]
    fn stale_buckets_are_evicted_after_idle_ttl() {
        let ttl = Duration::from_millis(5);
        let rl = RateLimiter::with_ttl(60, 3, ttl);
        assert!(rl.check_and_consume("stale"));
        assert_eq!(rl.buckets.len(), 1);

        age_bucket(&rl, "stale", ttl + Duration::from_millis(1));
        make_sweep_due(&rl);
        assert!(rl.check_and_consume("trigger"));

        assert!(!rl.buckets.contains_key("stale"));
        assert!(rl.buckets.contains_key("trigger"));
    }

    #[test]
    fn recent_active_buckets_are_not_evicted() {
        let ttl = Duration::from_secs(1);
        let rl = RateLimiter::with_ttl(600, 10, ttl);
        assert!(rl.check_and_consume("old"));
        assert!(rl.check_and_consume("active"));

        age_bucket(&rl, "old", ttl + Duration::from_millis(1));
        age_bucket(&rl, "active", ttl - Duration::from_millis(100));
        make_sweep_due(&rl);
        assert!(rl.check_and_consume("trigger"));

        assert!(!rl.buckets.contains_key("old"));
        assert!(rl.buckets.contains_key("active"));
    }

    #[test]
    fn evicted_agent_keeps_correct_limiting_after_recreation() {
        let ttl = Duration::from_millis(5);
        let rl = RateLimiter::with_ttl(60, 2, ttl);
        assert!(rl.check_and_consume("agent"));

        age_bucket(&rl, "agent", ttl + Duration::from_millis(1));
        make_sweep_due(&rl);
        assert!(rl.check_and_consume("trigger"));
        assert!(!rl.buckets.contains_key("agent"));

        assert!(rl.check_and_consume("agent"));
        assert!(rl.check_and_consume("agent"));
        assert!(
            !rl.check_and_consume("agent"),
            "re-created bucket should still enforce burst capacity"
        );
    }

    #[test]
    fn bucket_count_stays_bounded_after_many_idle_agents() {
        let ttl = Duration::from_millis(5);
        let rl = RateLimiter::with_ttl(600, 1, ttl);
        for agent in 0..128 {
            assert!(rl.check_and_consume(&format!("agent-{agent}")));
            age_bucket(
                &rl,
                &format!("agent-{agent}"),
                ttl + Duration::from_millis(1),
            );
        }
        assert_eq!(rl.buckets.len(), 128);

        make_sweep_due(&rl);
        assert!(rl.check_and_consume("trigger"));

        assert!(
            rl.buckets.len() <= 1,
            "only the trigger bucket should remain after stale cleanup"
        );
    }
}
