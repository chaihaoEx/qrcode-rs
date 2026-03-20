use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

pub struct RateLimiter {
    attempts: Mutex<HashMap<String, (u32, Instant)>>,
    max_attempts: u32,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window_secs,
        }
    }

    /// Check if request is allowed and increment counter.
    /// Returns true if within limit, false if blocked.
    pub fn check_and_increment(&self, key: &str) -> bool {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();

        if let Some((count, first_attempt)) = map.get_mut(key) {
            if now.duration_since(*first_attempt).as_secs() > self.window_secs {
                *count = 1;
                *first_attempt = now;
                true
            } else if *count >= self.max_attempts {
                false
            } else {
                *count += 1;
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
}
