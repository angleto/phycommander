use crate::protocol::{Command, Status};
use anyhow::Result;
use std::fmt::Debug;

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
