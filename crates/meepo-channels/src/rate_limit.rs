//! Per-sender rate limiting for channel adapters

use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

/// A sliding-window rate limiter that tracks per-sender message counts.
///
/// Each sender gets their own window of timestamps. When a new message arrives,
/// expired timestamps are pruned and the count is checked against the limit.
#[derive(Clone)]
pub struct RateLimiter {
    /// Per-sender sliding windows of message timestamps
    windows: Arc<DashMap<String, VecDeque<Instant>>>,
    /// Maximum messages allowed per window
    max_messages: usize,
    /// Duration of the sliding window
    window_duration: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// # Arguments
    /// * `max_messages` - Maximum messages allowed per sender within the window
    /// * `window_duration` - Duration of the sliding window
    pub fn new(max_messages: usize, window_duration: Duration) -> Self {
        Self {
            windows: Arc::new(DashMap::new()),
            max_messages,
            window_duration,
        }
    }

    /// Check if a message from the given sender should be allowed.
    ///
    /// Returns `true` if the message is within rate limits, `false` if it should be dropped.
    /// Automatically records the message timestamp if allowed.
    pub fn check_and_record(&self, sender: &str) -> bool {
        let now = Instant::now();
        let cutoff = now - self.window_duration;

        let mut entry = self.windows.entry(sender.to_string()).or_default();
        let window = entry.value_mut();

        // Prune expired timestamps
        while window.front().is_some_and(|&t| t < cutoff) {
            window.pop_front();
        }

        if window.len() >= self.max_messages {
            warn!(
                "Rate limit exceeded for sender '{}': {} messages in {:?} (limit: {})",
                sender,
                window.len(),
                self.window_duration,
                self.max_messages,
            );
            return false;
        }

        window.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_within_limit() {
        let limiter = RateLimiter::new(5, Duration::from_secs(60));

        for _ in 0..5 {
            assert!(limiter.check_and_record("user1"));
        }
    }

    #[test]
    fn test_blocks_over_limit() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));

        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        assert!(!limiter.check_and_record("user1"));
    }

    #[test]
    fn test_independent_per_sender() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));

        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        assert!(!limiter.check_and_record("user1"));

        // user2 has its own window
        assert!(limiter.check_and_record("user2"));
        assert!(limiter.check_and_record("user2"));
        assert!(!limiter.check_and_record("user2"));
    }

    #[test]
    fn test_window_expiry() {
        let limiter = RateLimiter::new(2, Duration::from_millis(50));

        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        assert!(!limiter.check_and_record("user1"));

        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(60));

        // Should be allowed again
        assert!(limiter.check_and_record("user1"));
    }

    #[test]
    fn test_clone_shares_state() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let limiter2 = limiter.clone();

        assert!(limiter.check_and_record("user1"));
        assert!(limiter2.check_and_record("user1"));
        assert!(!limiter.check_and_record("user1"));
    }

    #[test]
    fn test_limit_of_one() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.check_and_record("user1"));
        assert!(!limiter.check_and_record("user1"));
    }

    #[test]
    fn test_many_senders() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        for i in 0..100 {
            let sender = format!("user_{}", i);
            assert!(limiter.check_and_record(&sender));
        }
    }

    #[test]
    fn test_empty_sender() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check_and_record(""));
        assert!(limiter.check_and_record(""));
        assert!(!limiter.check_and_record(""));
    }

    #[test]
    fn test_partial_window_expiry() {
        let limiter = RateLimiter::new(3, Duration::from_millis(50));

        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));

        // Wait for first two to expire
        std::thread::sleep(Duration::from_millis(60));

        // Third should be allowed (first two expired)
        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        assert!(limiter.check_and_record("user1"));
        // Fourth should be blocked
        assert!(!limiter.check_and_record("user1"));
    }
}
