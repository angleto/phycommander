//! `UsbLoopbackTransport` — hardware loopback transport for the
//! PhyCommander Step 2 raw-echo firmware.
//!
//! The Step 2 firmware replies to every 64-byte OUT packet with a
//! bit-exact copy of the same bytes on the IN pipe — it does not
//! speak the PhyCMD-64 protocol yet. The normal
//! [`super::usb::UsbTransport`] therefore cannot be used against
//! it: `decode_status` expects the `0x55AA` status header, sees
//! the `0xAA55` command header coming back, and rejects every
//! frame.
//!
//! This transport is a bridge between "Rust RT stack validated on
//! MockTransport" (Phase C) and "Rust RT stack validated on real
//! hardware" (Phase D). It mirrors the [`super::mock::MockTransport`]
//! semantics on an actual USB bulk round-trip:
//!
//!   * sends the PhyCMD-64–encoded command bytes as-is
//!   * reads 64 bytes back
//!   * fabricates a [`Status`] populated from the *sent* command fields (since the chip just echoes
//!     them)
//!
//! When the Step 3 firmware lands (real CRC + real response
//! construction on the chip), this transport becomes obsolete and
//! callers can move back to [`super::usb::UsbTransport`].
//!
//! ## Endpoints
//!
//! The Step 2 firmware exposes:
//!   * `EP 0x02 OUT` — host → device, 64/512 bulk
//!   * `EP 0x81 IN`  — device → host, 64/512 bulk
//!
//! Note this differs from the legacy [`super::usb::UsbTransport`]
//! which hard-coded `EP 0x82` for IN. The SAM3X UOTGHS physical
//! endpoint 2 is uni-directional so it cannot simultaneously be
//! `0x02 OUT` and `0x82 IN`; hence the firmware uses hw pipe 1
//! for IN (see `ATSAM3X8E_FW/src/udi_vendor.h`).

use std::time::Duration;

use anyhow::{Context, Result};
use rusb::{DeviceHandle, GlobalContext};
use tracing::{debug, info, warn};

use super::traits::{Transport, TransportStats};
use crate::protocol::{self, Command, Status, StatusFlags, MESSAGE_SIZE};

// PhyCommander Vendor Class identification
const VENDOR_ID: u16 = 0x2341;
const PRODUCT_ID: u16 = 0x003e;

// Bulk endpoint addresses as exposed by the Step 2 firmware
const EP_OUT: u8 = 0x02;
const EP_IN: u8 = 0x81;

const INTERFACE_NUM: u8 = 0;
const TIMEOUT_MS: u64 = 50;

/// Raw-echo loopback transport for the Step 2 firmware.
///
/// Intended for hardware integration testing of the Rust RT stack.
/// Not a production transport — produces synthetic status frames
/// that reflect the *sent* command, not anything actually read
/// from the chip's sensors.
pub struct UsbLoopbackTransport {
    handle: DeviceHandle<GlobalContext>,
    stats: TransportStats,
    read_buffer: [u8; MESSAGE_SIZE],
    device_info: String,
}

impl std::fmt::Debug for UsbLoopbackTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsbLoopbackTransport")
            .field("device", &self.device_info)
            .finish()
    }
}

impl UsbLoopbackTransport {
    /// Open the PhyCommander vendor-class device (VID:PID 2341:003e)
    /// and claim interface 0.
    pub fn new() -> Result<Self> {
        info!("UsbLoopbackTransport: opening {:04x}:{:04x}", VENDOR_ID, PRODUCT_ID);

        let handle = rusb::open_device_with_vid_pid(VENDOR_ID, PRODUCT_ID).ok_or_else(|| {
            anyhow::anyhow!("PhyCommander device {:04x}:{:04x} not found", VENDOR_ID, PRODUCT_ID)
        })?;

        // Detach kernel driver if any (vendor-class shouldn't have one
        // but be defensive).
        let _ = handle.set_auto_detach_kernel_driver(true);

        handle
            .claim_interface(INTERFACE_NUM)
            .with_context(|| format!("claim interface {INTERFACE_NUM}"))?;

        let device_info = format!("USB-loopback {:04x}:{:04x}", VENDOR_ID, PRODUCT_ID);
        info!("UsbLoopbackTransport: claimed interface {}", INTERFACE_NUM);

        Ok(Self {
            handle,
            stats: TransportStats::default(),
            read_buffer: [0u8; MESSAGE_SIZE],
            device_info,
        })
    }
}

impl Transport for UsbLoopbackTransport {
    fn send_command(&mut self, cmd: &Command) -> Result<()> {
        let bytes = protocol::encode_command(cmd);
        let sent = self
            .handle
            .write_bulk(EP_OUT, &bytes, Duration::from_millis(TIMEOUT_MS))
            .context("bulk write failed")?;
        if sent != MESSAGE_SIZE {
            anyhow::bail!("short write: {} of {}", sent, MESSAGE_SIZE);
        }
        self.stats.messages_sent += 1;
        self.stats.bytes_sent += MESSAGE_SIZE as u64;
        Ok(())
    }

    fn receive_status(&mut self) -> Result<Status> {
        let read = self
            .handle
            .read_bulk(EP_IN, &mut self.read_buffer, Duration::from_millis(TIMEOUT_MS))
            .context("bulk read failed")?;
        if read != MESSAGE_SIZE {
            anyhow::bail!("short read: {} of {}", read, MESSAGE_SIZE);
        }
        self.stats.messages_received += 1;
        self.stats.bytes_received += MESSAGE_SIZE as u64;
        // On its own receive_status can't synthesize a useful
        // Status because it doesn't know what was sent. Callers
        // should use `exchange` instead.
        Ok(Status::default())
    }

    fn exchange(&mut self, cmd: &Command) -> Result<Status> {
        // Step 1: write the encoded command
        let bytes = protocol::encode_command(cmd);
        let written = self
            .handle
            .write_bulk(EP_OUT, &bytes, Duration::from_millis(TIMEOUT_MS))
            .context("bulk write failed")?;
        if written != MESSAGE_SIZE {
            anyhow::bail!("short write: {} of {}", written, MESSAGE_SIZE);
        }

        // Step 2: read 64 bytes of echo back
        let read = self
            .handle
            .read_bulk(EP_IN, &mut self.read_buffer, Duration::from_millis(TIMEOUT_MS))
            .context("bulk read failed")?;
        if read != MESSAGE_SIZE {
            self.stats.errors += 1;
            anyhow::bail!("short read: {} of {}", read, MESSAGE_SIZE);
        }

        // Step 3: verify the echo is bit-exact — if not, something
        // on the bus corrupted the frame.
        if self.read_buffer != bytes[..] {
            self.stats.errors += 1;
            debug!("echo mismatch at byte {}", find_mismatch(&bytes, &self.read_buffer));
            anyhow::bail!("echoed bytes do not match sent bytes");
        }

        self.stats.messages_sent += 1;
        self.stats.messages_received += 1;
        self.stats.bytes_sent += MESSAGE_SIZE as u64;
        self.stats.bytes_received += MESSAGE_SIZE as u64;

        // Step 4: synthesise a Status that mirrors the command,
        // matching MockTransport semantics so the upper layers
        // (scheduler + RtStats + StatusBus) see identical results
        // on the hardware path as on the mock path.
        Ok(Status {
            digital_in: cmd.digital_out,
            digital_out: cmd.digital_out,
            adc: [cmd.dac[0], cmd.dac[1], 0, 0, 0, 0, 0, 0],
            flags: StatusFlags { usb_configured: true, ..Default::default() },
            seq_num: cmd.seq_num,
            loop_time_us: 0,
            uptime_ms: 0,
            error_count: 0,
        })
    }

    fn stats(&self) -> &TransportStats {
        &self.stats
    }

    fn reset_stats(&mut self) {
        self.stats = TransportStats::default();
    }

    fn name(&self) -> &str {
        "UsbLoopback"
    }

    fn device_id(&self) -> String {
        self.device_info.clone()
    }

    fn is_connected(&self) -> bool {
        self.handle.active_configuration().is_ok()
    }

    fn set_timeout(&mut self, _timeout: Duration) -> Result<()> {
        Ok(())
    }

    fn max_rate(&self) -> u32 {
        20_000
    }

    fn typical_latency_us(&self) -> u32 {
        80
    }
}

impl Drop for UsbLoopbackTransport {
    fn drop(&mut self) {
        if let Err(e) = self.handle.release_interface(INTERFACE_NUM) {
            warn!("UsbLoopbackTransport: release interface failed: {}", e);
        }
        info!(
            "UsbLoopbackTransport closed. sent={} recv={} err={}",
            self.stats.messages_sent, self.stats.messages_received, self.stats.errors
        );
    }
}

fn find_mismatch(a: &[u8], b: &[u8]) -> usize {
    for (i, (ai, bi)) in a.iter().zip(b.iter()).enumerate() {
        if ai != bi {
            return i;
        }
    }
    a.len().min(b.len())
}
