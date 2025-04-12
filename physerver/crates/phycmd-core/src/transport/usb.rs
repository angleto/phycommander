use super::traits::{Transport, TransportStats};
use crate::protocol::{self, Command, Status, MESSAGE_SIZE};
use anyhow::{Context, Result};
use rusb::{Device, DeviceHandle, GlobalContext};
use std::time::Duration;
use tracing::{debug, error, info, warn};

// Arduino Due USB VID/PID
const VENDOR_ID: u16 = 0x2341;  // Arduino
const PRODUCT_ID: u16 = 0x003e; // Arduino Due (Programming Port)

// USB endpoints — must match the firmware descriptor layout in
// ATSAM3X8E_FW/src/udi_vendor.h. SAM3X UOTGHS hardware endpoints
// are uni-directional, so IN and OUT use different endpoint numbers:
//   EP 1 IN  (0x81) — device → host (status frames)
//   EP 2 OUT (0x02) — host → device (command frames)
const EP_OUT: u8 = 0x02; // Bulk OUT endpoint
const EP_IN: u8 = 0x81;  // Bulk IN endpoint

// USB configuration
const INTERFACE_NUM: u8 = 0;
const TIMEOUT_MS: u64 = 100;

/// Direct USB bulk transfer transport
///
/// Provides lowest latency and highest throughput communication with Arduino Due.
/// Uses bulk endpoints for bidirectional data transfer.
///
/// Performance characteristics:
/// - Latency: 50-200µs (vs 500-1000µs for serial)
/// - Max rate: 10-20 kHz (vs 1 kHz for serial)
/// - Throughput: ~8 Mbps sustained (vs ~900 kbps for serial)
#[derive(Debug)]
pub struct UsbTransport {
    device_handle: DeviceHandle<GlobalContext>,
    stats: TransportStats,
    read_buffer: [u8; MESSAGE_SIZE],
    device_info: String,
}

impl UsbTransport {
    /// Create a new USB transport by finding and opening the Arduino Due
    pub fn new() -> Result<Self> {
        info!("Initializing direct USB transport");

        let (device, device_handle) = Self::find_and_open_device()?;

        // Get device info
        let device_desc = device
            .device_descriptor()
            .context("Failed to get device descriptor")?;

        let device_info = format!(
            "USB {:04x}:{:04x}",
            device_desc.vendor_id(),
            device_desc.product_id()
        );

        info!("Opened device: {}", device_info);

        // Claim interface
        device_handle
            .claim_interface(INTERFACE_NUM)
            .context("Failed to claim USB interface")?;

        info!("USB interface claimed successfully");

        Ok(Self {
            device_handle,
            stats: TransportStats::default(),
            read_buffer: [0u8; MESSAGE_SIZE],
            device_info,
        })
    }

    /// Find and open the Arduino Due device
    fn find_and_open_device() -> Result<(Device<GlobalContext>, DeviceHandle<GlobalContext>)> {
        let devices = rusb::devices().context("Failed to enumerate USB devices")?;

        for device in devices.iter() {
            let device_desc = match device.device_descriptor() {
                Ok(desc) => desc,
                Err(_) => continue,
            };

            if device_desc.vendor_id() == VENDOR_ID && device_desc.product_id() == PRODUCT_ID {
                info!(
                    "Found Arduino Due: VID={:04x} PID={:04x}",
                    device_desc.vendor_id(),
                    device_desc.product_id()
                );

                let handle = device
                    .open()
                    .context("Failed to open USB device")?;

                return Ok((device, handle));
            }
        }

        anyhow::bail!(
            "Arduino Due not found (VID:{:04x} PID:{:04x}). Is it connected?",
            VENDOR_ID,
            PRODUCT_ID
        )
    }

    /// Write bulk data to device
    fn bulk_write(&mut self, data: &[u8]) -> Result<()> {
        let start = std::time::Instant::now();

        let written = self
            .device_handle
            .write_bulk(EP_OUT, data, Duration::from_millis(TIMEOUT_MS))
            .context("USB bulk write failed")?;

        if written != data.len() {
            anyhow::bail!(
                "Incomplete USB write: {} bytes of {} sent",
                written,
                data.len()
            );
        }

        let elapsed = start.elapsed().as_micros() as u64;
        self.stats.avg_latency_us = (self.stats.avg_latency_us + elapsed) / 2;

        debug!("USB bulk write: {} bytes in {}µs", written, elapsed);

        Ok(())
    }

    /// Read bulk data from device
    fn bulk_read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let start = std::time::Instant::now();

        let read = self
            .device_handle
            .read_bulk(EP_IN, buffer, Duration::from_millis(TIMEOUT_MS))
            .context("USB bulk read failed")?;

        let elapsed = start.elapsed().as_micros() as u64;
        self.stats.avg_latency_us = (self.stats.avg_latency_us + elapsed) / 2;

        debug!("USB bulk read: {} bytes in {}µs", read, elapsed);

        Ok(read)
    }
}

impl Transport for UsbTransport {
    fn send_command(&mut self, cmd: &Command) -> Result<()> {
        let bytes = protocol::encode_command(cmd);

        self.bulk_write(&bytes)?;

        self.stats.messages_sent += 1;
        self.stats.bytes_sent += MESSAGE_SIZE as u64;

        debug!("Sent command via USB: seq={}", cmd.seq_num);

        Ok(())
    }

    fn receive_status(&mut self) -> Result<Status> {
        // Read exactly MESSAGE_SIZE bytes
        let mut total_read = 0;

        while total_read < MESSAGE_SIZE {
            let start = std::time::Instant::now();

            let read = self
                .device_handle
                .read_bulk(
                    EP_IN,
                    &mut self.read_buffer[total_read..],
                    Duration::from_millis(TIMEOUT_MS)
                )
                .context("USB bulk read failed")?;

            let elapsed = start.elapsed().as_micros() as u64;
            self.stats.avg_latency_us = (self.stats.avg_latency_us + elapsed) / 2;

            if read == 0 {
                anyhow::bail!("USB read returned 0 bytes (disconnected?)");
            }

            total_read += read;
        }

        // Decode the status
        match protocol::decode_status(&self.read_buffer) {
            Ok(status) => {
                self.stats.messages_received += 1;
                self.stats.bytes_received += MESSAGE_SIZE as u64;

                debug!(
                    "Received status via USB: seq={}, loop_time={}µs",
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

    /// Optimised single-call exchange: write OUT + read IN with no
    /// intermediate timing, no debug logging, no read loop. The
    /// PhyCMD-64 protocol always produces exactly 64 bytes per
    /// direction, so a single bulk transfer per direction suffices.
    fn exchange(&mut self, cmd: &Command) -> Result<Status> {
        let bytes = protocol::encode_command(cmd);

        let written = self
            .device_handle
            .write_bulk(EP_OUT, &bytes, Duration::from_millis(TIMEOUT_MS))
            .context("USB bulk write")?;
        if written != MESSAGE_SIZE {
            anyhow::bail!("short write: {written}/{MESSAGE_SIZE}");
        }

        let read = self
            .device_handle
            .read_bulk(EP_IN, &mut self.read_buffer, Duration::from_millis(TIMEOUT_MS))
            .context("USB bulk read")?;
        if read != MESSAGE_SIZE {
            anyhow::bail!("short read: {read}/{MESSAGE_SIZE}");
        }

        let status = protocol::decode_status(&self.read_buffer)
            .map_err(|e| {
                self.stats.crc_errors += 1;
                self.stats.errors += 1;
                anyhow::anyhow!("{e}")
            })?;

        self.stats.messages_sent += 1;
        self.stats.messages_received += 1;
        self.stats.bytes_sent += MESSAGE_SIZE as u64;
        self.stats.bytes_received += MESSAGE_SIZE as u64;

        Ok(status)
    }

    fn stats(&self) -> &TransportStats {
        &self.stats
    }

    fn reset_stats(&mut self) {
        self.stats = TransportStats::default();
    }

    fn name(&self) -> &str {
        "USB (Direct Bulk)"
    }

    fn device_id(&self) -> String {
        self.device_info.clone()
    }

    fn is_connected(&self) -> bool {
        // Try a simple operation to check if device is still connected
        self.device_handle.active_configuration().is_ok()
    }

    fn set_timeout(&mut self, _timeout: Duration) -> Result<()> {
        // Timeout is handled per-transfer in rusb
        Ok(())
    }

    fn max_rate(&self) -> u32 {
        // USB Full Speed bulk: theoretical ~10-20 kHz
        // Practical with overhead: ~5-10 kHz
        10000
    }

    fn typical_latency_us(&self) -> u32 {
        // Direct USB bulk transfer latency: 50-200µs
        100
    }
}

impl Drop for UsbTransport {
    fn drop(&mut self) {
        // Release interface
        if let Err(e) = self.device_handle.release_interface(INTERFACE_NUM) {
            warn!("Failed to release USB interface: {}", e);
        }

        info!(
            "Closing USB device. Stats: sent={}, received={}, crc_errors={}, avg_latency={}µs",
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
    fn test_usb_device_search() {
        // This test just ensures enumeration doesn't panic
        let devices = rusb::devices();
        assert!(devices.is_ok());

        if let Ok(devices) = devices {
            println!("Found {} USB devices", devices.len());
        }
    }
}
