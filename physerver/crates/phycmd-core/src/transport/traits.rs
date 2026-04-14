use crate::protocol::{Command, Status};
use anyhow::Result;
use std::fmt::Debug;
use std::time::Duration;

/// Statistics for transport layer
#[derive(Debug, Clone, Default)]
pub struct TransportStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
    pub crc_errors: u64,
    pub timeout_errors: u64,
    pub avg_latency_us: u64,
}

/// Transport abstraction for communication with device
///
/// Implementations:
/// - SerialTransport: USB CDC (virtual serial port)
/// - UsbTransport: Direct USB bulk transfers
pub trait Transport: Send + Debug {
    /// Send a command to the device
    fn send_command(&mut self, cmd: &Command) -> Result<()>;

    /// Receive a status message from the device
    fn receive_status(&mut self) -> Result<Status>;

    /// Send command and receive status (optimized path)
    fn exchange(&mut self, cmd: &Command) -> Result<Status> {
        self.send_command(cmd)?;
        self.receive_status()
    }

    /// Get transport statistics
    fn stats(&self) -> &TransportStats;

    /// Reset statistics
    fn reset_stats(&mut self);

    /// Get transport name/type
    fn name(&self) -> &str;

    /// Get device identifier
    fn device_id(&self) -> String;

    /// Check if transport is connected
    fn is_connected(&self) -> bool;

    /// Set read/write timeout
    fn set_timeout(&mut self, timeout: std::time::Duration) -> Result<()>;

    /// Get maximum achievable rate (Hz)
    fn max_rate(&self) -> u32 {
        // Conservative default
        1000
    }

    /// Get typical latency (microseconds)
    fn typical_latency_us(&self) -> u32 {
        // Conservative default
        1000
    }
}

// =========================================================================
//   Pipelined transport — two-phase submit/reap
// =========================================================================

/// A transport that supports non-blocking submit + blocking reap for
/// pipelined operation.
///
/// The RT scheduler submits a command at the *start* of a tick, then
/// sleeps with `clock_nanosleep(TIMER_ABSTIME)`. The USB round-trip
/// happens while the scheduler sleeps (on a dedicated I/O helper
/// thread). At the *start of the next tick*, the scheduler reaps the
/// response, processes it, and submits the command for the new tick.
///
/// The contract is **strictly depth-1**: at most one transfer is in
/// flight at any time. `submit(N)` → `reap() → Ok(response_N)` →
/// `submit(N+1)` → ... . This preserves the 1:1 tick-to-frame
/// invariant required by `CommandStaging::mark_sent()`.
pub trait PipelinedTransport: Send + Debug {
    /// Submit a command for asynchronous transmission.
    ///
    /// The command bytes are copied into the transport's internal
    /// send queue and the call returns immediately. The actual USB
    /// I/O is performed by a helper thread.
    ///
    /// # Errors
    ///
    /// Returns `Err` only if the I/O helper thread has died or the
    /// internal channel is full (indicates a bug — depth-1 pipeline
    /// must reap before the next submit).
    fn submit(&mut self, cmd: &Command) -> Result<()>;

    /// Block until the response to the most recently submitted
    /// command is available, or until `timeout` elapses.
    ///
    /// Returns `Ok(status)` on success. The returned `Status`
    /// corresponds to the command from the most recent `submit()`.
    ///
    /// Returns `Err` on:
    ///   - timeout (the USB exchange did not complete in time)
    ///   - transport error (CRC mismatch, short read, device lost)
    ///   - helper thread panic
    fn reap(&mut self, timeout: Duration) -> Result<Status>;

    /// Drain any in-flight transfer on shutdown. Called once when the
    /// scheduler loop exits to ensure the last frame is properly
    /// reaped (or timed out) and `mark_sent` can be called.
    fn drain(&mut self, timeout: Duration) -> Result<Option<Status>>;

    /// Transport statistics.
    fn stats(&self) -> &TransportStats;
    /// Reset statistics counters.
    fn reset_stats(&mut self);
    /// Human-readable transport name.
    fn name(&self) -> &str;
    /// Device identifier string.
    fn device_id(&self) -> String;
    /// `true` if the underlying device is still reachable.
    fn is_connected(&self) -> bool;
}
