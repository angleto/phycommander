//! Hard-paced real-time scheduler.
//!
//! This is the loop that turns staging writes into actual USB
//! traffic at a deterministic tick rate. It ties together the
//! other modules in this crate:
//!
//!   * [`crate::staging::CommandStaging`] — composes the next frame
//!   * [`crate::transport::Transport`]    — sends it on the wire
//!   * [`crate::status_bus::StatusBus`]   — broadcasts the response
//!   * [`crate::stats::RtStats`]          — counts everything
//!   * [`crate::rt_setup`]                — SCHED_FIFO / mlockall / affinity
//!
//! ## Timing discipline
//!
//! The scheduler uses `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)`
//! with an absolute target timestamp that is advanced by exactly
//! `period_ns` each iteration, *independently* of how long the
//! previous iteration took. This is the canonical pattern for
//! hard-RT loops on Linux: it eliminates the drift that a naive
//! `sleep(period)` after the I/O would accumulate, and lets jitter
//! be accounted for explicitly.
//!
//! If a tick overruns its deadline (e.g. the transport took longer
//! than the period), the scheduler *does not* immediately fire the
//! next tick to "catch up". Instead it computes how many periods
//! have already elapsed, logs that as `missed_ticks`, and schedules
//! the next tick at the earliest future boundary. This preserves
//! the phase of the loop — subsequent ticks still fall on the
//! original grid — and surfaces slippage as an explicit metric
//! rather than silently corrupting timing.

use crate::config::RtConfig;
use crate::staging::CommandStaging;
use crate::stats::RtStats;
use crate::status_bus::{StatusBus, StatusFrame};
use crate::transport::{PipelinedTransport, Transport};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

// -------------------------------------------------------------------------
//   Stop handle
// -------------------------------------------------------------------------

/// Shared stop flag. Cloneable. Calling [`RtSchedulerStopHandle::stop`]
/// asks the scheduler loop to exit at the top of the next tick.
#[derive(Clone, Debug)]
pub struct RtSchedulerStopHandle {
    flag: Arc<AtomicBool>,
}

impl RtSchedulerStopHandle {
    /// Signal the scheduler to stop. Safe to call from any thread.
    pub fn stop(&self) {
        self.flag.store(true, Ordering::Release);
    }
    pub fn is_stopping(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

// -------------------------------------------------------------------------
//   RtScheduler
// -------------------------------------------------------------------------

pub struct RtScheduler {
    config: RtConfig,
    staging: Arc<CommandStaging>,
    bus: Arc<StatusBus>,
    stats: Arc<RtStats>,
    transport_sync: Option<Box<dyn Transport>>,
    transport_pipe: Option<Box<dyn PipelinedTransport>>,
    stop: Arc<AtomicBool>,
}

impl RtScheduler {
    /// Construct a scheduler with a synchronous transport
    /// (write+read blocking per tick).
    pub fn new(
        config: RtConfig,
        staging: Arc<CommandStaging>,
        bus: Arc<StatusBus>,
        stats: Arc<RtStats>,
        transport: Box<dyn Transport>,
    ) -> Self {
        Self {
            config,
            staging,
            bus,
            stats,
            transport_sync: Some(transport),
            transport_pipe: None,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Construct a scheduler with a pipelined transport
    /// (submit/reap overlapped with sleep).
    pub fn new_pipelined(
        config: RtConfig,
        staging: Arc<CommandStaging>,
        bus: Arc<StatusBus>,
        stats: Arc<RtStats>,
        transport: Box<dyn PipelinedTransport>,
    ) -> Self {
        Self {
            config,
            staging,
            bus,
            stats,
            transport_sync: None,
            transport_pipe: Some(transport),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Return a handle that can be used to request loop termination
    /// from another thread.
    pub fn stop_handle(&self) -> RtSchedulerStopHandle {
        RtSchedulerStopHandle { flag: Arc::clone(&self.stop) }
    }

    /// Run the scheduler loop on the *current* thread until
    /// [`RtSchedulerStopHandle::stop`] is called from somewhere.
    ///
    /// If `config.enable_rt` is true, RT optimisations are applied
    /// (SCHED_FIFO priority, memory locking, CPU affinity, DMA
    /// latency) **on this thread** before entering the loop. A
    /// typical deployment spawns a dedicated `std::thread::Builder`
    /// and calls this inside the closure; the HTTP server, WebSocket
    /// endpoints, IPC shared memory, etc. all live on other threads
    /// with normal scheduling policy.
    /// Run the scheduler loop on the *current* thread until
    /// [`RtSchedulerStopHandle::stop`] is called.
    ///
    /// Dispatches to the sync or pipelined inner loop based on the
    /// transport provided at construction time. RT optimisations are
    /// applied once at the top of `run`, regardless of mode.
    pub fn run(mut self) -> Result<()> {
        self.config.validate()?;

        if self.config.is_aggressive_rate() {
            warn!(
                "rate_hz={} is above the measured safe ceiling (~15 kHz) \
                 on EHCI + SAM3X hardware; missed ticks are likely",
                self.config.rate_hz
            );
        }

        if self.config.enable_rt {
            let rt_cfg = crate::rt_setup::RtConfig {
                enable_rt_scheduler: true,
                rt_priority: self.config.rt_priority,
                lock_memory: self.config.lock_memory,
                set_cpu_affinity: self.config.cpu_affinity.is_some(),
                cpu_core: self.config.cpu_affinity,
                set_dma_latency: self.config.dma_latency_us.is_some(),
            };
            if let Err(e) = crate::rt_setup::apply_rt_optimizations(&rt_cfg) {
                warn!("failed to apply some RT optimisations: {}", e);
            }
        }

        if let Some(mut t) = self.transport_sync.take() {
            self.run_sync(t.as_mut())
        } else if let Some(mut t) = self.transport_pipe.take() {
            self.run_pipelined(t.as_mut())
        } else {
            anyhow::bail!("no transport configured")
        }
    }

    // =================================================================
    //   Synchronous loop (original — one blocking exchange per tick)
    // =================================================================

    fn run_sync(&self, transport: &mut dyn Transport) -> Result<()> {
        let period_ns = self.config.tick_period().as_nanos() as i64;
        let start_ns = now_monotonic_ns();
        info!(
            "RtScheduler running (sync): rate={} Hz, period={} ns",
            self.config.rate_hz, period_ns
        );

        let mut tick_index: u64 = 0;
        let mut cmd_seq: u64 = 0;

        loop {
            if self.stop.load(Ordering::Acquire) {
                info!("RtScheduler stop: {} ticks", tick_index);
                return Ok(());
            }

            let tick_expected_ns = start_ns + (tick_index as i64) * period_ns;
            if let Err(e) = sleep_until_abs_monotonic(tick_expected_ns) {
                error!("clock_nanosleep: {e}");
                return Err(e.into());
            }

            let tick_sent_ns = now_monotonic_ns();
            let jitter_ns = tick_sent_ns - tick_expected_ns;
            let jitter_us = clamp_to_i32(jitter_ns / 1_000);

            let missed_prior = if jitter_ns > period_ns {
                let n = (jitter_ns / period_ns) as u64;
                self.stats.record_missed(n as u32);
                n as u32
            } else {
                0
            };
            tick_index = tick_index + 1 + missed_prior as u64;

            let (mut cmd, write_gen) = self.staging.take_snapshot();
            cmd.seq_num = (cmd_seq & 0xFF) as u8;

            let io_result = transport.exchange(&cmd);
            self.staging.mark_sent(write_gen);
            let tick_recv_ns = now_monotonic_ns();

            match io_result {
                Ok(status) => {
                    let latency_us = clamp_to_u32((tick_recv_ns - tick_sent_ns) / 1_000);
                    self.stats.record_ok(latency_us, jitter_us);
                    let wire_seq = status.seq_num;
                    self.bus.publish(StatusFrame {
                        status,
                        cmd_seq,
                        wire_seq,
                        tick_index,
                        tick_expected_ns,
                        tick_sent_ns,
                        tick_recv_ns,
                        latency_us,
                        jitter_us,
                        missed_ticks_prior: missed_prior,
                    });
                }
                Err(e) => {
                    trace!("sync exchange failed: {e}");
                    self.stats.record_transport_error();
                    self.stats.record_missed(1);
                }
            }

            cmd_seq = cmd_seq.wrapping_add(1);
        }
    }

    // =================================================================
    //   Pipelined loop — submit/sleep/reap overlapped
    //
    //   Timeline for a single tick:
    //
    //     tick N wakeup
    //       ├─ REAP response from tick N-1     (~5 μs channel recv)
    //       ├─ mark_sent(N-1) + publish(N-1)
    //       ├─ SNAPSHOT staging for tick N
    //       ├─ SUBMIT command N                 (~5 μs channel send)
    //       └─ SLEEP until tick N+1 deadline
    //              └── I/O helper thread does write_bulk+read_bulk ──┘
    //
    //   The USB round-trip happens DURING the sleep. The scheduler
    //   thread CPU footprint per tick is ~10-15 μs instead of ~350.
    // =================================================================

    fn run_pipelined(&self, transport: &mut dyn PipelinedTransport) -> Result<()> {
        let period_ns = self.config.tick_period().as_nanos() as i64;
        let start_ns = now_monotonic_ns();
        info!(
            "RtScheduler running (pipelined): rate={} Hz, period={} ns",
            self.config.rate_hz, period_ns
        );

        let mut tick_index: u64 = 0;
        let mut cmd_seq: u64 = 0;

        // State for the 1-tick-behind reap pattern. `None` means
        // no transfer is in flight (true at the very first tick and
        // after a submit failure).
        struct InFlight {
            write_gen: u64,
            tick_submitted_ns: i64,
            tick_expected_ns: i64,
            cmd_seq: u64,
            tick_index: u64,
            missed_prior: u32,
            jitter_us: i32,
        }
        let mut in_flight: Option<InFlight> = None;

        loop {
            if self.stop.load(Ordering::Acquire) {
                // Drain last in-flight transfer so mark_sent fires
                // and BlockUntilSent waiters unblock.
                if in_flight.is_some() {
                    match transport.drain(self.config.transport_timeout) {
                        Ok(Some(status)) => {
                            let ifr = in_flight.take().unwrap();
                            self.staging.mark_sent(ifr.write_gen);
                            let tick_recv_ns = now_monotonic_ns();
                            let latency_us =
                                clamp_to_u32((tick_recv_ns - ifr.tick_submitted_ns) / 1_000);
                            self.stats.record_ok(latency_us, ifr.jitter_us);
                            let wire_seq = status.seq_num;
                            self.bus.publish(StatusFrame {
                                status,
                                cmd_seq: ifr.cmd_seq,
                                wire_seq,
                                tick_index: ifr.tick_index,
                                tick_expected_ns: ifr.tick_expected_ns,
                                tick_sent_ns: ifr.tick_submitted_ns,
                                tick_recv_ns,
                                latency_us,
                                jitter_us: ifr.jitter_us,
                                missed_ticks_prior: ifr.missed_prior,
                            });
                        }
                        _ => {
                            let ifr = in_flight.take().unwrap();
                            self.staging.mark_sent(ifr.write_gen);
                        }
                    }
                }
                info!("RtScheduler stop (pipelined): {} ticks", tick_index);
                return Ok(());
            }

            // -- sleep until the tick deadline ---------------------
            let tick_expected_ns = start_ns + (tick_index as i64) * period_ns;
            if let Err(e) = sleep_until_abs_monotonic(tick_expected_ns) {
                error!("clock_nanosleep: {e}");
                return Err(e.into());
            }

            let tick_wakeup_ns = now_monotonic_ns();
            let jitter_ns = tick_wakeup_ns - tick_expected_ns;
            let jitter_us = clamp_to_i32(jitter_ns / 1_000);

            // catch-up
            let missed_prior = if jitter_ns > period_ns {
                let n = (jitter_ns / period_ns) as u64;
                self.stats.record_missed(n as u32);
                n as u32
            } else {
                0
            };
            tick_index = tick_index + 1 + missed_prior as u64;

            // -- REAP the previous tick's response (if any) -------
            if let Some(ifr) = in_flight.take() {
                match transport.reap(self.config.transport_timeout) {
                    Ok(status) => {
                        let tick_recv_ns = now_monotonic_ns();
                        self.staging.mark_sent(ifr.write_gen);
                        let latency_us =
                            clamp_to_u32((tick_recv_ns - ifr.tick_submitted_ns) / 1_000);
                        self.stats.record_ok(latency_us, ifr.jitter_us);
                        let wire_seq = status.seq_num;
                        self.bus.publish(StatusFrame {
                            status,
                            cmd_seq: ifr.cmd_seq,
                            wire_seq,
                            tick_index: ifr.tick_index,
                            tick_expected_ns: ifr.tick_expected_ns,
                            tick_sent_ns: ifr.tick_submitted_ns,
                            tick_recv_ns,
                            latency_us,
                            jitter_us: ifr.jitter_us,
                            missed_ticks_prior: ifr.missed_prior,
                        });
                    }
                    Err(e) => {
                        trace!("reap failed: {e}");
                        self.staging.mark_sent(ifr.write_gen);
                        self.stats.record_transport_error();
                        self.stats.record_missed(1);
                    }
                }
            }

            // -- SNAPSHOT + SUBMIT for this tick ------------------
            let (mut cmd, write_gen) = self.staging.take_snapshot();
            cmd.seq_num = (cmd_seq & 0xFF) as u8;

            match transport.submit(&cmd) {
                Ok(()) => {
                    in_flight = Some(InFlight {
                        write_gen,
                        tick_submitted_ns: now_monotonic_ns(),
                        tick_expected_ns,
                        cmd_seq,
                        tick_index,
                        missed_prior,
                        jitter_us,
                    });
                }
                Err(e) => {
                    error!("submit failed: {e}");
                    self.staging.mark_sent(write_gen);
                    self.stats.record_transport_error();
                    // in_flight stays None — next tick will skip reap
                }
            }

            cmd_seq = cmd_seq.wrapping_add(1);
        }
    }
}

// -------------------------------------------------------------------------
//   libc wall-clock helpers
// -------------------------------------------------------------------------

#[inline]
fn now_monotonic_ns() -> i64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `clock_gettime(CLOCK_MONOTONIC, ...)` is always safe
    // with a valid `timespec` pointer.
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64
}

/// `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, target_ns)`.
///
/// Sleeps until the absolute monotonic timestamp `target_ns`. If
/// the timestamp is in the past, returns immediately. Returns an
/// `io::Error` with the POSIX errno on any other failure.
#[cfg(target_os = "linux")]
fn sleep_until_abs_monotonic(target_ns: i64) -> std::io::Result<()> {
    let ts =
        libc::timespec { tv_sec: target_ns / 1_000_000_000, tv_nsec: target_ns % 1_000_000_000 };
    // SAFETY: `&ts` is a valid pointer to an initialised `timespec`.
    // clock_nanosleep is a thread-safe libc call.
    let rc = unsafe {
        libc::clock_nanosleep(libc::CLOCK_MONOTONIC, libc::TIMER_ABSTIME, &ts, std::ptr::null_mut())
    };
    if rc == 0 {
        Ok(())
    } else {
        // clock_nanosleep returns the errno directly; it does NOT
        // set the global errno.
        Err(std::io::Error::from_raw_os_error(rc))
    }
}

/// Non-Linux fallback for the test suite running on developer
/// machines (macOS does not have `clock_nanosleep` in libc prior to
/// 10.12 and its semantics around `TIMER_ABSTIME` are quirky).
///
/// Busy-spins on `clock_gettime` which is correct for short sleeps
/// but wasteful; since production is Linux-only, this only exists
/// to let `cargo test` run locally.
#[cfg(not(target_os = "linux"))]
fn sleep_until_abs_monotonic(target_ns: i64) -> std::io::Result<()> {
    loop {
        let now = now_monotonic_ns();
        if now >= target_ns {
            return Ok(());
        }
        let delta = target_ns - now;
        if delta > 1_000_000 {
            // More than 1 ms to go: coarse sleep for delta - 500 μs
            std::thread::sleep(std::time::Duration::from_nanos((delta - 500_000) as u64));
        } else {
            // Final approach: yield to the scheduler to keep from
            // pinning the core at 100% in debug mode.
            std::thread::yield_now();
        }
    }
}

#[inline]
fn clamp_to_u32(v: i64) -> u32 {
    if v < 0 {
        0
    } else if v > u32::MAX as i64 {
        u32::MAX
    } else {
        v as u32
    }
}

#[inline]
fn clamp_to_i32(v: i64) -> i32 {
    if v < i32::MIN as i64 {
        i32::MIN
    } else if v > i32::MAX as i64 {
        i32::MAX
    } else {
        v as i32
    }
}

// =========================================================================
//   Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RtConfig;
    use crate::staging::{CommandStaging, WriteMode};
    use crate::stats::RtStats;
    use crate::status_bus::StatusBus;
    use crate::transport::mock::{MockState, MockTransport};
    use parking_lot::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};
    use tokio::sync::broadcast::error::TryRecvError;

    /// Build a scheduler with a MockTransport and the given mock
    /// latency. Returns the scheduler, its staging/stats/bus, and
    /// a handle on the mock state for later inspection.
    fn build_sched(
        rate_hz: u32,
        mock_latency_us: u64,
    ) -> (
        RtScheduler,
        Arc<CommandStaging>,
        Arc<StatusBus>,
        Arc<RtStats>,
        Arc<Mutex<MockState>>,
    ) {
        let mock_state = Arc::new(Mutex::new(MockState {
            latency: Duration::from_micros(mock_latency_us),
            fail_every_nth: None,
            ..Default::default()
        }));
        let transport = Box::new(MockTransport::with_state(Arc::clone(&mock_state)));
        let staging = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        let bus = Arc::new(StatusBus::new(1024));
        let stats = Arc::new(RtStats::new());

        let config = RtConfig {
            rate_hz,
            default_write_mode: WriteMode::BlockUntilSent,
            enable_rt: false, // no sudo in tests
            lock_memory: false,
            dma_latency_us: None,
            cpu_affinity: None,
            ..Default::default()
        };

        let sched = RtScheduler::new(
            config,
            Arc::clone(&staging),
            Arc::clone(&bus),
            Arc::clone(&stats),
            transport,
        );

        (sched, staging, bus, stats, mock_state)
    }

    #[test]
    fn starts_and_stops_cleanly() {
        let (sched, _staging, _bus, stats, _mock) = build_sched(1_000, 100);
        let stop = sched.stop_handle();
        let handle = thread::spawn(move || sched.run());

        // Let it spin for a bit, then stop.
        thread::sleep(Duration::from_millis(50));
        stop.stop();
        let result = handle.join().unwrap();
        assert!(result.is_ok());

        let snap = stats.snapshot();
        assert!(snap.tick_ok > 10, "expected ≥10 ticks in 50ms at 1kHz, got {}", snap.tick_ok);
    }

    #[test]
    fn hits_target_rate_with_mock() {
        // Target 1 kHz, mock latency 100 us. In 200 ms expect
        // roughly 200 ticks. Allow a generous ±20% tolerance to
        // absorb cargo-test environmental jitter: when `cargo
        // test` runs the full suite in parallel, CPU-hungry
        // neighbour tests can easily cost us a couple of ticks.
        // The point of this test is to verify the loop is ticking
        // at approximately the requested rate, not to calibrate
        // real-time performance on a loaded build host.
        let (sched, _staging, _bus, stats, _mock) = build_sched(1_000, 100);
        let stop = sched.stop_handle();
        let t0 = Instant::now();
        let h = thread::spawn(move || sched.run());

        thread::sleep(Duration::from_millis(200));
        stop.stop();
        h.join().unwrap().unwrap();
        let elapsed = t0.elapsed().as_secs_f64();

        let snap = stats.snapshot();
        let eff = snap.effective_hz(elapsed);
        assert!(
            eff > 800.0 && eff < 1200.0,
            "effective rate {:.1} Hz outside tolerance window",
            eff
        );
        // Under ideal conditions missed_ticks == 0, but under
        // `cargo test` parallel load we'll see a handful of slips.
        // Anything above ~10% of the total is a real bug in the
        // catch-up logic; below that is just environmental noise.
        assert!(
            snap.missed_ticks < snap.tick_count / 10,
            "missed {} ticks out of {} (>10%); catch-up logic broken?",
            snap.missed_ticks,
            snap.tick_count
        );
    }

    #[test]
    fn publishes_status_frames_in_order() {
        let (sched, _staging, bus, _stats, _mock) = build_sched(2_000, 50);
        let mut rx = bus.subscribe();
        let stop = sched.stop_handle();
        let h = thread::spawn(move || sched.run());

        thread::sleep(Duration::from_millis(100));
        stop.stop();
        h.join().unwrap().unwrap();

        // Drain the receiver. cmd_seq must be strictly increasing
        // starting at 0.
        let mut prev: i64 = -1;
        let mut count = 0usize;
        loop {
            match rx.try_recv() {
                Ok(frame) => {
                    assert!(
                        (frame.cmd_seq as i64) > prev,
                        "frames out of order: prev={} got={}",
                        prev,
                        frame.cmd_seq
                    );
                    prev = frame.cmd_seq as i64;
                    count += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => break,
            }
        }
        assert!(count > 100, "expected ≥100 frames, got {count}");
    }

    #[test]
    fn slow_transport_records_missed_ticks() {
        // Mock latency 500 us, tick period 100 us (10 kHz) → the
        // loop cannot possibly keep up and every iteration slips
        // by ~4 periods. Verify missed_ticks > 0.
        let (sched, _staging, _bus, stats, _mock) = build_sched(10_000, 500);
        let stop = sched.stop_handle();
        let h = thread::spawn(move || sched.run());

        thread::sleep(Duration::from_millis(100));
        stop.stop();
        h.join().unwrap().unwrap();

        let snap = stats.snapshot();
        assert!(snap.missed_ticks > 0, "expected missed ticks, got {}", snap.missed_ticks);
        // The number of OK ticks should be bounded by
        // elapsed / mock_latency ≈ 100ms / 500us = 200 at most.
        assert!(snap.tick_ok <= 250);
    }

    #[test]
    fn transport_error_is_recorded_but_loop_continues() {
        // fail_every_nth = 5 → 20% error rate.
        let mock_state = Arc::new(Mutex::new(MockState {
            latency: Duration::from_micros(100),
            fail_every_nth: Some(5),
            ..Default::default()
        }));
        let transport = Box::new(MockTransport::with_state(Arc::clone(&mock_state)));
        let staging = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        let bus = Arc::new(StatusBus::new(1024));
        let stats = Arc::new(RtStats::new());

        let config = RtConfig {
            rate_hz: 1_000,
            enable_rt: false,
            lock_memory: false,
            dma_latency_us: None,
            cpu_affinity: None,
            ..Default::default()
        };

        let sched = RtScheduler::new(
            config,
            Arc::clone(&staging),
            Arc::clone(&bus),
            Arc::clone(&stats),
            transport,
        );
        let stop = sched.stop_handle();
        let h = thread::spawn(move || sched.run());

        thread::sleep(Duration::from_millis(200));
        stop.stop();
        h.join().unwrap().unwrap();

        let snap = stats.snapshot();
        assert!(snap.transport_errors > 0);
        assert!(snap.tick_ok > 0);
        // Error count should be roughly 1/5 of successful ticks.
        let ratio = snap.transport_errors as f64 / snap.tick_ok as f64;
        assert!(
            ratio > 0.15 && ratio < 0.35,
            "error ratio {} outside expected 20% ± 5% window",
            ratio
        );
    }

    #[test]
    fn block_until_sent_writer_is_released_by_scheduler() {
        // A writer in BlockUntilSent mode that marks DAC0 dirty
        // should be unblocked within one tick period once the
        // scheduler starts running. This verifies the staging
        // buffer <-> scheduler mark_sent handshake end-to-end.
        let (sched, staging, _bus, _stats, _mock) = build_sched(1_000, 100);
        staging.set_dac0(100).unwrap();
        // Dirty bit is set. Now launch the scheduler and a second
        // writer that should block initially.
        let stop = sched.stop_handle();
        let staging2 = Arc::clone(&staging);

        let writer = thread::spawn(move || {
            let t0 = Instant::now();
            staging2.set_dac0(200).unwrap();
            t0.elapsed()
        });

        let sched_handle = thread::spawn(move || sched.run());

        // The writer should unblock within a few ms at 1 kHz.
        let writer_elapsed = writer.join().unwrap();
        stop.stop();
        sched_handle.join().unwrap().unwrap();

        assert!(
            writer_elapsed < Duration::from_millis(50),
            "writer was not released in time: {}ms",
            writer_elapsed.as_millis()
        );
        let snap = staging.peek_frame();
        // The final value could be either 100 or 200 depending on
        // tick timing; what matters is the writer completed.
        assert!(snap.dac[0] == 100 || snap.dac[0] == 200);
    }

    #[test]
    fn catchup_skips_rather_than_storms() {
        // Latency = 5 ms, tick period = 1 ms (1 kHz). The loop
        // falls behind instantly. Verify:
        //   - tick_count advances (slots accounted for)
        //   - missed_ticks > 0
        //   - the scheduler does NOT fire back-to-back to "catch up"
        //     (which would storm the transport and pile exchanges)
        let (sched, _staging, _bus, stats, mock_state) = build_sched(1_000, 5_000);
        let stop = sched.stop_handle();
        let h = thread::spawn(move || sched.run());

        thread::sleep(Duration::from_millis(100));
        stop.stop();
        h.join().unwrap().unwrap();

        let snap = stats.snapshot();
        let mock = mock_state.lock();
        // Roughly 20 exchanges in 100 ms at 5 ms latency each.
        assert!(
            mock.call_count < 30,
            "too many exchanges — scheduler is storming the transport ({})",
            mock.call_count
        );
        assert!(snap.missed_ticks > 0, "expected missed ticks under overload");
    }

    // =================================================================
    //   Pipelined scheduler tests
    // =================================================================

    fn build_pipelined_sched(
        rate_hz: u32,
        mock_latency_us: u64,
    ) -> (
        RtScheduler,
        Arc<CommandStaging>,
        Arc<StatusBus>,
        Arc<RtStats>,
        Arc<Mutex<MockState>>,
    ) {
        use crate::transport::mock::MockPipelinedTransport;
        let mock_state = Arc::new(Mutex::new(MockState {
            latency: Duration::from_micros(mock_latency_us),
            fail_every_nth: None,
            ..Default::default()
        }));
        let transport = Box::new(MockPipelinedTransport::with_state(Arc::clone(&mock_state)));
        let staging = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        let bus = Arc::new(StatusBus::new(1024));
        let stats = Arc::new(RtStats::new());

        let config = RtConfig {
            rate_hz,
            default_write_mode: WriteMode::BlockUntilSent,
            enable_rt: false,
            lock_memory: false,
            dma_latency_us: None,
            cpu_affinity: None,
            ..Default::default()
        };

        let sched = RtScheduler::new_pipelined(
            config,
            Arc::clone(&staging),
            Arc::clone(&bus),
            Arc::clone(&stats),
            transport,
        );

        (sched, staging, bus, stats, mock_state)
    }

    #[test]
    fn pipelined_starts_and_stops_cleanly() {
        let (sched, _staging, _bus, stats, _mock) = build_pipelined_sched(1_000, 100);
        let stop = sched.stop_handle();
        let h = thread::spawn(move || sched.run());
        thread::sleep(Duration::from_millis(50));
        stop.stop();
        h.join().unwrap().unwrap();

        let snap = stats.snapshot();
        assert!(snap.tick_ok > 10);
    }

    // Wall-clock rate assertion — 5 kHz ± 20% over 300 ms. Flaky on
    // overcommitted CI runners where no PREEMPT-RT scheduler is
    // available and the VM itself gets preempted. Kept runnable
    // locally via `cargo test -- --ignored` for manual verification.
    #[test]
    #[ignore = "timing-sensitive; needs an RT-capable host to be reliable"]
    fn pipelined_hits_target_rate() {
        // 5 kHz with 100 us mock latency. The pipelined loop should
        // overlap I/O with sleep and hit rate even though
        // latency (100 us) < period (200 us) with little margin.
        // The sync loop CANNOT hit 5 kHz with 100 us latency on
        // an Atom because the ~100 us CPU overhead pushes total
        // beyond 200 us. Pipelined can because CPU work is ~15 us.
        let (sched, _staging, _bus, stats, _mock) = build_pipelined_sched(5_000, 100);
        let stop = sched.stop_handle();
        let t0 = Instant::now();
        let h = thread::spawn(move || sched.run());
        thread::sleep(Duration::from_millis(300));
        stop.stop();
        h.join().unwrap().unwrap();
        let elapsed = t0.elapsed().as_secs_f64();

        let snap = stats.snapshot();
        let eff = snap.effective_hz(elapsed);
        assert!(
            eff > 4000.0 && eff < 6000.0,
            "pipelined effective rate {eff:.1} Hz outside ±20% of 5 kHz"
        );
        // Missed should be well under 10% — pipelined has headroom
        assert!(
            snap.missed_ticks < snap.tick_count / 10,
            "pipelined missed too many: {} / {}",
            snap.missed_ticks,
            snap.tick_count
        );
    }

    #[test]
    fn pipelined_publishes_ordered_frames() {
        let (sched, _staging, bus, _stats, _mock) = build_pipelined_sched(2_000, 50);
        let mut rx = bus.subscribe();
        let stop = sched.stop_handle();
        let h = thread::spawn(move || sched.run());
        thread::sleep(Duration::from_millis(100));
        stop.stop();
        h.join().unwrap().unwrap();

        let mut prev: i64 = -1;
        let mut count = 0usize;
        loop {
            match rx.try_recv() {
                Ok(f) => {
                    assert!((f.cmd_seq as i64) > prev);
                    prev = f.cmd_seq as i64;
                    count += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => break,
            }
        }
        // With 1-tick pipeline delay, we get N-1 published frames
        // for N ticks. 100 ms at 2 kHz → ~200 ticks → ~199 frames.
        assert!(count > 100, "expected ≥100 frames, got {count}");
    }

    #[test]
    fn pipelined_mark_sent_releases_waiter() {
        let (sched, staging, _bus, _stats, _mock) = build_pipelined_sched(1_000, 100);
        staging.set_dac0(100).unwrap();

        let staging2 = Arc::clone(&staging);
        let writer = thread::spawn(move || {
            let t0 = Instant::now();
            staging2.set_dac0(200).unwrap();
            t0.elapsed()
        });

        let stop = sched.stop_handle();
        let sched_h = thread::spawn(move || sched.run());

        // In pipelined mode, mark_sent happens at reap time (tick+1).
        // At 1 kHz that's ~2 ms after submit (1 ms sleep + reap).
        let elapsed = writer.join().unwrap();
        stop.stop();
        sched_h.join().unwrap().unwrap();

        assert!(
            elapsed < Duration::from_millis(100),
            "writer blocked too long in pipelined mode: {}ms",
            elapsed.as_millis()
        );
    }
}
