//! Runtime statistics for the RT scheduler.
//!
//! The scheduler updates a [`RtStats`] struct after each tick with
//! latency, jitter, and missed-tick accounting. The stats are
//! designed to be cheap to update on the hot path (atomic increments
//! + a bucketed histogram with a few integer ops) and cheap to
//! snapshot for `/api/stats` or logs.
//!
//! All fields are `AtomicU64` / `AtomicI64` so readers can observe
//! a consistent snapshot without taking a lock. Cross-field
//! consistency is *not* guaranteed (a concurrent snapshot may see
//! `tick_count = N` and `missed_ticks = M+1`), which is fine for
//! diagnostics and aggregated metrics.

use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};

// -------------------------------------------------------------------------
//   Jitter histogram
// -------------------------------------------------------------------------

/// Bucket boundaries for the jitter histogram, in microseconds.
///
/// Buckets are (inclusive-lower, exclusive-upper) half-open
/// intervals. Negative jitter (scheduler woke up early) is lumped
/// into the first bucket; anything above the last bucket is lumped
/// into the overflow bucket.
///
/// The boundaries are picked to span "healthy RT" (0–50 μs), "warn"
/// (50–200 μs), "bad" (200–1000 μs), and "catastrophic" (>1 ms),
/// which covers the 10 kHz / 20 kHz target on our hardware.
pub const JITTER_BUCKET_BOUNDS_US: &[i32] = &[
    0,   // bucket 0: jitter < 0        (early)
    10,  // bucket 1: 0..10 μs          (ideal)
    25,  // bucket 2: 10..25 μs
    50,  // bucket 3: 25..50 μs
    100, // bucket 4: 50..100 μs
    200, // bucket 5: 100..200 μs
    500, // bucket 6: 200..500 μs
    1000, // bucket 7: 500..1000 μs
         // overflow at index 8 for anything >= 1000 μs
];

/// Number of histogram buckets (= boundaries + 1 overflow).
pub const JITTER_NUM_BUCKETS: usize = 9;

// -------------------------------------------------------------------------
//   RtStats
// -------------------------------------------------------------------------

/// Live statistics for the RT scheduler loop.
///
/// Updated once per tick; snapshotted on demand by consumers (web
/// UI, `/api/stats`, logs). Use [`RtStats::snapshot`] to get a
/// plain `RtStatsSnapshot` that is `Clone + Send + Serialize`.
#[derive(Debug, Default)]
pub struct RtStats {
    /// Total number of ticks the scheduler has attempted (including
    /// missed ones).
    pub tick_count: AtomicU64,

    /// Number of ticks that completed successfully (sent + received
    /// a valid response).
    pub tick_ok: AtomicU64,

    /// Number of ticks the scheduler had to skip because the
    /// previous iteration ran past its deadline, OR because the
    /// transport reported a timeout / error.
    pub missed_ticks: AtomicU64,

    /// Total number of transport errors (CRC mismatch, timeout,
    /// decode failure, ...). A subset of `missed_ticks`.
    pub transport_errors: AtomicU64,

    /// Sum of all observed round-trip latencies in microseconds.
    /// Divide by `tick_ok` to get the mean latency.
    pub latency_sum_us: AtomicU64,

    /// Maximum round-trip latency seen so far, in microseconds.
    pub latency_max_us: AtomicU32,

    /// Sum of all observed `|jitter|` values in microseconds, for
    /// mean-absolute-jitter calculation.
    pub jitter_abs_sum_us: AtomicU64,

    /// Minimum and maximum signed jitter observed so far, in
    /// microseconds.
    pub jitter_min_us: AtomicI64,
    pub jitter_max_us: AtomicI64,

    /// Bucketed jitter histogram. Indices correspond to
    /// [`JITTER_BUCKET_BOUNDS_US`]; the final bucket is overflow.
    pub jitter_histogram: [AtomicU64; JITTER_NUM_BUCKETS],
}

impl RtStats {
    /// Create a fresh stats struct with all counters at zero.
    pub fn new() -> Self {
        let mut hist: [AtomicU64; JITTER_NUM_BUCKETS] = Default::default();
        // Default for [AtomicU64; N] is zeroed — explicit for clarity
        for b in hist.iter_mut() {
            *b = AtomicU64::new(0);
        }
        Self {
            tick_count: AtomicU64::new(0),
            tick_ok: AtomicU64::new(0),
            missed_ticks: AtomicU64::new(0),
            transport_errors: AtomicU64::new(0),
            latency_sum_us: AtomicU64::new(0),
            latency_max_us: AtomicU32::new(0),
            jitter_abs_sum_us: AtomicU64::new(0),
            jitter_min_us: AtomicI64::new(i64::MAX),
            jitter_max_us: AtomicI64::new(i64::MIN),
            jitter_histogram: hist,
        }
    }

    /// Record the outcome of a successful tick.
    ///
    /// Called from the RT loop after a valid exchange; bumps
    /// `tick_count`, `tick_ok`, and updates latency/jitter metrics.
    pub fn record_ok(&self, latency_us: u32, jitter_us: i32) {
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.tick_ok.fetch_add(1, Ordering::Relaxed);
        self.latency_sum_us.fetch_add(latency_us as u64, Ordering::Relaxed);

        // latency_max with atomic max
        let mut cur = self.latency_max_us.load(Ordering::Relaxed);
        while latency_us > cur {
            match self.latency_max_us.compare_exchange_weak(
                cur,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(v) => cur = v,
            }
        }

        // jitter absolute sum + signed min/max
        let abs = jitter_us.unsigned_abs() as u64;
        self.jitter_abs_sum_us.fetch_add(abs, Ordering::Relaxed);

        let mut cur_min = self.jitter_min_us.load(Ordering::Relaxed);
        while (jitter_us as i64) < cur_min {
            match self.jitter_min_us.compare_exchange_weak(
                cur_min,
                jitter_us as i64,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(v) => cur_min = v,
            }
        }

        let mut cur_max = self.jitter_max_us.load(Ordering::Relaxed);
        while (jitter_us as i64) > cur_max {
            match self.jitter_max_us.compare_exchange_weak(
                cur_max,
                jitter_us as i64,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(v) => cur_max = v,
            }
        }

        // histogram update
        let idx = bucket_index(jitter_us);
        self.jitter_histogram[idx].fetch_add(1, Ordering::Relaxed);
    }

    /// Record a missed tick (scheduler slipped, transport timeout,
    /// transport error, ...). `count` may be > 1 if the scheduler
    /// had to jump multiple tick periods at once.
    pub fn record_missed(&self, count: u32) {
        let n = count as u64;
        self.tick_count.fetch_add(n, Ordering::Relaxed);
        self.missed_ticks.fetch_add(n, Ordering::Relaxed);
    }

    /// Record a transport-level error (CRC, timeout, decode, ...).
    /// The scheduler typically also calls [`record_missed(1)`] in
    /// the same iteration.
    pub fn record_transport_error(&self) {
        self.transport_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Take a plain, cloneable snapshot of the current counters.
    pub fn snapshot(&self) -> RtStatsSnapshot {
        let tick_count = self.tick_count.load(Ordering::Relaxed);
        let tick_ok = self.tick_ok.load(Ordering::Relaxed);
        let missed_ticks = self.missed_ticks.load(Ordering::Relaxed);
        let transport_errors = self.transport_errors.load(Ordering::Relaxed);
        let latency_sum_us = self.latency_sum_us.load(Ordering::Relaxed);
        let latency_max_us = self.latency_max_us.load(Ordering::Relaxed);
        let jitter_abs_sum_us = self.jitter_abs_sum_us.load(Ordering::Relaxed);
        let jitter_min_us = self.jitter_min_us.load(Ordering::Relaxed);
        let jitter_max_us = self.jitter_max_us.load(Ordering::Relaxed);

        let mut histogram = [0u64; JITTER_NUM_BUCKETS];
        for (i, b) in self.jitter_histogram.iter().enumerate() {
            histogram[i] = b.load(Ordering::Relaxed);
        }

        let mean_latency_us = if tick_ok > 0 {
            latency_sum_us as f64 / tick_ok as f64
        } else {
            0.0
        };
        let mean_abs_jitter_us = if tick_ok > 0 {
            jitter_abs_sum_us as f64 / tick_ok as f64
        } else {
            0.0
        };

        RtStatsSnapshot {
            tick_count,
            tick_ok,
            missed_ticks,
            transport_errors,
            latency_sum_us,
            latency_max_us,
            mean_latency_us,
            jitter_min_us: if jitter_min_us == i64::MAX {
                0
            } else {
                jitter_min_us
            },
            jitter_max_us: if jitter_max_us == i64::MIN {
                0
            } else {
                jitter_max_us
            },
            jitter_abs_sum_us,
            mean_abs_jitter_us,
            jitter_histogram: histogram,
        }
    }

    /// Reset all counters to zero. Useful at the beginning of a
    /// benchmark run.
    pub fn reset(&self) {
        self.tick_count.store(0, Ordering::Relaxed);
        self.tick_ok.store(0, Ordering::Relaxed);
        self.missed_ticks.store(0, Ordering::Relaxed);
        self.transport_errors.store(0, Ordering::Relaxed);
        self.latency_sum_us.store(0, Ordering::Relaxed);
        self.latency_max_us.store(0, Ordering::Relaxed);
        self.jitter_abs_sum_us.store(0, Ordering::Relaxed);
        self.jitter_min_us.store(i64::MAX, Ordering::Relaxed);
        self.jitter_max_us.store(i64::MIN, Ordering::Relaxed);
        for b in self.jitter_histogram.iter() {
            b.store(0, Ordering::Relaxed);
        }
    }
}

#[inline]
fn bucket_index(jitter_us: i32) -> usize {
    // First bucket catches negative jitter (early wakeup).
    if jitter_us < 0 {
        return 0;
    }
    // Buckets 1..N-1 use the upper bounds from
    // JITTER_BUCKET_BOUNDS_US[1..].
    for (i, &bound) in JITTER_BUCKET_BOUNDS_US.iter().enumerate().skip(1) {
        if jitter_us < bound {
            return i;
        }
    }
    // Overflow: >= last bound
    JITTER_NUM_BUCKETS - 1
}

// -------------------------------------------------------------------------
//   Plain snapshot
// -------------------------------------------------------------------------

/// Immutable snapshot of [`RtStats`] suitable for serialisation or
/// passing across threads.
#[derive(Clone, Debug)]
pub struct RtStatsSnapshot {
    pub tick_count: u64,
    pub tick_ok: u64,
    pub missed_ticks: u64,
    pub transport_errors: u64,

    pub latency_sum_us: u64,
    pub latency_max_us: u32,
    pub mean_latency_us: f64,

    pub jitter_min_us: i64,
    pub jitter_max_us: i64,
    pub jitter_abs_sum_us: u64,
    pub mean_abs_jitter_us: f64,

    pub jitter_histogram: [u64; JITTER_NUM_BUCKETS],
}

impl RtStatsSnapshot {
    /// Effective achieved rate over the observed ticks, in Hz, given
    /// the total elapsed wall-clock time in seconds.
    pub fn effective_hz(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs <= 0.0 {
            return 0.0;
        }
        self.tick_ok as f64 / elapsed_secs
    }

    /// Fraction of ticks that met their deadline (0.0..=1.0).
    pub fn success_ratio(&self) -> f64 {
        if self.tick_count == 0 {
            1.0
        } else {
            self.tick_ok as f64 / self.tick_count as f64
        }
    }
}

// =========================================================================
//   Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_stats_are_all_zero() {
        let s = RtStats::new();
        let snap = s.snapshot();
        assert_eq!(snap.tick_count, 0);
        assert_eq!(snap.tick_ok, 0);
        assert_eq!(snap.missed_ticks, 0);
        assert_eq!(snap.transport_errors, 0);
        assert_eq!(snap.latency_max_us, 0);
        assert_eq!(snap.jitter_min_us, 0);
        assert_eq!(snap.jitter_max_us, 0);
        assert_eq!(snap.mean_latency_us, 0.0);
        assert_eq!(snap.mean_abs_jitter_us, 0.0);
        for b in snap.jitter_histogram.iter() {
            assert_eq!(*b, 0);
        }
    }

    #[test]
    fn record_ok_updates_all_counters() {
        let s = RtStats::new();
        s.record_ok(55, 5);
        s.record_ok(60, -3);
        s.record_ok(50, 12);
        let snap = s.snapshot();
        assert_eq!(snap.tick_count, 3);
        assert_eq!(snap.tick_ok, 3);
        assert_eq!(snap.latency_sum_us, 165);
        assert_eq!(snap.latency_max_us, 60);
        assert!((snap.mean_latency_us - 55.0).abs() < 1e-9);
        assert_eq!(snap.jitter_min_us, -3);
        assert_eq!(snap.jitter_max_us, 12);
        assert_eq!(snap.jitter_abs_sum_us, 5 + 3 + 12);
        assert!((snap.mean_abs_jitter_us - (20.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn record_missed_updates_counters() {
        let s = RtStats::new();
        s.record_missed(1);
        s.record_missed(3);
        let snap = s.snapshot();
        assert_eq!(snap.tick_count, 4);
        assert_eq!(snap.missed_ticks, 4);
        assert_eq!(snap.tick_ok, 0);
    }

    #[test]
    fn histogram_bucketing_matches_boundaries() {
        let s = RtStats::new();
        s.record_ok(10, -50); // bucket 0 (early)
        s.record_ok(10, 0); // bucket 1 (0..10)
        s.record_ok(10, 5); // bucket 1
        s.record_ok(10, 9); // bucket 1
        s.record_ok(10, 10); // bucket 2 (10..25)
        s.record_ok(10, 24); // bucket 2
        s.record_ok(10, 25); // bucket 3 (25..50)
        s.record_ok(10, 49); // bucket 3
        s.record_ok(10, 50); // bucket 4 (50..100)
        s.record_ok(10, 100); // bucket 5 (100..200)
        s.record_ok(10, 250); // bucket 6 (200..500)
        s.record_ok(10, 750); // bucket 7 (500..1000)
        s.record_ok(10, 1500); // bucket 8 overflow
        s.record_ok(10, 50_000); // bucket 8 overflow

        let snap = s.snapshot();
        assert_eq!(snap.jitter_histogram[0], 1);
        assert_eq!(snap.jitter_histogram[1], 3);
        assert_eq!(snap.jitter_histogram[2], 2);
        assert_eq!(snap.jitter_histogram[3], 2);
        assert_eq!(snap.jitter_histogram[4], 1);
        assert_eq!(snap.jitter_histogram[5], 1);
        assert_eq!(snap.jitter_histogram[6], 1);
        assert_eq!(snap.jitter_histogram[7], 1);
        assert_eq!(snap.jitter_histogram[8], 2);
    }

    #[test]
    fn reset_zeros_everything() {
        let s = RtStats::new();
        s.record_ok(100, 50);
        s.record_missed(5);
        s.record_transport_error();
        s.reset();
        let snap = s.snapshot();
        assert_eq!(snap.tick_count, 0);
        assert_eq!(snap.tick_ok, 0);
        assert_eq!(snap.missed_ticks, 0);
        assert_eq!(snap.transport_errors, 0);
        assert_eq!(snap.latency_max_us, 0);
        assert_eq!(snap.jitter_min_us, 0);
        assert_eq!(snap.jitter_max_us, 0);
    }

    #[test]
    fn effective_hz_and_success_ratio() {
        let s = RtStats::new();
        for _ in 0..100 {
            s.record_ok(50, 0);
        }
        s.record_missed(5);

        let snap = s.snapshot();
        // 100 ok ticks over 10 ms = 10000 Hz
        assert!((snap.effective_hz(0.01) - 10_000.0).abs() < 1e-6);
        // 100 ok out of 105 total
        assert!((snap.success_ratio() - 100.0 / 105.0).abs() < 1e-9);
    }

    #[test]
    fn concurrent_record_is_safe() {
        // Smoke test: many threads incrementing at once should not
        // corrupt the counters.
        use std::sync::Arc;
        use std::thread;
        let s = Arc::new(RtStats::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let s = Arc::clone(&s);
            handles.push(thread::spawn(move || {
                for _ in 0..10_000 {
                    s.record_ok(50, 5);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let snap = s.snapshot();
        assert_eq!(snap.tick_count, 80_000);
        assert_eq!(snap.tick_ok, 80_000);
        assert_eq!(snap.latency_sum_us, 80_000 * 50);
    }
}
