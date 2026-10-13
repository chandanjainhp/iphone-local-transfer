//! Throughput measurement over a rolling time window.
//!
//! Speeds are derived only from real (timestamp, cumulative bytes) samples, so
//! the UI can show "—" instead of a misleading number before enough data
//! exists. An ETA is offered only once an average speed over a meaningful
//! span is available.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Window used for the "instantaneous" figure.
const INSTANT_WINDOW: Duration = Duration::from_millis(1500);
/// Window used for the average figure.
const AVG_WINDOW: Duration = Duration::from_secs(5);
/// Minimum observed span before any number is considered trustworthy.
const MIN_SPAN: Duration = Duration::from_millis(600);
/// Minimum byte delta before any number is considered trustworthy.
const MIN_BYTES: u64 = 64 * 1024;

#[derive(Debug, Default)]
pub struct SpeedMeter {
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedMeter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record cumulative progress at `now`.
    pub fn record(&mut self, now: Instant, cumulative_bytes: u64) {
        // Ignore out-of-order or non-advancing samples.
        if let Some((last_t, last_b)) = self.samples.back() {
            if now < *last_t || cumulative_bytes < *last_b {
                return;
            }
        }
        self.samples.push_back((now, cumulative_bytes));
        // Keep one sample older than the widest window so deltas stay computable.
        let cutoff = now - AVG_WINDOW;
        while self.samples.len() > 2 && self.samples[1].0 <= cutoff {
            self.samples.pop_front();
        }
    }

    fn delta(&self, now: Instant, window: Duration) -> Option<(Duration, u64)> {
        let newest = *self.samples.back()?;
        if now.saturating_duration_since(newest.0) > window / 2 {
            // Data stopped flowing; do not report a stale speed as current.
            return None;
        }
        let cutoff = now - window;
        let boundary = *self
            .samples
            .iter()
            .rev()
            .find(|(t, _)| *t <= cutoff)
            .or(self.samples.front())?;
        let span = newest.0.saturating_duration_since(boundary.0);
        let bytes = newest.1.saturating_sub(boundary.1);
        Some((span, bytes))
    }

    /// Speed over the short window; `None` until there is enough data.
    pub fn instant_bps(&self, now: Instant) -> Option<u64> {
        let (span, bytes) = self.delta(now, INSTANT_WINDOW)?;
        if span >= Duration::from_millis(400) && bytes >= MIN_BYTES / 4 {
            Some((bytes as f64 / span.as_secs_f64()) as u64)
        } else {
            None
        }
    }

    /// Speed over the longer window; `None` until there is enough data or the
    /// transfer has actually stalled.
    pub fn average_bps(&self, now: Instant) -> Option<u64> {
        let (span, bytes) = self.delta(now, AVG_WINDOW)?;
        if span >= MIN_SPAN && bytes >= MIN_BYTES {
            Some((bytes as f64 / span.as_secs_f64()) as u64)
        } else {
            None
        }
    }

    /// Estimated seconds remaining, only with a trustworthy average and a known
    /// remaining byte count.
    pub fn eta_secs(&self, now: Instant, remaining: Option<u64>) -> Option<u64> {
        let remaining = remaining?;
        let avg = self.average_bps(now)?;
        if avg == 0 {
            return None;
        }
        Some(remaining.div_ceil(avg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn no_speed_before_enough_samples() {
        let mut m = SpeedMeter::new();
        let base = t0();
        m.record(base, 0);
        m.record(base + Duration::from_millis(100), 10 * 1024);
        assert_eq!(m.instant_bps(base + Duration::from_millis(100)), None);
        assert_eq!(m.average_bps(base + Duration::from_millis(100)), None);
        assert_eq!(
            m.eta_secs(base + Duration::from_millis(100), Some(1_000_000)),
            None
        );
    }

    #[test]
    fn computes_speed_from_real_samples() {
        let mut m = SpeedMeter::new();
        let base = t0();
        // 5 MiB over 2 seconds => ~2.5 MiB/s
        for i in 0..=10 {
            let secs = i as f64 * 0.2;
            let at = base + Duration::from_secs_f64(secs);
            m.record(at, (secs * 2.5 * 1024.0 * 1024.0) as u64);
        }
        let now = base + Duration::from_secs(2);
        let avg = m.average_bps(now).expect("average available");
        let expected = (2.5 * 1024.0 * 1024.0) as u64;
        let tolerance = expected / 10;
        assert!(
            avg.abs_diff(expected) <= tolerance,
            "avg {avg} not within {tolerance} of {expected}"
        );
        let eta = m
            .eta_secs(now, Some(10 * 1024 * 1024))
            .expect("eta available");
        assert!((3..=6).contains(&eta), "eta {eta} out of range");
    }

    #[test]
    fn stalls_report_no_speed() {
        let mut m = SpeedMeter::new();
        let base = t0();
        for i in 0..=10 {
            let secs = i as f64 * 0.5;
            let at = base + Duration::from_secs_f64(secs);
            m.record(at, (secs * 1024.0 * 1024.0) as u64);
        }
        // No new samples for 10 seconds: stale data must not be reported.
        let now = base + Duration::from_secs(15);
        assert_eq!(m.instant_bps(now), None);
        assert_eq!(m.average_bps(now), None);
        assert_eq!(m.eta_secs(now, Some(1_000_000)), None);
    }

    #[test]
    fn eta_requires_known_remaining_and_speed() {
        let mut m = SpeedMeter::new();
        let base = t0();
        for i in 0..=12 {
            let secs = i as f64 * 0.5;
            let at = base + Duration::from_secs_f64(secs);
            m.record(at, (secs * 1024.0 * 1024.0) as u64);
        }
        let now = base + Duration::from_secs(6);
        assert!(
            m.eta_secs(now, None).is_none(),
            "unknown remaining => no ETA"
        );
    }

    #[test]
    fn ignores_out_of_order_and_regressing_samples() {
        let mut m = SpeedMeter::new();
        let base = t0();
        m.record(base + Duration::from_secs(2), 1_000);
        m.record(base + Duration::from_secs(1), 999_999); // older timestamp
        m.record(base + Duration::from_secs(3), 500); // fewer bytes
        assert_eq!(m.samples.len(), 1);
        assert_eq!(
            m.samples.back(),
            Some(&(base + Duration::from_secs(2), 1_000))
        );
    }
}
