//! Runtime configuration for the RT scheduler.
//!
//! This type intentionally covers only the knobs needed by
//! [`crate::scheduler::RtScheduler`] and friends. The service-level
//! configuration (HTTP port, IPC shared memory name, log level, ...)
//! lives in the `physervice` crate's own `config.toml` loader and
//! should not pollute the core library.

use std::time::Duration;

use thiserror::Error;

use crate::staging::WriteMode;

/// Complete RT scheduler configuration.
#[derive(Debug, Clone)]
pub struct RtConfig {
    /// Target tick rate in Hz. Bound 1..=20_000 on this hardware;
    /// values above 15 kHz will be accepted but logged as "likely
    /// to miss deadlines" (the empirical ceiling on EHCI + SAM3X
    /// UOTGHS is about 18 kHz average).
    pub rate_hz: u32,

    /// Default [`WriteMode`] applied by the staging buffer when the
    /// caller does not pass an explicit per-call override.
    pub default_write_mode: WriteMode,

    /// Per-subscriber capacity of the status bus broadcast channel.
    /// A slow subscriber that falls behind by more than this many
    /// frames will start seeing `Lagged` errors. 1024 = ~100 ms at
    /// 10 kHz.
    pub status_bus_capacity: usize,

    /// Number of USB bulk transfers kept in flight at once by the
    /// transport layer. Higher values give more throughput up to
    /// the EHCI/UOTGHS plateau but add tail latency. 8 is the
    /// measured sweet spot.
    pub pipeline_depth: usize,

    /// Enable the hard real-time optimisations in [`crate::rt_setup`]
    /// (SCHED_FIFO, mlockall, CPU affinity, DMA latency).
    pub enable_rt: bool,

    /// SCHED_FIFO priority for the scheduler thread (1..=99).
    /// 80 is a sensible default: above typical kernel threads and
    /// userspace daemons, below critical kernel work.
    pub rt_priority: i32,

    /// Lock all process memory via `mlockall` to avoid page faults
    /// on the hot path.
    pub lock_memory: bool,

    /// Optional CPU core to pin the scheduler thread to. Must NOT be
    /// a `nohz_full` isolated CPU for USB workloads, because
    /// isolated cores suppress softirq and libusb event dispatch.
    /// Leave `None` to let the kernel schedule it.
    pub cpu_affinity: Option<usize>,

    /// If `Some`, write the given latency to `/dev/cpu_dma_latency`
    /// to bound the C-state exit latency. `Some(0)` disables deep
    /// C-states entirely — maximum responsiveness, highest power.
    pub dma_latency_us: Option<i32>,

    /// Maximum time the scheduler will wait on a single transport
    /// exchange before recording a missed tick. Should be somewhat
    /// larger than the tick period so a single slow round-trip does
    /// not blow the whole loop.
    pub transport_timeout: Duration,
}

impl Default for RtConfig {
    fn default() -> Self {
        Self {
            rate_hz: 10_000,
            default_write_mode: WriteMode::BlockUntilSent,
            status_bus_capacity: 1024,
            pipeline_depth: 8,
            enable_rt: true,
            rt_priority: 80,
            lock_memory: true,
            cpu_affinity: None,
            dma_latency_us: Some(0),
            transport_timeout: Duration::from_millis(5),
        }
    }
}

/// Validation errors produced by [`RtConfig::validate`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RtConfigError {
    #[error("rate_hz must be between 1 and 20000, got {0}")]
    RateOutOfRange(u32),

    #[error("rt_priority must be between 1 and 99, got {0}")]
    PriorityOutOfRange(i32),

    #[error("pipeline_depth must be between 1 and 64, got {0}")]
    PipelineDepthOutOfRange(usize),

    #[error("status_bus_capacity must be at least 2, got {0}")]
    StatusBusCapacityTooSmall(usize),

    #[error("transport_timeout must be greater than 100µs, got {0:?}")]
    TransportTimeoutTooSmall(Duration),
}

impl RtConfig {
    /// Validate the configuration.
    ///
    /// Returns `Ok(())` if the config is within sane bounds or a
    /// descriptive error otherwise. Note: this does *not* check
    /// kernel capabilities (CAP_SYS_NICE for SCHED_FIFO, write
    /// access to /dev/cpu_dma_latency, etc.) — that is deferred to
    /// [`crate::rt_setup::apply_rt_optimizations`] at runtime.
    pub fn validate(&self) -> Result<(), RtConfigError> {
        if self.rate_hz == 0 || self.rate_hz > 20_000 {
            return Err(RtConfigError::RateOutOfRange(self.rate_hz));
        }
        if self.rt_priority < 1 || self.rt_priority > 99 {
            return Err(RtConfigError::PriorityOutOfRange(self.rt_priority));
        }
        if self.pipeline_depth < 1 || self.pipeline_depth > 64 {
            return Err(RtConfigError::PipelineDepthOutOfRange(self.pipeline_depth));
        }
        if self.status_bus_capacity < 2 {
            return Err(RtConfigError::StatusBusCapacityTooSmall(self.status_bus_capacity));
        }
        if self.transport_timeout < Duration::from_micros(100) {
            return Err(RtConfigError::TransportTimeoutTooSmall(self.transport_timeout));
        }
        Ok(())
    }

    /// The nominal tick period derived from `rate_hz`.
    pub fn tick_period(&self) -> Duration {
        Duration::from_nanos(1_000_000_000 / self.rate_hz as u64)
    }

    /// `true` if the configured rate is above the measured safe
    /// ceiling for this hardware (see Fase A investigation). A
    /// caller can use this to decide whether to emit a warning.
    pub fn is_aggressive_rate(&self) -> bool {
        self.rate_hz > 15_000
    }
}

// =========================================================================
//   Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let c = RtConfig::default();
        assert!(c.validate().is_ok());
        assert_eq!(c.rate_hz, 10_000);
        assert_eq!(c.default_write_mode, WriteMode::BlockUntilSent);
        assert_eq!(c.pipeline_depth, 8);
    }

    #[test]
    fn tick_period_matches_rate() {
        let c = RtConfig { rate_hz: 10_000, ..Default::default() };
        assert_eq!(c.tick_period(), Duration::from_micros(100));

        let c = RtConfig { rate_hz: 1_000, ..Default::default() };
        assert_eq!(c.tick_period(), Duration::from_millis(1));
    }

    #[test]
    fn rate_out_of_range_errors() {
        let mut c = RtConfig::default();

        c.rate_hz = 0;
        assert!(matches!(c.validate(), Err(RtConfigError::RateOutOfRange(0))));

        c.rate_hz = 30_000;
        assert!(matches!(c.validate(), Err(RtConfigError::RateOutOfRange(30_000))));
    }

    #[test]
    fn priority_out_of_range_errors() {
        let mut c = RtConfig::default();
        c.rt_priority = 0;
        assert!(matches!(c.validate(), Err(RtConfigError::PriorityOutOfRange(0))));
        c.rt_priority = 100;
        assert!(matches!(c.validate(), Err(RtConfigError::PriorityOutOfRange(100))));
    }

    #[test]
    fn aggressive_rate_flag() {
        let c = RtConfig { rate_hz: 10_000, ..Default::default() };
        assert!(!c.is_aggressive_rate());
        let c = RtConfig { rate_hz: 16_000, ..Default::default() };
        assert!(c.is_aggressive_rate());
    }

    #[test]
    fn pipeline_depth_bounds() {
        let mut c = RtConfig::default();
        c.pipeline_depth = 0;
        assert!(matches!(c.validate(), Err(RtConfigError::PipelineDepthOutOfRange(0))));
        c.pipeline_depth = 128;
        assert!(matches!(c.validate(), Err(RtConfigError::PipelineDepthOutOfRange(128))));
        c.pipeline_depth = 8;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn transport_timeout_lower_bound() {
        let c = RtConfig { transport_timeout: Duration::from_micros(50), ..Default::default() };
        assert!(matches!(c.validate(), Err(RtConfigError::TransportTimeoutTooSmall(_))));
    }
}
