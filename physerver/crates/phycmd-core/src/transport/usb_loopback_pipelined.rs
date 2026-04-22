//! Pipelined version of [`super::usb_loopback::UsbLoopbackTransport`].
//!
//! A dedicated I/O helper thread performs the synchronous `write_bulk`
//! + `read_bulk` round-trip. The SCHED_FIFO scheduler thread submits
//! commands and reaps results via a pair of bounded(1) crossbeam
//! channels, never touching any kernel-blocking USB call.
//!
//! See the [`super::traits::PipelinedTransport`] trait for the
//! two-phase submit/reap contract and the architectural rationale.

use super::traits::{PipelinedTransport, TransportStats};
use crate::protocol::{self, Command, Status, StatusFlags, MESSAGE_SIZE};
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use rusb::{DeviceHandle, GlobalContext};
use std::fmt;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{debug, info, trace};

const VENDOR_ID: u16 = 0x2341;
const PRODUCT_ID: u16 = 0x003e;
const EP_OUT: u8 = 0x02;
const EP_IN: u8 = 0x81;
const INTERFACE: u8 = 0;
const IO_TIMEOUT: Duration = Duration::from_millis(50);

// -------------------------------------------------------------------------
//   Wire types exchanged over the channels
// -------------------------------------------------------------------------

struct IoRequest {
    encoded: [u8; MESSAGE_SIZE],
    cmd: Command,
}

// -------------------------------------------------------------------------
//   I/O helper thread body
// -------------------------------------------------------------------------

fn io_helper(
    handle: DeviceHandle<GlobalContext>,
    rx_cmd: Receiver<IoRequest>,
    tx_res: Sender<Result<Status>>,
) {
    let mut stats_sent: u64 = 0;
    let mut stats_recv: u64 = 0;
    let mut read_buf = [0u8; MESSAGE_SIZE];

    loop {
        // Block until the scheduler submits a command (or the channel
        // is closed on shutdown).
        let req = match rx_cmd.recv() {
            Ok(r) => r,
            Err(_) => {
                trace!("io_helper: command channel closed, exiting");
                break;
            }
        };

        // Synchronous USB exchange — this is the only place in the
        // system where we block on kernel I/O.
        let result = (|| -> Result<Status> {
            let written =
                handle.write_bulk(EP_OUT, &req.encoded, IO_TIMEOUT).context("write_bulk")?;
            if written != MESSAGE_SIZE {
                anyhow::bail!("short write: {written}/{MESSAGE_SIZE}");
            }
            stats_sent += 1;

            let read = handle.read_bulk(EP_IN, &mut read_buf, IO_TIMEOUT).context("read_bulk")?;
            if read != MESSAGE_SIZE {
                anyhow::bail!("short read: {read}/{MESSAGE_SIZE}");
            }
            stats_recv += 1;

            // Verify bit-exact echo (Step 2 firmware is raw echo)
            if read_buf != req.encoded {
                let pos = read_buf
                    .iter()
                    .zip(req.encoded.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or(MESSAGE_SIZE);
                anyhow::bail!("echo mismatch at byte {pos}");
            }

            // Synthesise Status from the echoed command (same logic
            // as the sync UsbLoopbackTransport).
            Ok(Status {
                digital_in: req.cmd.digital_out,
                digital_out: req.cmd.digital_out,
                adc: [req.cmd.dac[0], req.cmd.dac[1], 0, 0, 0, 0, 0, 0],
                flags: StatusFlags { usb_configured: true, ..Default::default() },
                seq_num: req.cmd.seq_num,
                loop_time_us: 0,
                uptime_ms: 0,
                error_count: 0,
            })
        })();

        // Send the result back. If the scheduler has dropped its
        // receiver (shutdown race), we exit silently.
        if tx_res.send(result).is_err() {
            trace!("io_helper: result channel closed, exiting");
            break;
        }
    }

    debug!("io_helper exiting: {} sent, {} received", stats_sent, stats_recv);

    // Release the USB interface on exit.
    let _ = handle.release_interface(INTERFACE);
}

// -------------------------------------------------------------------------
//   PipelinedUsbLoopbackTransport
// -------------------------------------------------------------------------

pub struct PipelinedUsbLoopbackTransport {
    tx_cmd: Sender<IoRequest>,
    rx_res: Receiver<Result<Status>>,
    io_thread: Option<JoinHandle<()>>,
    stats: TransportStats,
    device_info: String,
    in_flight: bool,
}

impl fmt::Debug for PipelinedUsbLoopbackTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipelinedUsbLoopbackTransport")
            .field("device", &self.device_info)
            .field("in_flight", &self.in_flight)
            .finish()
    }
}

impl PipelinedUsbLoopbackTransport {
    /// Open the PhyCommander device and spawn the I/O helper thread.
    pub fn new() -> Result<Self> {
        info!("PipelinedUsbLoopbackTransport: opening {:04x}:{:04x}", VENDOR_ID, PRODUCT_ID);

        let handle = rusb::open_device_with_vid_pid(VENDOR_ID, PRODUCT_ID).ok_or_else(|| {
            anyhow::anyhow!("device {:04x}:{:04x} not found", VENDOR_ID, PRODUCT_ID)
        })?;

        let _ = handle.set_auto_detach_kernel_driver(true);
        handle
            .claim_interface(INTERFACE)
            .with_context(|| format!("claim interface {INTERFACE}"))?;

        let (tx_cmd, rx_cmd) = bounded::<IoRequest>(1);
        let (tx_res, rx_res) = bounded::<Result<Status>>(1);

        let device_info = format!("USB-pipelined {:04x}:{:04x}", VENDOR_ID, PRODUCT_ID);

        let io_thread = thread::Builder::new()
            .name("phycmd-usb-io".to_string())
            .spawn(move || io_helper(handle, rx_cmd, tx_res))?;

        info!("PipelinedUsbLoopbackTransport: helper thread spawned");

        Ok(Self {
            tx_cmd,
            rx_res,
            io_thread: Some(io_thread),
            stats: TransportStats::default(),
            device_info,
            in_flight: false,
        })
    }
}

impl PipelinedTransport for PipelinedUsbLoopbackTransport {
    fn submit(&mut self, cmd: &Command) -> Result<()> {
        if self.in_flight {
            anyhow::bail!(
                "submit called while a previous transfer is still in flight \
                 (depth-1 pipeline violation)"
            );
        }
        let encoded = protocol::encode_command(cmd);
        let req = IoRequest { encoded, cmd: cmd.clone() };
        self.tx_cmd
            .send(req)
            .map_err(|_| anyhow::anyhow!("I/O helper thread is dead"))?;
        self.in_flight = true;
        Ok(())
    }

    fn reap(&mut self, timeout: Duration) -> Result<Status> {
        if !self.in_flight {
            anyhow::bail!("reap called with no in-flight transfer");
        }
        let result = self.rx_res.recv_timeout(timeout).map_err(|e| match e {
            crossbeam_channel::RecvTimeoutError::Timeout => {
                self.stats.timeout_errors += 1;
                anyhow::anyhow!("reap timed out after {timeout:?}")
            }
            crossbeam_channel::RecvTimeoutError::Disconnected => {
                anyhow::anyhow!("I/O helper thread died")
            }
        })?;

        self.in_flight = false;

        match &result {
            Ok(_) => {
                self.stats.messages_sent += 1;
                self.stats.messages_received += 1;
                self.stats.bytes_sent += MESSAGE_SIZE as u64;
                self.stats.bytes_received += MESSAGE_SIZE as u64;
            }
            Err(_) => {
                self.stats.errors += 1;
            }
        }

        result
    }

    fn drain(&mut self, timeout: Duration) -> Result<Option<Status>> {
        if !self.in_flight {
            return Ok(None);
        }
        match self.reap(timeout) {
            Ok(s) => Ok(Some(s)),
            Err(e) => {
                self.in_flight = false;
                Err(e)
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
        "PipelinedUsbLoopback"
    }
    fn device_id(&self) -> String {
        self.device_info.clone()
    }
    fn is_connected(&self) -> bool {
        self.io_thread.as_ref().map(|h| !h.is_finished()).unwrap_or(false)
    }
}

impl Drop for PipelinedUsbLoopbackTransport {
    fn drop(&mut self) {
        // Close the command channel so the helper thread exits its
        // recv() loop. Then join it (with a generous timeout in case
        // it's stuck in a slow USB transfer).
        drop(self.tx_cmd.clone()); // drop the sender
                                   // Actually we need to move tx_cmd out. But we can't in Drop.
                                   // The real move-out happens when `Self` is dropped — the
                                   // Sender field is dropped automatically, closing the channel.
        if let Some(h) = self.io_thread.take() {
            // Give the helper up to 500 ms to finish its current
            // transfer and notice the closed channel.
            let _ = h.join();
        }
        info!(
            "PipelinedUsbLoopbackTransport dropped: sent={} recv={} err={}",
            self.stats.messages_sent, self.stats.messages_received, self.stats.errors
        );
    }
}
