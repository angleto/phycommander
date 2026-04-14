//! `phycmd` — ergonomic Rust API for the PhyCommander hard-real-time
//! control loop.
//!
//! This crate is a thin, user-facing layer on top of `phycmd-core`.
//! It hides the fact that the RT scheduler, the staging buffer, the
//! status bus and the stats counters are separate pieces and gives
//! the caller a single [`PhyCommander`] handle with ergonomic
//! setters, a subscribe method, and graceful `Drop`-based teardown.
//!
//! ## Quick start
//!
//! ```no_run
//! use phycmd::{PhyCommander, RtConfig, WriteMode};
//! # use phycmd::Transport;
//! # fn build_transport() -> Box<dyn Transport> { unimplemented!() }
//!
//! let config = RtConfig {
//!     rate_hz: 10_000,
//!     default_write_mode: WriteMode::BlockUntilSent,
//!     ..Default::default()
//! };
//!
//! let phy = PhyCommander::open(config, build_transport())?;
//!
//! // Writes use the sticky default mode (BlockUntilSent):
//! phy.set_dac0(1234)?;
//! phy.set_dac0(5678)?;             // blocks until the 1234 has gone out
//!
//! // Or override per call:
//! phy.set_dac0_with(WriteMode::Coalesce, 9012)?;
//!
//! // Subscribe to status frames
//! let mut rx = phy.subscribe();
//! while let Ok(frame) = rx.blocking_recv() {
//!     println!("tick={} adc0={}", frame.cmd_seq, frame.status.adc[0]);
//! }
//! # anyhow::Ok(())
//! ```
//!
//! When the [`PhyCommander`] handle is dropped, the RT loop is
//! stopped, the staging waiters are unblocked, the scheduler thread
//! is joined, and the transport is closed.

use std::sync::Arc;
use std::thread::{self, JoinHandle};

use phycmd_core::{CommandStaging, RtScheduler, RtSchedulerStopHandle, RtStats, StatusBus};
use thiserror::Error;
use tracing::{debug, warn};

// Re-export the types the caller needs to pass to / receive from
// this API so they don't have to also depend on `phycmd-core`.
pub use phycmd_core::{
    Command, CommandFlags, RtConfig, RtConfigError, RtStatsSnapshot, StagingError, Status,
    StatusFlags, Transport, TransportStats, WriteMode,
};

// Re-export the PipelinedTransport trait for callers that want to
// open_pipelined().
pub use phycmd_core::PipelinedTransport;

// Real transports are optional — they need libudev / libusb at
// build time. Users can still pass any `Box<dyn Transport>` they
// built themselves.
pub use phycmd_core::stats::{JITTER_BUCKET_BOUNDS_US, JITTER_NUM_BUCKETS};
pub use phycmd_core::status_bus::StatusFrame;
pub use phycmd_core::transport::SerialTransport;
#[cfg(feature = "usb")]
pub use phycmd_core::transport::{
    PipelinedUsbLoopbackTransport, UsbLoopbackTransport, UsbTransport,
};

// Re-export the broadcast receiver type subscribers interact with.
pub type StatusReceiver = tokio::sync::broadcast::Receiver<StatusFrame>;

// Also re-export the transport mock when the feature is on, so
// users can build integration tests without talking to hardware.
#[cfg(any(test, feature = "test-mock"))]
pub use phycmd_core::transport::mock::{MockState, MockTransport};

// -------------------------------------------------------------------------
//   Errors
// -------------------------------------------------------------------------

/// All the ways `PhyCommander::open` can fail.
#[derive(Debug, Error)]
pub enum OpenError {
    #[error("invalid RT configuration: {0}")]
    InvalidConfig(#[from] RtConfigError),

    #[error("failed to spawn RT scheduler thread: {0}")]
    SpawnFailed(#[from] std::io::Error),
}

// -------------------------------------------------------------------------
//   PhyCommander
// -------------------------------------------------------------------------

/// A running PhyCommander instance.
///
/// Cheap to clone via [`Arc`] internally; the [`PhyCommander`] struct
/// itself is *not* `Clone` because it owns the teardown responsibility
/// (Drop joins the scheduler thread). If you need multiple handles
/// to the same instance, wrap it in an `Arc<PhyCommander>`.
#[derive(Debug)]
pub struct PhyCommander {
    staging: Arc<CommandStaging>,
    bus: Arc<StatusBus>,
    stats: Arc<RtStats>,
    stop_handle: RtSchedulerStopHandle,
    join_handle: Option<JoinHandle<anyhow::Result<()>>>,
}

impl PhyCommander {
    /// Open a new PhyCommander with the given config and transport.
    ///
    /// The RT scheduler is started in a dedicated `std::thread` named
    /// `"phycmd-rt"`. Kernel RT priorities (`SCHED_FIFO`, `mlockall`,
    /// CPU affinity, DMA latency) are applied to that thread if the
    /// config asks for them; lacking privileges, a warning is logged
    /// and the loop continues under normal scheduling policy.
    ///
    /// The returned handle will terminate the scheduler and join its
    /// thread on `Drop`. To shut down gracefully without dropping,
    /// call [`PhyCommander::stop`] then [`PhyCommander::join`].
    pub fn open(config: RtConfig, transport: Box<dyn Transport>) -> Result<Self, OpenError> {
        config.validate()?;
        let (staging, bus, stats) = Self::shared_state(&config);
        let scheduler = RtScheduler::new(
            config,
            Arc::clone(&staging),
            Arc::clone(&bus),
            Arc::clone(&stats),
            transport,
        );
        Self::spawn(staging, bus, stats, scheduler)
    }

    /// Open with a pipelined transport (submit/sleep/reap overlap).
    /// The USB round-trip happens during `clock_nanosleep`, so the
    /// scheduler thread CPU footprint is ~15 μs instead of ~350 μs
    /// per tick, enabling higher sustained rates.
    pub fn open_pipelined(
        config: RtConfig,
        transport: Box<dyn PipelinedTransport>,
    ) -> Result<Self, OpenError> {
        config.validate()?;
        let (staging, bus, stats) = Self::shared_state(&config);
        let scheduler = RtScheduler::new_pipelined(
            config,
            Arc::clone(&staging),
            Arc::clone(&bus),
            Arc::clone(&stats),
            transport,
        );
        Self::spawn(staging, bus, stats, scheduler)
    }

    fn shared_state(config: &RtConfig) -> (Arc<CommandStaging>, Arc<StatusBus>, Arc<RtStats>) {
        let staging = Arc::new(CommandStaging::new(config.default_write_mode));
        let bus = Arc::new(StatusBus::new(config.status_bus_capacity));
        let stats = Arc::new(RtStats::new());
        (staging, bus, stats)
    }

    fn spawn(
        staging: Arc<CommandStaging>,
        bus: Arc<StatusBus>,
        stats: Arc<RtStats>,
        scheduler: RtScheduler,
    ) -> Result<Self, OpenError> {
        let stop_handle = scheduler.stop_handle();
        debug!("spawning phycmd-rt scheduler thread");
        let join_handle = thread::Builder::new()
            .name("phycmd-rt".to_string())
            .spawn(move || scheduler.run())?;
        Ok(Self { staging, bus, stats, stop_handle, join_handle: Some(join_handle) })
    }

    // =================================================================
    //   Write mode configuration
    // =================================================================

    /// Change the sticky write mode used by all `set_*` methods that
    /// do not pass an explicit per-call override.
    pub fn set_default_mode(&self, mode: WriteMode) {
        self.staging.set_default_mode(mode);
    }

    /// Current sticky write mode.
    pub fn default_mode(&self) -> WriteMode {
        self.staging.default_mode()
    }

    // =================================================================
    //   DAC setters
    // =================================================================

    pub fn set_dac0(&self, value: u16) -> Result<(), StagingError> {
        self.staging.set_dac0(value)
    }
    pub fn set_dac0_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.staging.set_dac0_with(mode, value)
    }

    pub fn set_dac1(&self, value: u16) -> Result<(), StagingError> {
        self.staging.set_dac1(value)
    }
    pub fn set_dac1_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.staging.set_dac1_with(mode, value)
    }

    // =================================================================
    //   PWM setters
    // =================================================================

    pub fn set_pwm0(&self, value: u16) -> Result<(), StagingError> {
        self.staging.set_pwm0(value)
    }
    pub fn set_pwm0_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.staging.set_pwm0_with(mode, value)
    }

    pub fn set_pwm1(&self, value: u16) -> Result<(), StagingError> {
        self.staging.set_pwm1(value)
    }
    pub fn set_pwm1_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.staging.set_pwm1_with(mode, value)
    }

    // =================================================================
    //   Flags
    // =================================================================

    pub fn set_flags(&self, flags: CommandFlags) -> Result<(), StagingError> {
        self.staging.set_flags(flags)
    }
    pub fn set_flags_with(&self, mode: WriteMode, flags: CommandFlags) -> Result<(), StagingError> {
        self.staging.set_flags_with(mode, flags)
    }

    // =================================================================
    //   Digital outputs
    // =================================================================

    /// Overwrite all 16 digital output bits at once. In BlockUntilSent
    /// mode blocks until the entire `digital_out` word is clean.
    pub fn set_digital_out(&self, mask: u16) -> Result<(), StagingError> {
        self.staging.set_digital_out(mask)
    }
    pub fn set_digital_out_with(&self, mode: WriteMode, mask: u16) -> Result<(), StagingError> {
        self.staging.set_digital_out_with(mode, mask)
    }

    /// Set a single digital output bit (0..16). In BlockUntilSent
    /// mode blocks only on the specific bit, so writers toggling
    /// different bits never block each other.
    pub fn set_digital_out_bit(&self, bit: u8, value: bool) -> Result<(), StagingError> {
        self.staging.set_digital_out_bit(bit, value)
    }
    pub fn set_digital_out_bit_with(
        &self,
        mode: WriteMode,
        bit: u8,
        value: bool,
    ) -> Result<(), StagingError> {
        self.staging.set_digital_out_bit_with(mode, bit, value)
    }

    // =================================================================
    //   Observer — status bus
    // =================================================================

    /// Subscribe to the status bus. The returned receiver will see
    /// every status frame produced *after* the subscribe call.
    ///
    /// A slow subscriber will observe its oldest backlog dropped and
    /// get a `broadcast::error::RecvError::Lagged(n)` on the next
    /// `recv`. The RT loop is never back-pressured by a slow consumer.
    pub fn subscribe(&self) -> StatusReceiver {
        self.bus.subscribe()
    }

    /// Number of currently active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.bus.receiver_count()
    }

    // =================================================================
    //   Stats
    // =================================================================

    /// Take a cloneable snapshot of the RT scheduler statistics
    /// (tick count, missed ticks, latency/jitter histogram, ...).
    pub fn stats(&self) -> RtStatsSnapshot {
        self.stats.snapshot()
    }

    /// Reset the RT statistics. Does not affect the scheduler loop.
    pub fn reset_stats(&self) {
        self.stats.reset();
    }

    // =================================================================
    //   Lifecycle
    // =================================================================

    /// Signal the scheduler to stop at the next tick boundary. Does
    /// not wait for it to actually finish; call [`join`] or drop
    /// the `PhyCommander` to block on completion.
    pub fn stop(&self) {
        self.stop_handle.stop();
    }

    /// Stop the scheduler and wait for the thread to exit.
    /// After `join`, subsequent operations on this handle will
    /// still work against the buffered staging/bus state, but
    /// no new status frames will be produced.
    pub fn join(&mut self) -> anyhow::Result<()> {
        self.stop_handle.stop();
        if let Some(h) = self.join_handle.take() {
            h.join().map_err(|_| anyhow::anyhow!("RT thread panicked"))?
        } else {
            Ok(())
        }
    }

    /// `true` if the scheduler thread is still running.
    pub fn is_running(&self) -> bool {
        self.join_handle.as_ref().map(|h| !h.is_finished()).unwrap_or(false)
    }
}

impl Drop for PhyCommander {
    fn drop(&mut self) {
        self.stop_handle.stop();
        if let Some(h) = self.join_handle.take() {
            match h.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => warn!("RT scheduler returned error on shutdown: {}", e),
                Err(_) => warn!("RT scheduler thread panicked"),
            }
        }
    }
}

// =========================================================================
//   Unit tests (use MockTransport via the `test-mock` feature)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};
    use tokio::sync::broadcast::error::TryRecvError;

    fn test_config(rate_hz: u32) -> RtConfig {
        RtConfig {
            rate_hz,
            default_write_mode: WriteMode::BlockUntilSent,
            enable_rt: false,
            lock_memory: false,
            dma_latency_us: None,
            cpu_affinity: None,
            ..Default::default()
        }
    }

    fn mock_transport_with_latency(us: u64) -> (Box<dyn Transport>, Arc<Mutex<MockState>>) {
        let state = Arc::new(Mutex::new(MockState {
            latency: Duration::from_micros(us),
            ..Default::default()
        }));
        let t = Box::new(MockTransport::with_state(Arc::clone(&state)));
        (t, state)
    }

    #[test]
    fn open_and_drop_cleanly() {
        let (transport, _mock) = mock_transport_with_latency(50);
        let phy = PhyCommander::open(test_config(1_000), transport).unwrap();
        assert!(phy.is_running());
        thread::sleep(Duration::from_millis(30));
        drop(phy);
        // Dropping must unblock any pending waiters and join the
        // thread without deadlocking — the assertion here is simply
        // "we got past Drop without hanging".
    }

    #[test]
    fn setters_are_reflected_on_the_wire() {
        let (transport, mock) = mock_transport_with_latency(50);
        let phy = PhyCommander::open(test_config(2_000), transport).unwrap();

        // With BlockUntilSent default, every setter implicitly waits
        // for the previous frame to go out. After 20 successful
        // writes we should have roughly 20 observed commands on the
        // mock.
        for i in 0..20u16 {
            phy.set_dac0(i * 10).unwrap();
        }
        // Give the scheduler one more tick to flush.
        thread::sleep(Duration::from_millis(20));

        let observed = {
            let s = mock.lock();
            s.observed_commands.clone()
        };
        // The last observed command must carry dac0 = 190 at some
        // point in the trailing frames (Coalesce-free, so 190
        // eventually lands).
        let has_final = observed.iter().any(|c| c.dac[0] == 190);
        assert!(has_final, "final dac0 value never reached the transport");
    }

    #[test]
    fn subscriber_receives_frames_in_order() {
        let (transport, _mock) = mock_transport_with_latency(50);
        let phy = PhyCommander::open(test_config(2_000), transport).unwrap();
        let mut rx = phy.subscribe();

        thread::sleep(Duration::from_millis(100));
        phy.stop();

        let mut prev: i64 = -1;
        let mut n = 0usize;
        loop {
            match rx.try_recv() {
                Ok(frame) => {
                    assert!((frame.cmd_seq as i64) > prev);
                    prev = frame.cmd_seq as i64;
                    n += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => break,
            }
        }
        assert!(n > 50, "expected ≥50 frames, got {n}");
    }

    #[test]
    fn stats_are_exposed_and_updated() {
        let (transport, _mock) = mock_transport_with_latency(50);
        let phy = PhyCommander::open(test_config(1_000), transport).unwrap();

        let before = phy.stats();
        assert_eq!(before.tick_ok, 0);

        thread::sleep(Duration::from_millis(100));
        let after = phy.stats();
        assert!(after.tick_ok > 50);
        assert!(after.latency_max_us > 0);
    }

    #[test]
    fn stop_without_drop_still_joins() {
        let (transport, _mock) = mock_transport_with_latency(50);
        let mut phy = PhyCommander::open(test_config(1_000), transport).unwrap();
        thread::sleep(Duration::from_millis(30));
        phy.join().unwrap();
        assert!(!phy.is_running());
    }

    #[test]
    fn block_until_sent_writer_is_released() {
        // End-to-end: with BlockUntilSent mode, two writes from two
        // threads to the same field must serialise but both must
        // eventually complete.
        let (transport, _mock) = mock_transport_with_latency(100);
        let phy = Arc::new(PhyCommander::open(test_config(1_000), transport).unwrap());

        let phy2 = Arc::clone(&phy);
        let t0 = Instant::now();
        let h1 = thread::spawn(move || {
            phy2.set_dac0(100).unwrap();
        });
        let phy3 = Arc::clone(&phy);
        let h2 = thread::spawn(move || {
            phy3.set_dac0(200).unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();
        let elapsed = t0.elapsed();
        // At 1 kHz two writes are serialised at most a few ms apart.
        assert!(
            elapsed < Duration::from_millis(100),
            "writers took too long: {}ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn per_call_override_bypasses_sticky_mode() {
        let (transport, _mock) = mock_transport_with_latency(200);
        let phy = PhyCommander::open(test_config(500), transport).unwrap();

        // Sticky mode is BlockUntilSent. Set a first value, then
        // immediately set another with Coalesce override. The
        // Coalesce override should NOT block, even though the
        // previous value is still dirty.
        phy.set_dac0(100).unwrap();
        let t0 = Instant::now();
        phy.set_dac0_with(WriteMode::Coalesce, 200).unwrap();
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_millis(50),
            "Coalesce override was blocked ({}ms)",
            elapsed.as_millis()
        );
    }

    #[test]
    fn set_default_mode_takes_effect() {
        let (transport, _mock) = mock_transport_with_latency(50);
        let phy = PhyCommander::open(test_config(500), transport).unwrap();
        assert_eq!(phy.default_mode(), WriteMode::BlockUntilSent);
        phy.set_default_mode(WriteMode::Coalesce);
        assert_eq!(phy.default_mode(), WriteMode::Coalesce);

        // A burst of writes in Coalesce should never block.
        let t0 = Instant::now();
        for i in 0..1000u16 {
            phy.set_dac0(i).unwrap();
        }
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_millis(20),
            "Coalesce burst blocked: {}ms for 1000 writes",
            elapsed.as_millis()
        );
    }

    #[test]
    fn invalid_config_rejected() {
        let (transport, _mock) = mock_transport_with_latency(50);
        let bad = RtConfig { rate_hz: 0, ..test_config(1_000) };
        match PhyCommander::open(bad, transport) {
            Err(OpenError::InvalidConfig(_)) => {}
            other => panic!("expected InvalidConfig error, got {other:?}"),
        }
    }

    #[test]
    fn subscriber_count_tracks_lifecycles() {
        let (transport, _mock) = mock_transport_with_latency(50);
        let phy = PhyCommander::open(test_config(1_000), transport).unwrap();
        assert_eq!(phy.subscriber_count(), 0);
        let rx1 = phy.subscribe();
        assert_eq!(phy.subscriber_count(), 1);
        let rx2 = phy.subscribe();
        assert_eq!(phy.subscriber_count(), 2);
        drop(rx1);
        assert_eq!(phy.subscriber_count(), 1);
        drop(rx2);
        assert_eq!(phy.subscriber_count(), 0);
    }
}
