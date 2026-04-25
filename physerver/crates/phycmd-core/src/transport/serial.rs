use std::{
    io::{Read, Write},
    time::Duration,
};

use anyhow::{Context, Result};
use serialport::SerialPort;
use tracing::{debug, error, info, warn};

use super::traits::{Transport, TransportStats};
use crate::protocol::{self, Command, Status, MESSAGE_SIZE};

/// Serial port (USB CDC) transport
#[derive(Debug)]
pub struct SerialTransport {
    port: Box<dyn SerialPort>,
    read_buffer: [u8; MESSAGE_SIZE],
    stats: TransportStats,
    device_path: String,
}

impl SerialTransport {
    /// Open a serial port with optimal settings for real-time communication
    pub fn new(port_name: &str, baud_rate: u32) -> Result<Self> {
        info!("Opening serial port {} at {} baud", port_name, baud_rate);

        let port = serialport::new(port_name, baud_rate)
            .timeout(Duration::from_millis(100))
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .open()
            .context("Failed to open serial port")?;

        info!("Serial port opened successfully");

        Ok(Self {
            port,
            read_buffer: [0u8; MESSAGE_SIZE],
            stats: TransportStats::default(),
            device_path: port_name.to_string(),
        })
    }

    /// Get available serial ports
    pub fn available_devices() -> Result<Vec<String>> {
        let ports = serialport::available_ports().context("Failed to enumerate serial ports")?;

        Ok(ports.into_iter().map(|p| p.port_name).collect())
    }

    /// Auto-detect PhyCMD device
    pub fn find_device() -> Result<String> {
        let ports = Self::available_devices()?;

        for port_name in &ports {
            info!("Checking port: {}", port_name);

            // Try to open and ping
            if let Ok(mut handler) = Self::new(port_name, 921600) {
                // Send a simple command and see if we get a valid response
                let cmd = Command::default();
                if handler.exchange(&cmd).is_ok() {
                    info!("Found PhyCMD device on {}", port_name);
                    return Ok(port_name.clone());
                }
            }
        }

        anyhow::bail!("No PhyCMD device found on available ports: {:?}", ports)
    }
}

impl Transport for SerialTransport {
    fn send_command(&mut self, cmd: &Command) -> Result<()> {
        let start = std::time::Instant::now();
        let bytes = protocol::encode_command(cmd);

        self.port.write_all(&bytes).context("Failed to write command to serial port")?;

        self.port.flush().context("Failed to flush serial port")?;

        self.stats.messages_sent += 1;
        self.stats.bytes_sent += MESSAGE_SIZE as u64;

        let elapsed = start.elapsed().as_micros() as u64;
        self.stats.avg_latency_us = (self.stats.avg_latency_us + elapsed) / 2;

        debug!("Sent command: seq={}", cmd.seq_num);

        Ok(())
    }

    fn receive_status(&mut self) -> Result<Status> {
        let start = std::time::Instant::now();

        // Read exactly 64 bytes
        self.port
            .read_exact(&mut self.read_buffer)
            .context("Failed to read status from serial port")?;

        // Decode the status
        match protocol::decode_status(&self.read_buffer) {
            Ok(status) => {
                self.stats.messages_received += 1;
                self.stats.bytes_received += MESSAGE_SIZE as u64;

                let elapsed = start.elapsed().as_micros() as u64;
                self.stats.avg_latency_us = (self.stats.avg_latency_us + elapsed) / 2;

                debug!(
                    "Received status: seq={}, loop_time={}µs",
                    status.seq_num, status.loop_time_us
                );
                Ok(status)
            }
            Err(e) => {
                match &e {
                    protocol::ProtocolError::CrcMismatch { .. } => {
                        self.stats.crc_errors += 1;
                        self.stats.errors += 1;
                        warn!("CRC error in received status: {}", e);
                    }
                    _ => {
                        self.stats.errors += 1;
                        error!("Protocol error: {}", e);
                    }
                }
                Err(e.into())
            }
        }
    }

    fn stats(&self) -> &TransportStats {
        &self.stats
    }

    fn reset_stats(&mut self) {
        self.stats = TransportStats::default();
    }

    fn name(&self) -> &str {
        "Serial (USB CDC)"
    }

    fn device_id(&self) -> String {
        self.device_path.clone()
    }

    fn is_connected(&self) -> bool {
        // Try to get port name to check if still valid
        self.port.name().is_some()
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.port.set_timeout(timeout).context("Failed to set timeout")
    }

    fn max_rate(&self) -> u32 {
        // USB CDC theoretical: ~1.5 kHz
        // Practical with overhead: ~1 kHz
        1000
    }

    fn typical_latency_us(&self) -> u32 {
        // USB Full Speed latency: 500-1000µs
        750
    }
}

impl Drop for SerialTransport {
    fn drop(&mut self) {
        info!(
            "Closing serial port. Stats: sent={}, received={}, crc_errors={}, avg_latency={}µs",
            self.stats.messages_sent,
            self.stats.messages_received,
            self.stats.crc_errors,
            self.stats.avg_latency_us
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_devices() {
        let ports = SerialTransport::available_devices();
        // This test just ensures it doesn't panic
        println!("Available ports: {:?}", ports);
    }
}
