//! Own request limits, so the app stays well below what Thingiverse allows:
//! a minimum gap between requests and a cap per sliding window. Time is passed in
//! as seconds, so tests need no real waiting.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Smallest distance between two requests.
    pub min_gap_s: f64,
    /// Length of the sliding window.
    pub window_s: f64,
    /// Requests allowed inside one window.
    pub window_max: usize,
}

impl Limits {
    /// One request per second, 60 per five minutes. Thingiverse allows 300 per five
    /// minutes (per its Getting Started text), so this is a fifth of that.
    pub const PRODUCTION: Limits = Limits { min_gap_s: 1.0, window_s: 300.0, window_max: 60 };

    /// Images come from the image hosts, not from the API (checked 2026-11-04: no key,
    /// no rate-limit headers, publicly cached). Ten per second at most and 300 per five
    /// minutes, which covers about fifteen pages of results.
    pub const IMAGES: Limits = Limits { min_gap_s: 0.1, window_s: 300.0, window_max: 300 };

    /// Tests that talk to a local stub do not want to wait.
    #[cfg(test)]
    pub const NONE: Limits = Limits { min_gap_s: 0.0, window_s: 300.0, window_max: usize::MAX };
}

/// What `Limiter::reserve` decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot {
    /// Go ahead after waiting this many seconds (0 = now).
    After(f64),
    /// The window is full; the oldest request leaves it in this many seconds.
    Full { retry_after_s: f64 },
}

#[derive(Debug)]
pub struct Limiter {
    limits: Limits,
    /// Start times of the reserved requests, oldest first.
    slots: VecDeque<f64>,
}

impl Limiter {
    pub fn new(limits: Limits) -> Self {
        Limiter { limits, slots: VecDeque::new() }
    }

    /// Reserves the next request slot. Callers that arrive together are staggered by
    /// `min_gap_s`, because every reservation moves the next one back.
    pub fn reserve(&mut self, now: f64) -> Slot {
        while self.slots.front().is_some_and(|t| now - t >= self.limits.window_s) {
            self.slots.pop_front();
        }
        if self.slots.len() >= self.limits.window_max {
            let oldest = self.slots.front().copied().unwrap_or(now);
            return Slot::Full { retry_after_s: (oldest + self.limits.window_s - now).max(1.0) };
        }
        let start = match self.slots.back() {
            Some(last) => (last + self.limits.min_gap_s).max(now),
            None => now,
        };
        self.slots.push_back(start);
        Slot::After(start - now)
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_requests_within_a_second_are_staggered() {
        let mut l = Limiter::new(Limits::PRODUCTION);
        assert_eq!(l.reserve(100.0), Slot::After(0.0));
        match l.reserve(100.2) {
            Slot::After(w) => assert!((w - 0.8).abs() < 1e-9, "waits the rest of the gap, got {w}"),
            other => panic!("{other:?}"),
        }
        // A third one right after queues behind the second.
        match l.reserve(100.3) {
            Slot::After(w) => assert!((w - 1.7).abs() < 1e-9, "got {w}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_sixty_first_request_in_five_minutes_is_refused() {
        let mut l = Limiter::new(Limits::PRODUCTION);
        for i in 0..60 {
            assert!(matches!(l.reserve(i as f64 * 2.0), Slot::After(_)), "request {i}");
        }
        match l.reserve(120.0) {
            Slot::Full { retry_after_s } => assert!((retry_after_s - 180.0).abs() < 1e-9, "got {retry_after_s}"),
            other => panic!("{other:?}"),
        }
        // Once the oldest has left the window, a new one fits.
        assert!(matches!(l.reserve(301.0), Slot::After(_)));
    }

    #[test]
    fn a_refused_request_does_not_use_up_a_slot() {
        let mut l = Limiter::new(Limits { min_gap_s: 0.0, window_s: 10.0, window_max: 1 });
        assert_eq!(l.reserve(0.0), Slot::After(0.0));
        assert!(matches!(l.reserve(1.0), Slot::Full { .. }));
        assert!(matches!(l.reserve(2.0), Slot::Full { .. }));
        assert_eq!(l.reserve(10.0), Slot::After(0.0));
    }
}
