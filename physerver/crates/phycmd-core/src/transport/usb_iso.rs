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

use crate::protocol::wave_types::*;
use crate::protocol::{decode_status, encode_command, Command, MESSAGE_SIZE};
use crate::staging::CommandStaging;
use crate::stats::RtStats;
use crate::status_bus::{StatusBus, StatusFrame};
use crate::waveforms::WaveformBank;

use anyhow::{Context, Result};
use libusb1_sys as ffi;
use parking_lot::RwLock;
use serde::Serialize;
use std::os::raw::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicU8, Ordering};
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
    /// Zero every counter. Intended for the `/api/reset_telemetry`
    /// REST endpoint so the dashboard can restart measurements on
    /// demand without rebooting the service.
    pub fn reset(&self) {
        self.iso_in_pkts_ok.store(0, Ordering::Relaxed);
        self.iso_in_errors.store(0, Ordering::Relaxed);
        self.iso_in_short.store(0, Ordering::Relaxed);
        self.iso_in_crc_errors.store(0, Ordering::Relaxed);
        self.iso_out_pkts_ok.store(0, Ordering::Relaxed);
        self.iso_out_errors.store(0, Ordering::Relaxed);
        self.commands_taken.store(0, Ordering::Relaxed);
    }

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
    /// libusb context and device handle, opened once in
    /// `IsoTransport::new` (main thread) and shared between the iso
    /// I/O thread (which uses EP3/EP4 for transfers) and the
    /// `WaveformDevice` control client (which uses EP0). libusb is
    /// thread-safe across distinct EP groups so the two paths
    /// do not race.
    ctx: *mut ffi::libusb_context,
    dev_handle: *mut ffi::libusb_device_handle,

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

    /// Optional waveform generator. When any of its channels is
    /// `enabled`, the iso OUT callback computes per-microframe
    /// commands instead of broadcasting a single staging snapshot.
    waveforms: Arc<WaveformBank>,

    /// Monotonic packet counter (used as `cmd_seq` in published
    /// `StatusFrame`s — it is not the wire seq_num, which is only
    /// 8-bit and only incremented on apply).
    seq_counter: AtomicU64,

    /// Rolling wire seq_num written into outgoing commands. Wraps at
    /// 256 (u8). Bumped once per OUT transfer (1 kHz) so the firmware
    /// echoes a changing seq and we can detect command drops and
    /// measure USB round-trip latency in the IN callback.
    out_seq_counter: AtomicU8,

    /// When false, the OUT path skips seq bumping / timestamp store
    /// and the IN path skips the latency correlation lookup. Leaves
    /// URB-level jitter + tick_count untouched. Toggle via
    /// POST /api/telemetry/detail to save a few atomic ops/s
    /// (~16 k/s on the OUT path at 8 kHz) when the detail isn't
    /// being watched.
    telemetry_detail_enabled: AtomicBool,

    /// Per-seq send timestamps (nanoseconds since `start`). The IN
    /// callback correlates status.seq_num against this ring to
    /// compute per-packet round-trip latency.
    out_seq_sent_ns: [AtomicI64; 256],

    /// Timestamp of the previous IN URB completion. Used to compute
    /// inter-URB jitter against the expected 1 ms transfer period
    /// (8 microframes × 125 us). Measured at URB granularity because
    /// libusb batches per-packet callbacks for an entire URB into a
    /// single burst — inter-packet deltas are not physically
    /// meaningful (all ~0, with a 1 ms gap between URBs).
    last_urb_ns: AtomicI64,

    /// I/O thread start instant — used to compute `tick_expected_ns`
    /// and `tick_sent_ns` on each published StatusFrame in a way that
    /// is comparable across frames.
    start: Instant,
}

// SAFETY: IsoInner contains the libusb context and device handle as
// raw pointers; libusb is documented as thread-safe for control vs
// transfer operations on disjoint endpoints. No part of the pointer
// targets is mutated from Rust safe code; the I/O thread and the
// WaveformDevice both call libusb routines that are designed for
// concurrent use within the same context.
unsafe impl Send for IsoInner {}
unsafe impl Sync for IsoInner {}

impl Drop for IsoInner {
    fn drop(&mut self) {
        // The I/O thread has already joined (see IsoTransport::Drop),
        // so we are the sole owner of ctx + dev_handle here. Free
        // the libusb resources cleanly.
        unsafe {
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
    waveforms: Arc<WaveformBank>,
    waveform_dev: Option<Arc<WaveformDevice>>, // populated once the I/O thread has its dev_handle
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
        let waveforms = Arc::new(WaveformBank::new());

        // Open the libusb context + claim the interface once, on the
        // main thread. Both the iso I/O thread (using EP3/EP4 for
        // bulk transfers) and the WaveformDevice control client
        // (using EP0 for vendor SETUP requests) share this dev_handle.
        // libusb is documented as thread-safe across distinct EP
        // groups; the two paths therefore do not race.
        let (ctx, dev_handle) = unsafe {
            let mut ctx_ptr: *mut ffi::libusb_context = ptr::null_mut();
            let r = ffi::libusb_init(&mut ctx_ptr);
            anyhow::ensure!(r == 0, "libusb_init failed: {r}");
            let dh = ffi::libusb_open_device_with_vid_pid(ctx_ptr, VID, PID);
            if dh.is_null() {
                ffi::libusb_exit(ctx_ptr);
                anyhow::bail!(
                    "Arduino Due not found (VID={VID:04x} PID={PID:04x}). \
                     Is the dual-mode iso firmware flashed and the device powered?"
                );
            }
            let _ = ffi::libusb_detach_kernel_driver(dh, INTERFACE);
            let r = ffi::libusb_claim_interface(dh, INTERFACE);
            if r != 0 {
                ffi::libusb_close(dh);
                ffi::libusb_exit(ctx_ptr);
                anyhow::bail!(
                    "claim_interface failed: {r} (is another process holding the interface?)"
                );
            }
            (ctx_ptr, dh)
        };

        let inner = Arc::new(IsoInner {
            ctx,
            dev_handle,
            stop: AtomicBool::new(false),
            latest_status: RwLock::new(crate::protocol::Status::default()),
            staging,
            bus,
            stats,
            iso_stats: Arc::clone(&iso_stats),
            waveforms: Arc::clone(&waveforms),
            seq_counter: AtomicU64::new(0),
            out_seq_counter: AtomicU8::new(0),
            out_seq_sent_ns: std::array::from_fn(|_| AtomicI64::new(0)),
            last_urb_ns: AtomicI64::new(0),
            telemetry_detail_enabled: AtomicBool::new(true),
            start: Instant::now(),
        });

        let waveform_dev =
            Arc::new(WaveformDevice { dev_handle, _ctx_keepalive: Arc::clone(&inner) });

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
            waveforms,
            waveform_dev: Some(waveform_dev),
            io_thread: Some(io_thread),
        })
    }

    /// Typed control-plane client for the on-chip function generator.
    /// Cheap to clone (Arc internally). All methods are blocking on
    /// libusb EP0 control transfers and must NOT be called from the
    /// iso I/O thread.
    pub fn waveform_dev(&self) -> Arc<WaveformDevice> {
        Arc::clone(
            self.waveform_dev
                .as_ref()
                .expect("WaveformDevice exists while IsoTransport is alive"),
        )
    }

    /// Shared handle to the waveform generator. The web layer mutates
    /// the per-channel specs through this; the iso OUT callback reads
    /// them on every microframe (cheap parking_lot RwLock read).
    pub fn waveforms(&self) -> Arc<WaveformBank> {
        Arc::clone(&self.waveforms)
    }

    /// Get / set the per-packet seq_num + latency tracking flag.
    /// Off saves ~24 k atomic ops/s across the iso OUT+IN hot paths.
    pub fn telemetry_detail(&self) -> bool {
        self.inner.telemetry_detail_enabled.load(Ordering::Relaxed)
    }
    pub fn set_telemetry_detail(&self, on: bool) {
        self.inner.telemetry_detail_enabled.store(on, Ordering::Relaxed);
        // Clear ring so stale timestamps don't produce bogus latency
        // values if the flag is flipped back on after a long pause.
        for slot in self.inner.out_seq_sent_ns.iter() {
            slot.store(0, Ordering::Relaxed);
        }
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

/// Per-thread transfer/buffer resources. The libusb context and
/// dev_handle live in IsoInner now (shared with WaveformDevice).
struct IoResources {
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
        }
    }
}

fn io_thread_main(inner: Arc<IsoInner>) -> Result<()> {
    unsafe {
        // libusb context + dev_handle were opened by IsoTransport::new
        // (main thread) and live in `inner`. We just borrow them here.
        let ctx_ptr = inner.ctx;
        let dev_handle = inner.dev_handle;

        let mut res = IoResources {
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
            let cb_in = Box::new(CallbackCtx { inner: Arc::clone(&inner), dir_in: true });
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
            let cb_out = Box::new(CallbackCtx { inner: Arc::clone(&inner), dir_in: false });
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

    let pkt_descs =
        std::slice::from_raw_parts(xfer.iso_packet_desc.as_ptr(), xfer.num_iso_packets as usize);

    if ctx.dir_in {
        // Compute inter-URB jitter once per transfer. Expected period
        // = 1 ms (PKTS_PER_TRANSFER=8 microframes × 125 us). Same
        // jitter value is fed to all 8 packets' record_ok so the
        // histogram stays at per-packet (8 kHz) granularity but the
        // timing metric reflects the real URB cadence.
        let urb_now_ns = inner.start.elapsed().as_nanos() as i64;
        let prev_urb_ns = inner.last_urb_ns.swap(urb_now_ns, Ordering::Relaxed);
        let urb_jitter_us: i32 = if prev_urb_ns == 0 {
            0
        } else {
            let delta_us = (urb_now_ns - prev_urb_ns) / 1_000;
            (delta_us as i32) - 1000
        };
        for (i, desc) in pkt_descs.iter().enumerate() {
            handle_in_packet(inner, xfer.buffer, i, urb_jitter_us, desc);
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

        // Refresh OUT buffer for the next transfer. Two paths:
        //
        //   * No waveform active → encode the staging snapshot once
        //     and broadcast it to all 8 packets (cheapest case, what
        //     the bulk-mode RtScheduler effectively does).
        //
        //   * Any waveform active → take the staging snapshot for the
        //     non-waveform fields, then re-encode 8 commands with the
        //     waveform-driven channels overridden per-microframe.
        //     This is where 5–8 kHz arbitrary-shape outputs come from.
        let (base_cmd, gen) = inner.staging.take_snapshot();
        let buf_len = (xfer.length as usize).min(PKTS_PER_TRANSFER * ISO_PKT_SIZE);
        let buf = std::slice::from_raw_parts_mut(xfer.buffer, buf_len);

        // Read each waveform once per callback (cheap RwLock read,
        // values copied out so per-packet evaluation needs no lock).
        let dac0_w = *inner.waveforms.dac0.read();
        let dac1_w = *inner.waveforms.dac1.read();
        let pwm0_w = *inner.waveforms.pwm0.read();
        let pwm1_w = *inner.waveforms.pwm1.read();
        let any_active = dac0_w.enabled || dac1_w.enabled || pwm0_w.enabled || pwm1_w.enabled;

        // When detailed telemetry is enabled, bump wire seq_num per
        // packet (8 kHz) and stamp the send timestamp so the IN
        // callback can compute real round-trip latency. When disabled,
        // seq_num stays at whatever base_cmd carries (0 by default)
        // and we save ~16 k atomic ops/s on this hot path.
        let detail = inner.telemetry_detail_enabled.load(Ordering::Relaxed);
        let t0 = if any_active {
            inner.waveforms.next_packet_idx(PKTS_PER_TRANSFER as u64)
        } else {
            0
        };
        const SR_HZ: f32 = (PKTS_PER_TRANSFER * 1000) as f32; // 8000 Hz on HS
        let now_ns = if detail {
            inner.start.elapsed().as_nanos() as i64
        } else {
            0
        };
        for p in 0..PKTS_PER_TRANSFER {
            let off = p * ISO_PKT_SIZE;
            if off + MESSAGE_SIZE > buf.len() {
                break;
            }
            let mut cmd = base_cmd.clone();
            if detail {
                let wire_seq =
                    inner.out_seq_counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
                inner.out_seq_sent_ns[wire_seq as usize].store(now_ns, Ordering::Relaxed);
                cmd.seq_num = wire_seq;
            }
            if any_active {
                let t = t0 + p as u64;
                if dac0_w.enabled {
                    cmd.dac[0] = dac0_w.sample(t, SR_HZ);
                }
                if dac1_w.enabled {
                    cmd.dac[1] = dac1_w.sample(t, SR_HZ);
                }
                if pwm0_w.enabled {
                    cmd.pwm[0] = pwm0_w.sample(t, SR_HZ);
                }
                if pwm1_w.enabled {
                    cmd.pwm[1] = pwm1_w.sample(t, SR_HZ);
                }
            }
            let encoded = encode_command(&cmd);
            buf[off..off + MESSAGE_SIZE].copy_from_slice(&encoded);
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
    urb_jitter_us: i32,
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
    let wire_seq_echo = status.seq_num;
    let frame = StatusFrame {
        status,
        cmd_seq,
        wire_seq: wire_seq_echo,
        tick_index: cmd_seq,
        tick_expected_ns: now_ns,
        tick_sent_ns: now_ns,
        tick_recv_ns: now_ns,
        latency_us: 0, // populated by the iso stats path below, not this per-frame view
        jitter_us: 0,
        missed_ticks_prior: 0,
    };
    inner.bus.publish(frame);

    // Jitter was computed once per URB by the xfer_cb caller and
    // passed in; reuse it for every packet so the histogram stays at
    // per-packet granularity without the bogus inter-packet ~0 delta
    // that comes from libusb's batched callback delivery.
    let jitter_us = urb_jitter_us;

    // Round-trip latency: firmware echoes the seq_num of the last
    // command it applied. Look up when we sent that seq and diff.
    // Gated on telemetry_detail_enabled to match the OUT path —
    // with the flag off seq_num is 0 anyway so the lookup would
    // always miss.
    let latency_us: u32 = if inner.telemetry_detail_enabled.load(Ordering::Relaxed) {
        let sent_ns = inner.out_seq_sent_ns[wire_seq_echo as usize].load(Ordering::Relaxed);
        if sent_ns > 0 && now_ns >= sent_ns {
            (((now_ns - sent_ns) / 1_000) as u32).min(u32::MAX / 2)
        } else {
            0
        }
    } else {
        0
    };

    // Also count it as a "tick" for the existing /api/rt_stats
    // endpoint so the dashboard shows live throughput without
    // dashboard changes (8 kHz successful packets ≈ 8 kHz tick rate).
    inner.stats.record_ok(latency_us, jitter_us);
}

// =========================================================================
//   WaveformDevice — typed control-transfer client for the firmware fn-gen
//
//   Shares the libusb device handle with IsoTransport so that one process
//   can simultaneously stream PhyCMD-64 frames at 8 kHz on the iso EPs
//   AND issue vendor SETUP requests on EP0 to (re)configure on-chip
//   waveform generators. libusb is thread-safe across distinct EP groups
//   so the two paths don't fight.
//
//   Created and owned by IsoTransport; reachable from the web layer via
//   `IsoTransport::waveform_dev()` returning Arc<WaveformDevice>.
// =========================================================================

/// Errors returned by WaveformDevice methods. STALL on the wire becomes
/// `ControlTransferStalled`; everything else is wrapped in `Other`.
#[derive(Debug, thiserror::Error)]
pub enum WaveformError {
    #[error("USB control transfer stalled (firmware rejected the request, e.g. validation failed): bRequest=0x{0:02x}")]
    ControlTransferStalled(u8),
    #[error("USB control transfer error code {0}")]
    ControlTransferFailed(i32),
    #[error("Unexpected payload size: expected {expected}, got {got}")]
    PayloadSize { expected: usize, got: usize },
    #[error("Unknown channel name: {0}")]
    UnknownChannel(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Snapshot returned by `WaveformDevice::caps`, decoded into Rust types
/// for ergonomic JSON serialisation by the web layer.
#[derive(Debug, Clone, Serialize)]
pub struct CapabilitiesView {
    pub protocol_version: u8,
    pub firmware_major: u16,
    pub firmware_minor: u16,
    pub num_dac: u8,
    pub num_pwm: u8,
    pub num_dout: u8,
    pub num_din: u8,
    pub num_adc: u8,
    pub modes_dac: u8,
    pub modes_pwm: u8,
    pub modes_dout: u8,
    pub modes_din: u8,
    pub modes_adc: u8,
    pub max_dac_sample_rate_hz: u32,
    pub max_arb_buffer_samples: u32,
}

/// Snapshot returned by `WaveformDevice::state`. Mirrors `ChannelState`
/// from PROTOCOL.md §2.3 with named fields and JSON-friendly types.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelStateView {
    pub channel_kind: u8,
    pub channel_index: u8,
    pub shape: u8,
    pub shape_name: &'static str,
    pub freq_mhz: u32,
    pub duty_x10: u16,
    pub amplitude: u16,
    pub offset: u16,
    pub phase_offset_x16: u16,
    pub arb_n_samples: u16,
    pub arb_loops_remaining: u16,
    pub arb_sample_rate_hz: u32,
    pub cur_phase_q24_8: u32,
}

fn shape_name(s: u8) -> &'static str {
    match s {
        0 => "off",
        1 => "dc",
        2 => "sine",
        3 => "square",
        4 => "triangle",
        5 => "sawtooth",
        6 => "arbitrary",
        16 => "lut",
        17 => "threshold",
        18 => "pulse_trig",
        19 => "pid",
        _ => "unknown",
    }
}

/// Typed control-plane client for the firmware function generator.
/// Cheap to clone (Arc internally). Methods are all blocking and
/// must NOT be called from the iso I/O thread (would deadlock with
/// libusb_handle_events).
pub struct WaveformDevice {
    dev_handle: *mut ffi::libusb_device_handle,
    /// Held only to keep the libusb context alive — calls are made
    /// directly via dev_handle.
    _ctx_keepalive: Arc<IsoInner>,
}

// SAFETY: libusb_device_handle is documented as thread-safe for
// libusb_control_transfer / libusb_submit_transfer when used on
// distinct endpoints from different threads. We exclusively use EP0
// here while IsoTransport uses EP3/EP4. No data race.
unsafe impl Send for WaveformDevice {}
unsafe impl Sync for WaveformDevice {}

impl WaveformDevice {
    fn ctrl_in(&self, b_request: u8, w_index: u16, length: u16) -> Result<Vec<u8>, WaveformError> {
        let mut buf = vec![0u8; length as usize];
        let n = unsafe {
            ffi::libusb_control_transfer(
                self.dev_handle,
                0xC0, // bmRequestType: vendor IN device
                b_request,
                0, // wValue
                w_index,
                buf.as_mut_ptr(),
                length,
                1000, // timeout ms
            )
        };
        if n < 0 {
            return Err(if n == ffi::constants::LIBUSB_ERROR_PIPE {
                WaveformError::ControlTransferStalled(b_request)
            } else {
                WaveformError::ControlTransferFailed(n)
            });
        }
        buf.truncate(n as usize);
        Ok(buf)
    }

    fn ctrl_out(&self, b_request: u8, w_index: u16, data: &[u8]) -> Result<(), WaveformError> {
        let n = unsafe {
            ffi::libusb_control_transfer(
                self.dev_handle,
                0x40, // bmRequestType: vendor OUT device
                b_request,
                0,
                w_index,
                data.as_ptr() as *mut u8,
                data.len() as u16,
                1000,
            )
        };
        if n < 0 {
            return Err(if n == ffi::constants::LIBUSB_ERROR_PIPE {
                WaveformError::ControlTransferStalled(b_request)
            } else {
                WaveformError::ControlTransferFailed(n)
            });
        }
        if (n as usize) != data.len() {
            return Err(WaveformError::PayloadSize { expected: data.len(), got: n as usize });
        }
        Ok(())
    }

    pub fn caps(&self) -> Result<CapabilitiesView, WaveformError> {
        let raw = self.ctrl_in(VREQ_GEN_GET_CAPS, 0, 32)?;
        if raw.len() < 32 {
            return Err(WaveformError::PayloadSize { expected: 32, got: raw.len() });
        }
        // SAFETY: Capabilities is repr(C, packed) and exactly 32 bytes; layout matches the wire.
        let c: Capabilities = unsafe { std::ptr::read_unaligned(raw.as_ptr() as *const _) };
        Ok(CapabilitiesView {
            protocol_version: c.protocol_version,
            firmware_major: c.firmware_major,
            firmware_minor: c.firmware_minor,
            num_dac: c.num_dac,
            num_pwm: c.num_pwm,
            num_dout: c.num_dout,
            num_din: c.num_din,
            num_adc: c.num_adc,
            modes_dac: c.modes_dac,
            modes_pwm: c.modes_pwm,
            modes_dout: c.modes_dout,
            modes_din: c.modes_din,
            modes_adc: c.modes_adc,
            max_dac_sample_rate_hz: c.max_dac_sample_rate_hz,
            max_arb_buffer_samples: c.max_arb_buffer_samples,
        })
    }

    pub fn state(&self, channel: &str) -> Result<ChannelStateView, WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        let raw = self.ctrl_in(VREQ_GEN_GET_STATE, id, 32)?;
        if raw.len() < 32 {
            return Err(WaveformError::PayloadSize { expected: 32, got: raw.len() });
        }
        let s: ChannelState = unsafe { std::ptr::read_unaligned(raw.as_ptr() as *const _) };
        Ok(ChannelStateView {
            channel_kind: s.channel_kind,
            channel_index: s.channel_index,
            shape: s.shape,
            shape_name: shape_name(s.shape),
            freq_mhz: s.freq_mhz,
            duty_x10: s.duty_x10,
            amplitude: s.amplitude,
            offset: s.offset,
            phase_offset_x16: s.phase_offset_x16,
            arb_n_samples: s.arb_n_samples,
            arb_loops_remaining: s.arb_loops_remaining,
            arb_sample_rate_hz: s.arb_sample_rate_hz,
            cur_phase_q24_8: s.cur_phase_q24_8,
        })
    }

    pub fn stop(&self, channel: &str) -> Result<(), WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        self.ctrl_out(VREQ_GEN_STOP, id, &[])
    }

    pub fn dac_get_clock(&self) -> Result<u32, WaveformError> {
        let raw = self.ctrl_in(VREQ_DAC_GET_CLOCK, 0, 4)?;
        Ok(u32::from_le_bytes(raw[..4].try_into().unwrap()))
    }
    pub fn dac_set_clock(&self, hz: u32) -> Result<(), WaveformError> {
        self.ctrl_out(VREQ_DAC_SET_CLOCK, 0, &hz.to_le_bytes())
    }
    pub fn adc_get_rate(&self) -> Result<u32, WaveformError> {
        let raw = self.ctrl_in(VREQ_ADC_GET_RATE, 0, 4)?;
        Ok(u32::from_le_bytes(raw[..4].try_into().unwrap()))
    }
    pub fn adc_set_rate(&self, hz: u32) -> Result<(), WaveformError> {
        self.ctrl_out(VREQ_ADC_SET_RATE, 0, &hz.to_le_bytes())
    }

    pub fn play_builtin(&self, channel: &str, spec: &WaveBuiltinSpec) -> Result<(), WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        let bytes = unsafe {
            std::slice::from_raw_parts(
                spec as *const _ as *const u8,
                std::mem::size_of::<WaveBuiltinSpec>(),
            )
        };
        self.ctrl_out(VREQ_GEN_PLAY_BUILTIN, id, bytes)
    }

    pub fn play_arbitrary(
        &self,
        channel: &str,
        header: &WaveArbHeader,
        samples: &[i16],
    ) -> Result<(), WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        let mut payload = Vec::with_capacity(8 + samples.len() * 2);
        payload.extend_from_slice(unsafe {
            std::slice::from_raw_parts(header as *const _ as *const u8, 8)
        });
        for s in samples {
            payload.extend_from_slice(&s.to_le_bytes());
        }
        self.ctrl_out(VREQ_GEN_PLAY_ARBITRARY, id, &payload)
    }

    pub fn play_lut(
        &self,
        channel: &str,
        header: &WaveLutSpec,
        entries: &[i16],
    ) -> Result<(), WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        let mut payload = Vec::with_capacity(12 + entries.len() * 2);
        payload.extend_from_slice(unsafe {
            std::slice::from_raw_parts(header as *const _ as *const u8, 12)
        });
        for e in entries {
            payload.extend_from_slice(&e.to_le_bytes());
        }
        self.ctrl_out(VREQ_GEN_PLAY_LUT, id, &payload)
    }

    pub fn play_threshold(
        &self,
        channel: &str,
        spec: &WaveThresholdSpec,
    ) -> Result<(), WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        let bytes = unsafe {
            std::slice::from_raw_parts(
                spec as *const _ as *const u8,
                std::mem::size_of::<WaveThresholdSpec>(),
            )
        };
        self.ctrl_out(VREQ_GEN_PLAY_THRESHOLD, id, bytes)
    }

    pub fn play_pulse_trig(
        &self,
        channel: &str,
        spec: &WavePulseSpec,
    ) -> Result<(), WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        let bytes = unsafe {
            std::slice::from_raw_parts(
                spec as *const _ as *const u8,
                std::mem::size_of::<WavePulseSpec>(),
            )
        };
        self.ctrl_out(VREQ_GEN_PLAY_PULSE_TRIG, id, bytes)
    }

    pub fn play_pid(&self, channel: &str, spec: &WavePidSpec) -> Result<(), WaveformError> {
        let id = channel_id_from_name(channel)
            .ok_or_else(|| WaveformError::UnknownChannel(channel.to_string()))?;
        let bytes = unsafe {
            std::slice::from_raw_parts(
                spec as *const _ as *const u8,
                std::mem::size_of::<WavePidSpec>(),
            )
        };
        self.ctrl_out(VREQ_GEN_PLAY_PID, id, bytes)
    }
}
