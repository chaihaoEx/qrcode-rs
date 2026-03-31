//! IP 限流模块
//!
//! 基于滑动窗口的 IP 级别请求频率限制器，主要用于登录接口防暴力破解。
//! 使用内存 HashMap 存储计数，定期清理过期条目防止内存泄漏。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// 每隔多少次调用执行一次过期条目清理
const CLEANUP_INTERVAL: u32 = 100;

/// IP 级别的请求频率限制器。
///
/// 在指定时间窗口内限制同一 key（通常为客户端 IP）的最大请求次数。
/// 线程安全，可在 actix-web 的多线程环境中通过 `web::Data` 共享。
pub struct RateLimiter {
    /// 请求计数表：key → (累计次数, 窗口起始时间)
    attempts: Mutex<HashMap<String, (u32, Instant)>>,
    /// 时间窗口内允许的最大请求次数
    max_attempts: u32,
    /// 时间窗口长度（秒）
    window_secs: u64,
    /// 总调用次数计数器，用于触发定期清理
    call_count: AtomicU32,
}

impl RateLimiter {
    /// 创建新的限流器实例。
    ///
    /// # 参数
    /// - `max_attempts` - 时间窗口内允许的最大请求次数
    /// - `window_secs` - 时间窗口长度（秒）
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window_secs,
            call_count: AtomicU32::new(0),
        }
    }

    /// 检查请求是否在限流范围内，并递增计数器。
    ///
    /// 返回 `true` 表示允许请求，`false` 表示已超出限制。
    /// 如果时间窗口已过期，自动重置计数器。
    /// 每 `CLEANUP_INTERVAL` 次调用自动清理过期条目，防止内存持续增长。
    pub fn check_and_increment(&self, key: &str) -> bool {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();

        // ---- 定期清理过期条目 ----
        let count = self.call_count.fetch_add(1, Ordering::Relaxed);
        if count % CLEANUP_INTERVAL == 0 {
            let window = self.window_secs;
            map.retain(|_, (_, first)| now.duration_since(*first).as_secs() <= window);
        }

        if let Some((attempts, first_attempt)) = map.get_mut(key) {
            if now.duration_since(*first_attempt).as_secs() > self.window_secs {
                // 时间窗口已过期，重置计数
                *attempts = 1;
                *first_attempt = now;
                true
            } else if *attempts >= self.max_attempts {
                // 已达到限制，拒绝请求
                false
            } else {
                // 在限制范围内，递增计数
                *attempts += 1;
                true
            }
        } else {
            // 首次请求，初始化计数
            map.insert(key.to_string(), (1, now));
            true
        }
    }

    /// 重置指定 key 的计数器（登录成功后调用）。
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
