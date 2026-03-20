use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

const CLEANUP_INTERVAL: u32 = 100;

pub struct RateLimiter {
    attempts: Mutex<HashMap<String, (u32, Instant)>>,
    max_attempts: u32,
    window_secs: u64,
    call_count: AtomicU32,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window_secs,
            call_count: AtomicU32::new(0),
        }
    }

    /// Check if request is allowed and increment counter.
    /// Returns true if within limit, false if blocked.
    /// Periodically cleans up expired entries to prevent memory growth.
    pub fn check_and_increment(&self, key: &str) -> bool {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();

        // Periodic cleanup: every CLEANUP_INTERVAL calls, remove expired entries
        let count = self.call_count.fetch_add(1, Ordering::Relaxed);
        if count % CLEANUP_INTERVAL == 0 {
            let window = self.window_secs;
            map.retain(|_, (_, first)| now.duration_since(*first).as_secs() <= window);
        }

        if let Some((attempts, first_attempt)) = map.get_mut(key) {
            if now.duration_since(*first_attempt).as_secs() > self.window_secs {
                *attempts = 1;
                *first_attempt = now;
                true
            } else if *attempts >= self.max_attempts {
                false
            } else {
                *attempts += 1;
                true
            }
        } else {
            map.insert(key.to_string(), (1, now));
            true
        }
    }

    /// Reset the counter for a key (call on successful login).
    pub fn reset(&self, key: &str) {
        let mut map = self.attempts.lock().unwrap();
        map.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_within_limit() {
        let limiter = RateLimiter::new(3, 300);
        assert!(limiter.check_and_increment("ip1"));
        assert!(limiter.check_and_increment("ip1"));
        assert!(limiter.check_and_increment("ip1"));
    }

    #[test]
    fn test_blocks_excess() {
        let limiter = RateLimiter::new(2, 300);
        assert!(limiter.check_and_increment("ip1"));
        assert!(limiter.check_and_increment("ip1"));
        assert!(!limiter.check_and_increment("ip1"));
        assert!(!limiter.check_and_increment("ip1"));
    }

    #[test]
    fn test_independent_keys() {
        let limiter = RateLimiter::new(1, 300);
        assert!(limiter.check_and_increment("ip1"));
        assert!(limiter.check_and_increment("ip2"));
        assert!(!limiter.check_and_increment("ip1"));
    }

    #[test]
    fn test_reset() {
        let limiter = RateLimiter::new(1, 300);
        assert!(limiter.check_and_increment("ip1"));
        assert!(!limiter.check_and_increment("ip1"));
        limiter.reset("ip1");
        assert!(limiter.check_and_increment("ip1"));
    }

    #[test]
    fn test_cleanup_runs_without_panic() {
        let limiter = RateLimiter::new(1000, 300);
        // Trigger cleanup by calling CLEANUP_INTERVAL times
        for i in 0..CLEANUP_INTERVAL + 1 {
            limiter.check_and_increment(&format!("ip{i}"));
        }
    }
}
