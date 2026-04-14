//! Observer-pattern publish/subscribe of status frames emitted by
//! the RT scheduler.
//!
//! The scheduler calls [`StatusBus::publish`] once per tick with a
//! [`StatusFrame`] that combines the wire-level [`crate::protocol::Status`]
//! payload with rich host-side temporal metadata: monotonic sequence
//! number, tick index, expected / actual tick timestamps, round-trip
//! latency, jitter and missed-tick accounting.
//!
//! Subscribers are independent: a slow consumer that does not keep
//! up never back-pressures the RT loop. Instead its queue capacity
//! is exceeded, the oldest frames are silently dropped, and the next
//! `recv()` call on that subscriber returns [`tokio::sync::broadcast::error::RecvError::Lagged`]
//! with the count of dropped frames so it can react (increment a
//! metric, reset its view, etc.).
//!
//! The underlying channel is a `tokio::sync::broadcast` — but we
//! only depend on `tokio` with `features = ["sync", "rt"]`, so
//! callers do not need an async runtime: blocking consumers can use
//! [`tokio::sync::broadcast::Receiver::blocking_recv`].

use crate::protocol::Status;
use tokio::sync::broadcast;

/// A status frame emitted by the RT scheduler after every successful
/// exchange.
///
/// In addition to the raw wire [`Status`] payload, this carries
/// host-side temporal metadata calculated by the scheduler at tick
/// time. See the field documentation below for what each value is
/// good for.
#[derive(Clone, Debug)]
pub struct StatusFrame {
    // -----------------------------------------------------------------
    //   Payload
    // -----------------------------------------------------------------
    /// The validated, decoded status payload as it came off the wire.
    pub status: Status,

    // -----------------------------------------------------------------
    //   Identification
    // -----------------------------------------------------------------
    /// Monotonic 64-bit sequence number of the command this status
    /// is a response to. Unlike the 8-bit `wire_seq` (which wraps
    /// every 256 frames), `cmd_seq` is suitable as a globally unique
    /// identifier in logs and metrics over arbitrarily long runs.
    pub cmd_seq: u64,

    /// The `seq_num` field from the wire protocol (the 8-bit counter
    /// the device echoes back). Carried for internal verification
    /// that the chip responded to the right command; subscribers
    /// normally should not need to look at it.
    pub wire_seq: u8,

    /// Index of the RT tick this exchange belongs to. In steady
    /// state `tick_index == cmd_seq`; if the scheduler skipped one
    /// or more ticks to catch up, `tick_index` jumps by `1 +
    /// missed_ticks_prior` while `cmd_seq` advances by one per
    /// actual exchange.
    pub tick_index: u64,

    // -----------------------------------------------------------------
    //   Temporal metadata (all on CLOCK_MONOTONIC in nanoseconds)
    // -----------------------------------------------------------------
    /// Timestamp at which this tick was *supposed* to start,
    /// i.e. `start_ns + tick_index * period_ns`.
    /// Used with [`tick_sent_ns`] to compute [`jitter_us`].
    pub tick_expected_ns: i64,

    /// Timestamp at which the scheduler actually submitted the
    /// command to the transport.
    pub tick_sent_ns: i64,

    /// Timestamp at which the scheduler received the status response
    /// back from the transport.
    pub tick_recv_ns: i64,

    /// Round-trip latency in microseconds
    /// (`tick_recv_ns - tick_sent_ns`, clamped to `u32::MAX`).
    /// Pre-calculated so subscribers don't have to do nanosecond
    /// arithmetic on every sample.
    pub latency_us: u32,

    /// How late (positive) or early (negative) this tick actually
    /// started, in microseconds (`tick_sent_ns - tick_expected_ns`).
    ///
    /// This is *the* diagnostic number for RT health: an ideal
    /// scheduler produces `jitter_us == 0` every tick. A histogram
    /// of this field over a minute of operation tells you everything
    /// about whether the loop is meeting its deadlines.
    pub jitter_us: i32,

    /// Number of ticks skipped between the previously published
    /// frame and this one. In steady state this is always zero. If
    /// the scheduler had to skip one or more tick deadlines to catch
    /// up (for example because the transport took longer than the
    /// tick period), this field lets the subscriber know how many
    /// intermediate states are missing from its view.
    pub missed_ticks_prior: u32,
}

// =========================================================================
//   StatusBus
// =========================================================================

/// Publisher side of the observer pattern for status frames.
///
/// Typically instantiated once by the RT scheduler and shared
/// (cheaply, via `Arc`) with any subsystem that needs to subscribe.
#[derive(Debug)]
pub struct StatusBus {
    tx: broadcast::Sender<StatusFrame>,
}

impl StatusBus {
    /// Create a new status bus with the given per-subscriber queue
    /// capacity.
    ///
    /// A slow subscriber that falls behind by more than `capacity`
    /// frames will observe its oldest backlog dropped and receive
    /// a `RecvError::Lagged(n)` on the next `recv()` call, where
    /// `n` is the number of frames skipped.
    ///
    /// Sensible values: `1024` for dashboards/REST (tolerant of
    /// bursty consumers), `256` for tight observers that want to
    /// see "roughly current" state.
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Subscribe to the bus. The returned receiver will see every
    /// frame published *after* the subscribe call (existing history
    /// is not replayed).
    ///
    /// Callers that want async delivery use `recv().await`; blocking
    /// callers (e.g. the Python dispatcher thread) use
    /// `blocking_recv()`; polling callers use `try_recv()`.
    pub fn subscribe(&self) -> broadcast::Receiver<StatusFrame> {
        self.tx.subscribe()
    }

    /// Publish a frame. Returns the number of active subscribers that
    /// were notified. Returns 0 if there are no subscribers; this is
    /// *not* an error — the RT scheduler always publishes regardless
    /// of whether anyone is listening.
    pub fn publish(&self, frame: StatusFrame) -> usize {
        // broadcast::Sender::send returns Err(SendError) only when
        // there are zero active receivers. We treat that as "no-op,
        // zero delivered".
        self.tx.send(frame).unwrap_or(0)
    }

    /// Return the number of currently active subscribers.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for StatusBus {
    fn default() -> Self {
        // 1024-frame backlog is a comfortable default: at 10 kHz it
        // gives a slow consumer 100 ms of latency tolerance before
        // it starts seeing Lagged errors.
        Self::new(1024)
    }
}

// =========================================================================
//   Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Status;
    use tokio::sync::broadcast::error::TryRecvError;

    fn make_frame(cmd_seq: u64) -> StatusFrame {
        StatusFrame {
            status: Status::default(),
            cmd_seq,
            wire_seq: (cmd_seq & 0xFF) as u8,
            tick_index: cmd_seq,
            tick_expected_ns: 1_000_000 * cmd_seq as i64,
            tick_sent_ns: 1_000_000 * cmd_seq as i64 + 100,
            tick_recv_ns: 1_000_000 * cmd_seq as i64 + 50_000,
            latency_us: 49,
            jitter_us: 0,
            missed_ticks_prior: 0,
        }
    }

    #[test]
    fn publish_with_no_subscribers_is_noop() {
        let bus = StatusBus::new(16);
        // Does not panic, returns 0.
        let delivered = bus.publish(make_frame(0));
        assert_eq!(delivered, 0);
        assert_eq!(bus.receiver_count(), 0);
    }

    #[test]
    fn single_subscriber_receives_frame() {
        let bus = StatusBus::new(16);
        let mut rx = bus.subscribe();

        let n = bus.publish(make_frame(42));
        assert_eq!(n, 1);

        let f = rx.try_recv().expect("frame should be available");
        assert_eq!(f.cmd_seq, 42);
        assert_eq!(f.wire_seq, 42);
    }

    #[test]
    fn multiple_subscribers_each_receive_every_frame() {
        let bus = StatusBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        let mut rx3 = bus.subscribe();

        for i in 0..5u64 {
            bus.publish(make_frame(i));
        }

        for rx in [&mut rx1, &mut rx2, &mut rx3] {
            for i in 0..5u64 {
                let f = rx.try_recv().unwrap();
                assert_eq!(f.cmd_seq, i);
            }
            assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
        }
    }

    #[test]
    fn slow_subscriber_sees_lagged_error() {
        // Capacity = 4. Push 10 frames. The slow subscriber that
        // never calls recv will observe a Lagged error when it
        // finally polls, with the count of missed frames.
        let bus = StatusBus::new(4);
        let mut rx_slow = bus.subscribe();

        for i in 0..10u64 {
            bus.publish(make_frame(i));
        }

        // The first try_recv should return Lagged(N) for some N >= 6,
        // then recover with the still-buffered tail.
        match rx_slow.try_recv() {
            Err(TryRecvError::Lagged(n)) => {
                assert!(n >= 6, "expected to lag by at least 6, got {n}");
            }
            other => panic!("expected Lagged, got {other:?}"),
        }

        // After the Lagged error, the remaining (capacity) frames
        // should be deliverable.
        let mut received = Vec::new();
        loop {
            match rx_slow.try_recv() {
                Ok(f) => received.push(f.cmd_seq),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => panic!("bus closed unexpectedly"),
            }
        }
        // We should see the last ~4 frames (the buffered tail).
        assert!(!received.is_empty());
        assert!(received.iter().all(|&s| s >= 6));
    }

    #[test]
    fn rt_loop_is_never_blocked_by_slow_consumer() {
        // Smoke test: 100_000 publishes with one slow subscriber
        // who never reads. publish() must never block, and must
        // never return an error that the scheduler would propagate.
        let bus = StatusBus::new(8);
        let _rx_slow = bus.subscribe(); // held but never polled

        use std::time::Instant;
        let t0 = Instant::now();
        for i in 0..100_000u64 {
            bus.publish(make_frame(i));
        }
        let elapsed = t0.elapsed();

        // 100 k publish operations should be sub-second on any
        // modern CPU even in debug mode. This would fail loudly
        // if publish() somehow acquired a contended lock or
        // allocated per-call.
        assert!(
            elapsed.as_millis() < 2000,
            "publish path is too slow: {} ms for 100k frames",
            elapsed.as_millis()
        );
    }

    #[test]
    fn frame_fields_preserved_end_to_end() {
        let bus = StatusBus::new(4);
        let mut rx = bus.subscribe();
        let sent = StatusFrame {
            status: Status::default(),
            cmd_seq: 0xCAFE_BABE,
            wire_seq: 0x42,
            tick_index: 0xBEEF,
            tick_expected_ns: 1_111_222_333,
            tick_sent_ns: 1_111_222_555,
            tick_recv_ns: 1_111_250_555,
            latency_us: 28_000,
            jitter_us: 222,
            missed_ticks_prior: 3,
        };
        bus.publish(sent.clone());
        let got = rx.try_recv().unwrap();

        assert_eq!(got.cmd_seq, sent.cmd_seq);
        assert_eq!(got.wire_seq, sent.wire_seq);
        assert_eq!(got.tick_index, sent.tick_index);
        assert_eq!(got.tick_expected_ns, sent.tick_expected_ns);
        assert_eq!(got.tick_sent_ns, sent.tick_sent_ns);
        assert_eq!(got.tick_recv_ns, sent.tick_recv_ns);
        assert_eq!(got.latency_us, sent.latency_us);
        assert_eq!(got.jitter_us, sent.jitter_us);
        assert_eq!(got.missed_ticks_prior, sent.missed_ticks_prior);
    }

    #[test]
    fn receiver_count_tracks_subscribers() {
        let bus = StatusBus::new(4);
        assert_eq!(bus.receiver_count(), 0);

        let rx1 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);

        let rx2 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);

        drop(rx1);
        assert_eq!(bus.receiver_count(), 1);

        drop(rx2);
        assert_eq!(bus.receiver_count(), 0);
    }
}
