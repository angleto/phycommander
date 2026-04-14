use crate::protocol::{self, Command, Status, MESSAGE_SIZE};
use anyhow::{Context, Result};
use serialport::{SerialPort, SerialPortBuilder};
use std::io::{Read, Write};
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub struct SerialPortHandler {
    port: Box<dyn SerialPort>,
    read_buffer: [u8; MESSAGE_SIZE],
    stats: SerialStats,
}

#[derive(Debug, Clone, Default)]
pub struct SerialStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub crc_errors: u64,
    pub timeout_errors: u64,
    pub framing_errors: u64,
}

impl SerialPortHandler {
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

        Ok(Self { port, read_buffer: [0u8; MESSAGE_SIZE], stats: SerialStats::default() })
    }

    /// Send a command to the device
    pub fn send_command(&mut self, cmd: &Command) -> Result<()> {
        let bytes = protocol::encode_command(cmd);

        self.port.write_all(&bytes).context("Failed to write command to serial port")?;

        self.port.flush().context("Failed to flush serial port")?;

        self.stats.messages_sent += 1;
        debug!("Sent command: seq={}", cmd.seq_num);

        Ok(())
    }

    /// Receive a status message from the device
    pub fn receive_status(&mut self) -> Result<Status> {
        // Read exactly 64 bytes
        self.port
            .read_exact(&mut self.read_buffer)
            .context("Failed to read status from serial port")?;

        // Decode the status
        match protocol::decode_status(&self.read_buffer) {
            Ok(status) => {
                self.stats.messages_received += 1;
                debug!(
                    "Received status: seq={}, loop_time={}µs",
                    status.seq_num, status.loop_time_us
                );
                Ok(status)
            }
            Err(e) => {
                match e {
                    protocol::ProtocolError::CrcMismatch { .. } => {
                        self.stats.crc_errors += 1;
                        warn!("CRC error in received status: {}", e);
                    }
                    _ => {
                        self.stats.framing_errors += 1;
                        error!("Protocol error: {}", e);
                    }
                }
                Err(e.into())
            }
        }
    }

    /// Send command and receive status in one operation
    pub fn exchange(&mut self, cmd: &Command) -> Result<Status> {
        self.send_command(cmd)?;
        self.receive_status()
    }

    /// Get serial port statistics
    pub fn stats(&self) -> &SerialStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = SerialStats::default();
    }

    /// Set read timeout
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.port.set_timeout(timeout).context("Failed to set timeout")
    }

    /// Get available ports
    pub fn available_ports() -> Result<Vec<String>> {
        let ports = serialport::available_ports().context("Failed to enumerate serial ports")?;

        Ok(ports.into_iter().map(|p| p.port_name).collect())
    }

    /// Auto-detect PhyCMD device
    pub fn find_phycmd_device() -> Result<String> {
        let ports = Self::available_ports()?;

        for port_name in &ports {
            info!("Checking port: {}", port_name);

            // Try to open and ping
            if let Ok(mut handler) = Self::new(port_name, 921600) {
                // Send a simple command and see if we get a valid response
                let cmd = Command::default();
                if let Ok(_status) = handler.exchange(&cmd) {
                    info!("Found PhyCMD device on {}", port_name);
                    return Ok(port_name.clone());
                }
            }
        }

        anyhow::bail!("No PhyCMD device found on available ports: {:?}", ports)
    }
}

impl Drop for SerialPortHandler {
    fn drop(&mut self) {
        info!(
            "Closing serial port. Stats: sent={}, received={}, crc_errors={}, timeout_errors={}",
            self.stats.messages_sent,
            self.stats.messages_received,
            self.stats.crc_errors,
            self.stats.timeout_errors
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_ports() {
        let ports = SerialPortHandler::available_ports();
        // This test just ensures it doesn't panic
        println!("Available ports: {:?}", ports);
    }
}
