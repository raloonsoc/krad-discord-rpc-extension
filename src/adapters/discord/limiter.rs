//! Rate window, debounce and reconnect backoff, with an injected clock.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Discord accepts 5 `set_activity` calls per 20 seconds.
pub(crate) const RATE_LIMIT: usize = 5;
pub(crate) const RATE_WINDOW: Duration = Duration::from_secs(20);
/// A new view waits this long so a burst of events costs a single call.
pub(crate) const DEBOUNCE: Duration = Duration::from_secs(1);
pub(crate) const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
pub(crate) const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Debounce plus the sliding window of recent calls.
#[derive(Debug, Default)]
pub(crate) struct RateLimiter {
    /// When each recent `set_activity` went out, oldest first.
    sent: VecDeque<Instant>,
    /// When the desired view last changed, for the debounce.
    changed_at: Option<Instant>,
}

impl RateLimiter {
    pub(crate) fn note_change(&mut self, now: Instant) {
        self.changed_at = Some(now);
    }

    /// True when the debounce has passed and the rate window has room.
    pub(crate) fn ready(&mut self, now: Instant) -> bool {
        while self
            .sent
            .front()
            .is_some_and(|sent| now.duration_since(*sent) >= RATE_WINDOW)
        {
            self.sent.pop_front();
        }
        let settled = self
            .changed_at
            .is_none_or(|changed| now.duration_since(changed) >= DEBOUNCE);
        settled && self.sent.len() < RATE_LIMIT
    }

    /// A call counts even if it fails, as Discord already saw the attempt.
    pub(crate) fn record(&mut self, now: Instant) {
        self.sent.push_back(now);
    }
}

/// Exponential retry spacing while Discord is not reachable.
#[derive(Debug)]
pub(crate) struct Backoff {
    current: Duration,
    retry_at: Option<Instant>,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            current: INITIAL_BACKOFF,
            retry_at: None,
        }
    }
}

impl Backoff {
    /// True when a connection attempt may start now.
    pub(crate) fn ready(&self, now: Instant) -> bool {
        !self.retry_at.is_some_and(|at| at > now)
    }

    /// Schedules the next attempt and doubles the wait, up to a minute.
    pub(crate) fn schedule(&mut self, now: Instant) {
        self.retry_at = Some(now + self.current);
        self.current = self.current.saturating_mul(2).min(MAX_BACKOFF);
    }

    /// Back to the initial wait, with no pending retry.
    pub(crate) fn reset(&mut self) {
        self.retry_at = None;
        self.current = INITIAL_BACKOFF;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_wait_the_debounce_and_respect_the_rate_window() {
        let mut limiter = RateLimiter::default();
        let start = Instant::now();
        limiter.note_change(start);
        assert!(!limiter.ready(start));
        assert!(limiter.ready(start + DEBOUNCE));
        for _ in 0..RATE_LIMIT {
            limiter.record(start);
        }
        assert!(!limiter.ready(start + DEBOUNCE));
        assert!(limiter.ready(start + RATE_WINDOW));
    }

    #[test]
    fn backoff_doubles_and_stops_at_a_minute() {
        let mut backoff = Backoff::default();
        let start = Instant::now();
        let mut seen = vec![backoff.current];
        for _ in 0..10 {
            backoff.schedule(start);
            seen.push(backoff.current);
        }
        assert_eq!(
            seen[..5],
            [
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(32),
            ]
        );
        assert_eq!(backoff.current, MAX_BACKOFF);
        assert_eq!(seen.last(), Some(&MAX_BACKOFF));
    }
}
