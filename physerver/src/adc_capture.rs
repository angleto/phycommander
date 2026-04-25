// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>

//! Full-rate ADC sample ring for the dashboard scope.
//!
//! The WebSocket `Status` broadcast is rate-limited to ~250 Hz (see
//! `main.rs`: WS_MIN_INTERVAL = 4 ms). That is fine for scalar read-outs
//! but throws away 97 % of the samples for anyone trying to reconstruct
//! a waveform — e.g. a 500 Hz sine generated on DAC1 ends up as two
//! samples per period and renders as jagged nonsense, even though the
//! firmware is dutifully ADC-sampling it at 8 kHz.
//!
//! This ring captures every frame straight off the iso RX bus so the
//! scope endpoint can hand a client the full 8 kHz stream. Sizing is
//! driven by the dashboard's longest default window (2 s @ 8 kHz =
//! 16384 samples). Longer windows on the client degrade gracefully —
//! they just render whatever the ring currently holds.
//!
//! The ring is write-exclusive from the bus-drain task in `main.rs`
//! and read-shared by the HTTP handler, so a `parking_lot::Mutex` is
//! adequate — contention is negligible at 8 kHz push vs ~10 Hz poll.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;

/// Max samples held in the ring. 16384 covers a 2 s window at 8 kHz
/// which matches the dashboard scope's longest "full fidelity" mode;
/// wider windows still work but start decimating on the client.
pub const CAPACITY: usize = 16384;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AdcSample {
    /// Monotonic sample index. Starts at 0, incremented per push.
    pub seq: u64,
    /// 8 raw ADC channel values (12-bit, 0..4095).
    pub adc: [u16; 8],
    /// GPIO digital input mirror so the scope can render DIN traces
    /// at full rate too.
    pub din: u16,
    /// GPIO digital output echo.
    pub dout: u16,
}

pub struct AdcRing {
    next_seq: u64,
    buf: VecDeque<AdcSample>,
}

impl AdcRing {
    pub fn new() -> Self {
        Self { next_seq: 0, buf: VecDeque::with_capacity(CAPACITY) }
    }

    pub fn push(&mut self, adc: [u16; 8], din: u16, dout: u16) {
        let sample = AdcSample { seq: self.next_seq, adc, din, dout };
        self.next_seq = self.next_seq.wrapping_add(1);
        if self.buf.len() == CAPACITY {
            self.buf.pop_front();
        }
        self.buf.push_back(sample);
    }

    /// Collect up to `max` most-recent samples with `seq >= since`.
    /// Returns the samples in chronological order.
    pub fn snapshot_since(&self, since: u64, max: usize) -> Vec<AdcSample> {
        // Find first sample whose seq >= since. If the ring wrapped
        // past `since`, start from the oldest sample we still have.
        let start_idx = self.buf.partition_point(|s| s.seq < since);
        let total = self.buf.len() - start_idx;
        let skip = total.saturating_sub(max);
        self.buf
            .iter()
            .skip(start_idx + skip)
            .copied()
            .collect()
    }

    /// First/last seq currently held. Returns `(0, 0)` on empty ring.
    pub fn span(&self) -> (u64, u64) {
        match (self.buf.front(), self.buf.back()) {
            (Some(a), Some(b)) => (a.seq, b.seq),
            _ => (0, 0),
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl Default for AdcRing {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedAdcRing = std::sync::Arc<Mutex<AdcRing>>;

pub fn new_shared() -> SharedAdcRing {
    std::sync::Arc::new(Mutex::new(AdcRing::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_wraps_and_preserves_seq() {
        let mut r = AdcRing::new();
        // Fill past capacity.
        for i in 0..(CAPACITY + 100) {
            r.push([i as u16; 8], 0, 0);
        }
        assert_eq!(r.len(), CAPACITY);
        let (first, last) = r.span();
        assert_eq!(last - first + 1, CAPACITY as u64);
        // `since` older than the ring → we get whatever is left.
        let snap = r.snapshot_since(0, usize::MAX);
        assert_eq!(snap.len(), CAPACITY);
        assert_eq!(snap[0].seq, first);
        // `since` newer than the tail → empty.
        let snap = r.snapshot_since(last + 1, usize::MAX);
        assert!(snap.is_empty());
    }

    #[test]
    fn snapshot_since_respects_max() {
        let mut r = AdcRing::new();
        for i in 0..100 {
            r.push([i; 8], 0, 0);
        }
        let snap = r.snapshot_since(0, 10);
        assert_eq!(snap.len(), 10);
        // Most-recent 10 samples: seq 90..99.
        assert_eq!(snap.first().unwrap().seq, 90);
        assert_eq!(snap.last().unwrap().seq, 99);
    }
}
