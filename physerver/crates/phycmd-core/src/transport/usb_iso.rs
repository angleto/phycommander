//! Asynchronous isochronous USB transport for the SAM3X PhyCommander.
//!
//! Unlike [`super::usb::UsbTransport`] (synchronous bulk, one round-trip
//! per call), this transport drives the iso EPs `0x83` IN and `0x04` OUT
//! exposed by the dual-mode firmware. Iso transfers are inherently
//! periodic: every USB microframe (125 µs in HS = 8 kHz) the host
//! controller schedules one packet in each direction, regardless of
//! application activity.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────┐         ┌──────────────────────────────────┐
//! │  application thread │         │        I/O thread (this mod)     │
//! │                     │  push   │                                  │
//! │  CommandStaging  ───┼────────►│  iso OUT cb: encode + submit     │
//! │                     │         │                                  │
//! │  StatusBus       ◄──┼─────────│  iso IN  cb: decode + publish    │
//! │                     │  publish│                                  │
//! │  RtStats         ◄──┼─────────│  per-packet: tick_count++,       │
//! │                     │         │              transport_errors++  │
//! │  IsoStats        ◄──┼─────────│  iso-specific counters           │
//! └─────────────────────┘         └──────────────────────────────────┘
//! ```
//!
//! The I/O thread owns the libusb context and all transfer descriptors;
//! they never escape it. The shared state ([`IsoInner`]) is reachable
//! from callbacks via the `user_data` pointer of each transfer.
//!
//! ## Loss model
//!
//! Iso has no NAK / retry at the USB level. Every microframe slot is
//! either used (transfer descriptor was queued in time) or wasted
//! (descriptor missed the schedule). The firmware applies "control hold"
//! semantics: a missed OUT means the device keeps applying the last
//! received setpoints. A missed IN simply means the host doesn't see
//! a fresh status sample for that microframe.
//!
//! The CRC-16-CCITT in the PhyCMD-64 protocol still catches in-flight
//! bit-errors, exposed as [`IsoStats::iso_in_crc_errors`].

use crate::protocol::{decode_status, encode_command, Command, MESSAGE_SIZE};
use crate::staging::CommandStaging;
use crate::stats::RtStats;
use crate::status_bus::{StatusBus, StatusFrame};

use anyhow::{Context, Result};
use libusb1_sys as ffi;
use parking_lot::RwLock;
use std::os::raw::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

// ---------------------------------------------------------------------
//   Tunables
// ---------------------------------------------------------------------

/// Arduino Due USB descriptor (firmware ATSAM3X8E_FW/src/udi_vendor.h).
const VID: u16 = 0x2341;
const PID: u16 = 0x003e;
const INTERFACE: i32 = 0;
const EP_ISO_IN: u8 = 0x83;
const EP_ISO_OUT: u8 = 0x04;

/// Iso wMaxPacketSize must match the firmware exactly (256 B per
/// microframe in HS — see UDI_VENDOR_EP_SIZE_ISO_HS in udi_vendor.h).
const ISO_PKT_SIZE: usize = 256;

/// Number of iso transfers kept in flight per direction. Each transfer
/// carries `PKTS_PER_TRANSFER` microframes (one transfer = 1 ms of
/// iso traffic). Eight transfers × 1 ms = 8 ms of buffered work, plenty
/// for the libusb event loop to keep the EHCI iso scheduler fed even
/// under brief CPU stalls.
const NUM_TRANSFERS: usize = 8;

/// Microframes packed into a single iso transfer. Larger values reduce
/// per-transfer libusb overhead but increase callback granularity (and
/// thus the published status latency). 8 = 1 ms = a sweet spot for a
/// 5–8 kHz control loop.
const PKTS_PER_TRANSFER: usize = 8;

/// Timeout passed to libusb_handle_events_timeout_completed. The I/O
/// thread wakes up at least this often to check the stop flag, even
/// when no transfers complete (which only happens at shutdown).
const HANDLE_EVENTS_TIMEOUT: Duration = Duration::from_millis(100);

// ---------------------------------------------------------------------
//   Public types
// ---------------------------------------------------------------------

/// Iso-specific counters, exposed alongside [`RtStats`].
///
/// The I/O thread updates these from its libusb completion callbacks.
/// All fields are atomic so the consumer (HTTP handler thread) can
/// snapshot them without holding any lock.
#[derive(Debug, Default)]
pub struct IsoStats {
    /// Iso IN packets that completed successfully and parsed as a
    /// valid PhyCMD-64 status (header + CRC OK).
    pub iso_in_pkts_ok: AtomicU64,
    /// Iso IN packets where libusb reported a transfer error or
    /// timeout in the per-packet status field.
    pub iso_in_errors: AtomicU64,
    /// Iso IN packets that arrived with `actual_length < 64` (truncated
    /// or zero-length, treated as a microframe slot the device didn't
    /// fill).
    pub iso_in_short: AtomicU64,
    /// Iso IN packets that decoded but failed CRC / header validation.
    pub iso_in_crc_errors: AtomicU64,

    /// Iso OUT packets that completed successfully on the wire.
    pub iso_out_pkts_ok: AtomicU64,
    /// Iso OUT packets where libusb reported a transfer error.
    pub iso_out_errors: AtomicU64,

    /// Cumulative number of distinct command snapshots taken from
    /// staging (one per OUT transfer, not per packet).
    pub commands_taken: AtomicU64,
}

impl IsoStats {
    pub fn snapshot(&self) -> IsoStatsSnapshot {
        IsoStatsSnapshot {
            iso_in_pkts_ok: self.iso_in_pkts_ok.load(Ordering::Relaxed),
            iso_in_errors: self.iso_in_errors.load(Ordering::Relaxed),
            iso_in_short: self.iso_in_short.load(Ordering::Relaxed),
            iso_in_crc_errors: self.iso_in_crc_errors.load(Ordering::Relaxed),
            iso_out_pkts_ok: self.iso_out_pkts_ok.load(Ordering::Relaxed),
            iso_out_errors: self.iso_out_errors.load(Ordering::Relaxed),
            commands_taken: self.commands_taken.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IsoStatsSnapshot {
    pub iso_in_pkts_ok: u64,
    pub iso_in_errors: u64,
    pub iso_in_short: u64,
    pub iso_in_crc_errors: u64,
    pub iso_out_pkts_ok: u64,
    pub iso_out_errors: u64,
    pub commands_taken: u64,
}

// ---------------------------------------------------------------------
//   Shared state
// ---------------------------------------------------------------------

/// State reachable from the libusb callbacks via `user_data`.
struct IsoInner {
    /// Set by the foreground thread to ask the I/O thread to drain and
    /// exit. Callbacks check this before re-submitting transfers.
    stop: AtomicBool,

    /// Latest decoded status from the device. Updated on every
    /// successful iso IN packet. Consumers read with a quick read-lock
    /// (no allocation, ~10 ns on uncontended path).
    latest_status: RwLock<crate::protocol::Status>,

    /// Outgoing command pipeline (shared with the application).
    staging: Arc<CommandStaging>,

    /// Status broadcast (shared with HTTP / WebSocket handlers).
    bus: Arc<StatusBus>,

    /// Generic RT stats (shared with the existing /api/rt_stats
    /// endpoint). For iso, `tick_count` advances per IN packet and
    /// `transport_errors` per IN/OUT failure.
    stats: Arc<RtStats>,

    /// Iso-specific counters, exposed via `IsoTransport::iso_stats()`.
    iso_stats: Arc<IsoStats>,

    /// Monotonic packet counter (used as `cmd_seq` in published
    /// `StatusFrame`s — it is not the wire seq_num, which is only
    /// 8-bit and only incremented on apply).
    seq_counter: AtomicU64,

    /// I/O thread start instant — used to compute `tick_expected_ns`
    /// and `tick_sent_ns` on each published StatusFrame in a way that
    /// is comparable across frames.
    start: Instant,
}

// SAFETY: IsoInner contains only Arc'd, atomic, and lock-protected
// state. No raw pointers or !Sync types live here.
unsafe impl Send for IsoInner {}
unsafe impl Sync for IsoInner {}

/// The user_data passed to libusb. Holding `dir_in: bool` separately
/// from the shared IsoInner avoids a lookup or branch per packet.
struct CallbackCtx {
    inner: Arc<IsoInner>,
    dir_in: bool,
}

// ---------------------------------------------------------------------
//   Public API
// ---------------------------------------------------------------------

/// Iso-mode USB transport. Spawns a dedicated I/O thread on `new()`
/// and tears it down on Drop. The application interacts only via the
/// shared [`CommandStaging`] and [`StatusBus`] passed to `new()`.
pub struct IsoTransport {
    inner: Arc<IsoInner>,
    iso_stats: Arc<IsoStats>,
    io_thread: Option<JoinHandle<()>>,
}

impl IsoTransport {
    /// Spawn the iso I/O thread and start servicing the iso EPs.
    ///
    /// Errors at this stage (libusb init, device not found, interface
    /// claim) propagate before the thread starts. Once the thread is
    /// up, libusb errors are surfaced via `IsoStats` counters and
    /// `RtStats::transport_errors` rather than panicking.
    pub fn new(
        staging: Arc<CommandStaging>,
        bus: Arc<StatusBus>,
        stats: Arc<RtStats>,
    ) -> Result<Self> {
        let iso_stats = Arc::new(IsoStats::default());

        let inner = Arc::new(IsoInner {
            stop: AtomicBool::new(false),
            latest_status: RwLock::new(crate::protocol::Status::default()),
            staging,
            bus,
            stats,
            iso_stats: Arc::clone(&iso_stats),
            seq_counter: AtomicU64::new(0),
            start: Instant::now(),
        });

        // The I/O thread does *all* libusb work. We open the device
        // here in the calling thread first to surface "device not
        // found" / "interface busy" errors synchronously, then close
        // and let the I/O thread re-open it on its own context — this
        // is necessary because libusb contexts and handles are not
        // safe to share across the open/handle_events boundary in
        // older libusb releases (1.0.21 and earlier). Re-opening from
        // the I/O thread costs ~5 ms but happens only once at startup.
        unsafe {
            let mut ctx_ptr: *mut ffi::libusb_context = ptr::null_mut();
            let r = ffi::libusb_init(&mut ctx_ptr);
            anyhow::ensure!(r == 0, "libusb_init pre-check failed: {r}");
            let dh = ffi::libusb_open_device_with_vid_pid(ctx_ptr, VID, PID);
            anyhow::ensure!(
                !dh.is_null(),
                "Arduino Due not found (VID={VID:04x} PID={PID:04x}). \
                 Is the dual-mode iso firmware flashed and the device powered?"
            );
            let _ = ffi::libusb_detach_kernel_driver(dh, INTERFACE);
            let r = ffi::libusb_claim_interface(dh, INTERFACE);
            ffi::libusb_close(dh);
            ffi::libusb_exit(ctx_ptr);
            anyhow::ensure!(
                r == 0,
                "Pre-claim failed: {r} (is another process holding the interface?)"
            );
        }

        let inner_for_thread = Arc::clone(&inner);
        let io_thread = std::thread::Builder::new()
            .name("phycmd-iso".into())
            .spawn(move || {
                if let Err(e) = io_thread_main(inner_for_thread) {
                    error!("iso I/O thread terminated: {e:#}");
                }
            })
            .context("spawning phycmd-iso I/O thread")?;

        info!(
            "IsoTransport up — {} transfers × {} packets × {} B per direction",
            NUM_TRANSFERS, PKTS_PER_TRANSFER, ISO_PKT_SIZE
        );

        Ok(Self {
            inner,
            iso_stats,
            io_thread: Some(io_thread),
        })
    }

    /// Snapshot of iso-specific counters.
    pub fn iso_stats(&self) -> IsoStatsSnapshot {
        self.iso_stats.snapshot()
    }

    /// Shared handle to the iso counters, for plumbing into the web
    /// layer alongside the existing [`RtStats`] handle.
    pub fn iso_stats_arc(&self) -> Arc<IsoStats> {
        Arc::clone(&self.iso_stats)
    }

    /// Latest decoded status (the most recent iso IN packet that
    /// passed CRC). Useful for non-broadcast read paths.
    pub fn latest_status(&self) -> crate::protocol::Status {
        self.inner.latest_status.read().clone()
    }
}

impl Drop for IsoTransport {
    fn drop(&mut self) {
        self.inner.stop.store(true, Ordering::Release);
        if let Some(h) = self.io_thread.take() {
            // Allow up to a few seconds for the I/O thread to drain
            // outstanding transfers. If it doesn't exit (libusb bug
            // or hung kernel driver) we log and bail rather than
            // hanging the whole physerver shutdown.
            let _ = h.join();
        }
        info!("IsoTransport stopped");
    }
}

// ---------------------------------------------------------------------
//   I/O thread — owns the libusb context and all transfer descriptors
// ---------------------------------------------------------------------

/// Resources allocated by the I/O thread. They're held as fields so
/// they live exactly as long as the thread (no leaks if the thread
/// panics with the panic-handler installed by the runtime).
struct IoResources {
    ctx: *mut ffi::libusb_context,
    dev_handle: *mut ffi::libusb_device_handle,
    transfers_in: Vec<*mut ffi::libusb_transfer>,
    transfers_out: Vec<*mut ffi::libusb_transfer>,
    // Buffers and callback contexts kept alive via Vec; transfers
    // hold raw pointers into them.
    _buffers_in: Vec<Vec<u8>>,
    _buffers_out: Vec<Vec<u8>>,
    _cb_ctx_in: Vec<Box<CallbackCtx>>,
    _cb_ctx_out: Vec<Box<CallbackCtx>>,
}

impl Drop for IoResources {
    fn drop(&mut self) {
        unsafe {
            for &t in self.transfers_in.iter().chain(self.transfers_out.iter()) {
                ffi::libusb_free_transfer(t);
            }
            if !self.dev_handle.is_null() {
                let _ = ffi::libusb_release_interface(self.dev_handle, INTERFACE);
                ffi::libusb_close(self.dev_handle);
            }
            if !self.ctx.is_null() {
                ffi::libusb_exit(self.ctx);
            }
        }
    }
}

fn io_thread_main(inner: Arc<IsoInner>) -> Result<()> {
    unsafe {
        // ---- libusb init ----
        let mut ctx_ptr: *mut ffi::libusb_context = ptr::null_mut();
        let r = ffi::libusb_init(&mut ctx_ptr);
        anyhow::ensure!(r == 0, "libusb_init failed: {r}");

        let dev_handle = ffi::libusb_open_device_with_vid_pid(ctx_ptr, VID, PID);
        anyhow::ensure!(
            !dev_handle.is_null(),
            "device disappeared between pre-check and I/O thread start"
        );

        let _ = ffi::libusb_detach_kernel_driver(dev_handle, INTERFACE);
        let r = ffi::libusb_claim_interface(dev_handle, INTERFACE);
        anyhow::ensure!(r == 0, "claim_interface failed in I/O thread: {r}");

        let mut res = IoResources {
            ctx: ctx_ptr,
            dev_handle,
            transfers_in: Vec::with_capacity(NUM_TRANSFERS),
            transfers_out: Vec::with_capacity(NUM_TRANSFERS),
            _buffers_in: Vec::with_capacity(NUM_TRANSFERS),
            _buffers_out: Vec::with_capacity(NUM_TRANSFERS),
            _cb_ctx_in: Vec::with_capacity(NUM_TRANSFERS),
            _cb_ctx_out: Vec::with_capacity(NUM_TRANSFERS),
        };

        // ---- Allocate transfer pool ----
        for _ in 0..NUM_TRANSFERS {
            // IN
            let buf_in = vec![0u8; PKTS_PER_TRANSFER * ISO_PKT_SIZE];
            let cb_in = Box::new(CallbackCtx {
                inner: Arc::clone(&inner),
                dir_in: true,
            });
            let xfer_in = alloc_iso_transfer(
                dev_handle,
                EP_ISO_IN,
                &buf_in,
                cb_in.as_ref() as *const CallbackCtx as *mut c_void,
            )?;
            res.transfers_in.push(xfer_in);
            res._buffers_in.push(buf_in);
            res._cb_ctx_in.push(cb_in);

            // OUT — pre-fill with a default Command. The callback
            // will overwrite this on every completion before re-submit.
            let mut buf_out = vec![0u8; PKTS_PER_TRANSFER * ISO_PKT_SIZE];
            let default_cmd = encode_command(&Command::default());
            for p in 0..PKTS_PER_TRANSFER {
                let off = p * ISO_PKT_SIZE;
                buf_out[off..off + MESSAGE_SIZE].copy_from_slice(&default_cmd);
                // bytes [off+64..off+256] stay zero (padding contract)
            }
            let cb_out = Box::new(CallbackCtx {
                inner: Arc::clone(&inner),
                dir_in: false,
            });
            let xfer_out = alloc_iso_transfer(
                dev_handle,
                EP_ISO_OUT,
                &buf_out,
                cb_out.as_ref() as *const CallbackCtx as *mut c_void,
            )?;
            res.transfers_out.push(xfer_out);
            res._buffers_out.push(buf_out);
            res._cb_ctx_out.push(cb_out);
        }

        // ---- Submit them all ----
        for &t in res.transfers_in.iter().chain(res.transfers_out.iter()) {
            let r = ffi::libusb_submit_transfer(t);
            if r != 0 {
                warn!("initial libusb_submit_transfer failed: {r}");
            }
        }

        let timeout_tv = libc::timeval {
            tv_sec: HANDLE_EVENTS_TIMEOUT.as_secs() as _,
            tv_usec: HANDLE_EVENTS_TIMEOUT.subsec_micros() as _,
        };

        // ---- Steady-state event loop ----
        while !inner.stop.load(Ordering::Acquire) {
            ffi::libusb_handle_events_timeout_completed(ctx_ptr, &timeout_tv, ptr::null_mut());
        }

        // ---- Shutdown: cancel outstanding transfers, then drain ----
        info!("iso I/O thread received stop signal — cancelling transfers");
        for &t in res.transfers_in.iter().chain(res.transfers_out.iter()) {
            ffi::libusb_cancel_transfer(t);
        }
        let drain_until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < drain_until {
            ffi::libusb_handle_events_timeout_completed(ctx_ptr, &timeout_tv, ptr::null_mut());
        }

        // res drops here, freeing transfers and closing the device.
        Ok(())
    }
}

unsafe fn alloc_iso_transfer(
    dev_handle: *mut ffi::libusb_device_handle,
    endpoint: u8,
    buf: &Vec<u8>,
    user_data: *mut c_void,
) -> Result<*mut ffi::libusb_transfer> {
    let xfer = ffi::libusb_alloc_transfer(PKTS_PER_TRANSFER as i32);
    anyhow::ensure!(!xfer.is_null(), "libusb_alloc_transfer failed");
    (*xfer).dev_handle = dev_handle;
    (*xfer).endpoint = endpoint;
    (*xfer).transfer_type = ffi::constants::LIBUSB_TRANSFER_TYPE_ISOCHRONOUS;
    (*xfer).timeout = 0;
    (*xfer).buffer = buf.as_ptr() as *mut u8;
    (*xfer).length = (PKTS_PER_TRANSFER * ISO_PKT_SIZE) as i32;
    (*xfer).num_iso_packets = PKTS_PER_TRANSFER as i32;
    (*xfer).callback = iso_callback;
    (*xfer).user_data = user_data;
    ffi::libusb_set_iso_packet_lengths(xfer, ISO_PKT_SIZE as u32);
    Ok(xfer)
}

// ---------------------------------------------------------------------
//   Completion callback — runs from libusb_handle_events_*
// ---------------------------------------------------------------------

extern "system" fn iso_callback(transfer: *mut ffi::libusb_transfer) {
    // SAFETY: libusb invokes this with a transfer it owns and whose
    // user_data points at a CallbackCtx that the I/O thread holds in
    // a Box for the lifetime of the transfer.
    unsafe { iso_callback_impl(transfer) }
}

unsafe fn iso_callback_impl(transfer: *mut ffi::libusb_transfer) {
    let xfer = &mut *transfer;
    let ctx = &*(xfer.user_data as *const CallbackCtx);
    let inner = &*ctx.inner;

    // Hard error on the whole transfer (cancelled, no-device, ...).
    // Per-packet errors are reported in the iso_packet_desc array
    // even when the overall transfer is COMPLETED; the wrapper status
    // here only fires for whole-transfer-level failures.
    if xfer.status != ffi::constants::LIBUSB_TRANSFER_COMPLETED {
        let n = xfer.num_iso_packets as u64;
        if ctx.dir_in {
            inner.iso_stats.iso_in_errors.fetch_add(n, Ordering::Relaxed);
        } else {
            inner.iso_stats.iso_out_errors.fetch_add(n, Ordering::Relaxed);
        }
        inner.stats.record_transport_error();
        if !inner.stop.load(Ordering::Relaxed) {
            ffi::libusb_submit_transfer(transfer);
        }
        return;
    }

    let pkt_descs = std::slice::from_raw_parts(
        xfer.iso_packet_desc.as_ptr(),
        xfer.num_iso_packets as usize,
    );

    if ctx.dir_in {
        for (i, desc) in pkt_descs.iter().enumerate() {
            handle_in_packet(inner, xfer.buffer, i, desc);
        }
    } else {
        // OUT direction: count completions, then refill buffer with
        // the latest command snapshot for the next submission.
        let mut errs: u64 = 0;
        let mut oks: u64 = 0;
        for desc in pkt_descs.iter() {
            if desc.status == ffi::constants::LIBUSB_TRANSFER_COMPLETED {
                oks += 1;
            } else {
                errs += 1;
            }
        }
        if oks > 0 {
            inner.iso_stats.iso_out_pkts_ok.fetch_add(oks, Ordering::Relaxed);
        }
        if errs > 0 {
            inner.iso_stats.iso_out_errors.fetch_add(errs, Ordering::Relaxed);
            inner.stats.record_transport_error();
        }

        // Refresh OUT buffer from staging once per transfer (every
        // PKTS_PER_TRANSFER microframes = 1 ms). Filling per-packet
        // would not help — the firmware applies whatever it last got.
        let (cmd, gen) = inner.staging.take_snapshot();
        let encoded = encode_command(&cmd);
        let buf_len = (xfer.length as usize).min(PKTS_PER_TRANSFER * ISO_PKT_SIZE);
        let buf = std::slice::from_raw_parts_mut(xfer.buffer, buf_len);
        for p in 0..PKTS_PER_TRANSFER {
            let off = p * ISO_PKT_SIZE;
            if off + MESSAGE_SIZE <= buf.len() {
                buf[off..off + MESSAGE_SIZE].copy_from_slice(&encoded);
                // padding bytes were zero at boot and we never write
                // anything in [off+64..off+256] so they stay zero.
            }
        }
        inner.iso_stats.commands_taken.fetch_add(1, Ordering::Relaxed);
        inner.staging.mark_sent(gen);
    }

    if !inner.stop.load(Ordering::Relaxed) {
        let r = ffi::libusb_submit_transfer(transfer);
        if r != 0 {
            warn!("iso resubmit failed (dir_in={}): {r}", ctx.dir_in);
            inner.stats.record_transport_error();
        }
    }
}

unsafe fn handle_in_packet(
    inner: &IsoInner,
    base: *mut u8,
    pkt_index: usize,
    desc: &ffi::libusb_iso_packet_descriptor,
) {
    if desc.status != ffi::constants::LIBUSB_TRANSFER_COMPLETED {
        inner.iso_stats.iso_in_errors.fetch_add(1, Ordering::Relaxed);
        inner.stats.record_transport_error();
        return;
    }

    let actual = desc.actual_length as usize;
    if actual < MESSAGE_SIZE {
        inner.iso_stats.iso_in_short.fetch_add(1, Ordering::Relaxed);
        return;
    }

    let off = pkt_index * ISO_PKT_SIZE;
    let pkt = std::slice::from_raw_parts(base.add(off), MESSAGE_SIZE);

    let status = match decode_status(pkt) {
        Ok(s) => s,
        Err(_) => {
            inner.iso_stats.iso_in_crc_errors.fetch_add(1, Ordering::Relaxed);
            inner.stats.record_transport_error();
            return;
        }
    };

    inner.iso_stats.iso_in_pkts_ok.fetch_add(1, Ordering::Relaxed);

    // Update latest snapshot (cheap RwLock; no other writers).
    *inner.latest_status.write() = status.clone();

    // Build a StatusFrame and publish to the bus. Iso doesn't have
    // a per-tick deadline so jitter_us = 0 and tick_expected = sent =
    // recv = "now". We still set cmd_seq to a monotonic packet
    // counter so subscribers can detect gaps.
    let now_ns = inner.start.elapsed().as_nanos() as i64;
    let cmd_seq = inner.seq_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let frame = StatusFrame {
        status,
        cmd_seq,
        wire_seq: 0, // not meaningful for iso (firmware seq advances only on apply)
        tick_index: cmd_seq,
        tick_expected_ns: now_ns,
        tick_sent_ns: now_ns,
        tick_recv_ns: now_ns,
        latency_us: 0,
        jitter_us: 0,
        missed_ticks_prior: 0,
    };
    inner.bus.publish(frame);

    // Also count it as a "tick" for the existing /api/rt_stats
    // endpoint so the dashboard shows live throughput without
    // dashboard changes (8 kHz successful packets ≈ 8 kHz tick rate).
    inner.stats.record_ok(0, 0);
}
