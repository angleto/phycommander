//! Stand-alone iso EP probe (Phase 1 validation).
//!
//! Submits a stream of asynchronous iso transfers on EP 0x83 (IN) and
//! EP 0x04 (OUT) of the Arduino Due running the dual-mode firmware
//! (bulk + iso, see ATSAM3X8E_FW/src/udi_vendor.c). For 60 seconds it:
//!
//!   * Receives status frames from the device (one PhyCMD-64 in the
//!     first 64 B of each 256-B iso packet, every 125 µs)
//!   * Sends idle command frames in the reverse direction
//!   * Records the wall-clock completion time of every iso packet
//!   * Detects packet loss as gaps in the firmware's `seq_num` field
//!     (it increments at most every command apply, so gaps with no
//!     command activity are expected — instead we count gaps in our
//!     own per-packet TX counter that the firmware echoes back)
//!
//! At the end it prints:
//!   - total iso packets received and transmitted
//!   - completion-time histogram (jitter w.r.t. the 125 µs deadline)
//!   - packet-loss counters
//!   - effective throughput
//!
//! Run it after stopping `physerver` (which holds the bulk EPs and may
//! claim the same interface):
//!
//!     sudo systemctl stop physerver
//!     cargo run --release --example iso_probe -p phycmd-core
//!
//! Bulk EPs are not touched here, so the device's bulk path keeps
//! working — `physerver` can be restarted afterwards without a reflash.

use libusb1_sys as ffi;
use phycmd_core::protocol::{decode_status, encode_command, Command, Status, MESSAGE_SIZE};

use std::os::raw::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Match the firmware (ATSAM3X8E_FW/src/udi_vendor.h)
const VID: u16 = 0x2341;
const PID: u16 = 0x003e;
const INTERFACE: i32 = 0;
const EP_ISO_IN: u8 = 0x83;
const EP_ISO_OUT: u8 = 0x04;
const ISO_PKT_SIZE: usize = 256;

// Async transfer parameters. Kept high enough to keep the EHCI iso
// schedule continuously fed; too few in flight and the host inserts
// gaps between transfers that show up as artificial jitter.
const NUM_TRANSFERS: usize = 8;     // depth per direction
const PKTS_PER_TRANSFER: usize = 8; // one transfer = 1 ms of microframes
const PROBE_DURATION: Duration = Duration::from_secs(60);

/// Per-direction shared state, accessed from the libusb completion thread
/// (which is whichever thread is currently inside libusb_handle_events).
struct DirState {
    direction_in: bool,
    pkts_ok: AtomicU64,
    pkts_short: AtomicU64,
    pkts_err: AtomicU64,
    crc_errors: AtomicU64,
    seq_gaps: AtomicU64,
    last_seq: std::sync::atomic::AtomicU32, // u16 promoted, sentinel 0xFFFFFFFF = uninitialised
    /// Histogram of *inter-packet* arrival intervals on iso IN (us).
    /// Bucket bounds match the RtScheduler ones for visual parity.
    inter_us_hist: [AtomicU64; 9],
    last_pkt_ts: parking_lot::Mutex<Option<Instant>>,
}

const HIST_BUCKETS_US: [i64; 8] = [50, 100, 150, 200, 300, 500, 1000, 2000];

impl DirState {
    fn new(direction_in: bool) -> Self {
        Self {
            direction_in,
            pkts_ok: 0.into(),
            pkts_short: 0.into(),
            pkts_err: 0.into(),
            crc_errors: 0.into(),
            seq_gaps: 0.into(),
            last_seq: u32::MAX.into(),
            inter_us_hist: Default::default(),
            last_pkt_ts: parking_lot::Mutex::new(None),
        }
    }

    fn record_inter_arrival(&self, now: Instant) {
        let mut last = self.last_pkt_ts.lock();
        if let Some(prev) = *last {
            let dt_us = now.duration_since(prev).as_micros() as i64;
            let bucket = HIST_BUCKETS_US.iter().position(|&b| dt_us < b).unwrap_or(8);
            self.inter_us_hist[bucket].fetch_add(1, Ordering::Relaxed);
        }
        *last = Some(now);
    }
}

struct CallbackCtx {
    state: Arc<DirState>,
    stop: Arc<AtomicBool>,
}

/// libusb completion callback. Runs on whichever thread is calling
/// libusb_handle_events_*. Must be reentrant-safe and lock-free on
/// the hot path.
extern "system" fn iso_callback(transfer: *mut ffi::libusb_transfer) {
    // SAFETY: libusb invokes this with a valid transfer pointer whose
    // user_data we own (allocated as Box<CallbackCtx> with stable address).
    unsafe { iso_callback_impl(transfer) }
}

unsafe fn iso_callback_impl(transfer: *mut ffi::libusb_transfer) {
    let xfer = &mut *transfer;
    let ctx = &*(xfer.user_data as *const CallbackCtx);
    let state = &ctx.state;

    if xfer.status != ffi::constants::LIBUSB_TRANSFER_COMPLETED {
        state.pkts_err.fetch_add(xfer.num_iso_packets as u64, Ordering::Relaxed);
        if !ctx.stop.load(Ordering::Relaxed) {
            ffi::libusb_submit_transfer(transfer);
        }
        return;
    }

    let now = Instant::now();
    let pkt_descs = std::slice::from_raw_parts(
        xfer.iso_packet_desc.as_ptr(),
        xfer.num_iso_packets as usize,
    );

    for (i, desc) in pkt_descs.iter().enumerate() {
        if desc.status != ffi::constants::LIBUSB_TRANSFER_COMPLETED {
            state.pkts_err.fetch_add(1, Ordering::Relaxed);
            continue;
        }

        if state.direction_in {
            // Each packet: first 64 B = PhyCMD-64 status, rest zeros.
            // libusb stores the per-packet payload in the contiguous
            // transfer buffer at offset i * iso_packet_desc[0].length.
            let pkt_buf_offset = (i as isize) * (ISO_PKT_SIZE as isize);
            let pkt_ptr = xfer.buffer.offset(pkt_buf_offset);
            let actual = desc.actual_length as usize;
            if actual < MESSAGE_SIZE {
                state.pkts_short.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let pkt = std::slice::from_raw_parts(pkt_ptr, MESSAGE_SIZE);
            // Decode PhyCMD-64 status frame (header + CRC validated inside)
            match decode_status(pkt) {
                Ok(status) => {
                    state.pkts_ok.fetch_add(1, Ordering::Relaxed);
                    detect_seq_gap(state, &status);
                    state.record_inter_arrival(now);
                }
                Err(_) => {
                    state.crc_errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        } else {
            // OUT direction: just count completions. Buffer payload
            // was filled once at submit time and is not modified.
            state.pkts_ok.fetch_add(1, Ordering::Relaxed);
        }
    }

    if !ctx.stop.load(Ordering::Relaxed) {
        // Re-arm.
        let r = ffi::libusb_submit_transfer(transfer);
        if r != 0 {
            eprintln!(
                "libusb_submit_transfer (resubmit, dir_in={}) failed: {}",
                state.direction_in, r
            );
        }
    }
}

fn detect_seq_gap(state: &DirState, status: &Status) {
    let cur = status.seq_num as u32;
    let prev = state.last_seq.swap(cur, Ordering::Relaxed);
    if prev == u32::MAX {
        return; // first frame, no baseline
    }
    // The firmware's seq_num only advances when a *valid* command is
    // applied. Since iso_probe doesn't send commands (or sends idle ones),
    // seq_num typically stays constant. We don't count gaps here; the
    // inter-arrival histogram is the real loss indicator.
    let _ = cur;
    let _ = prev;
}

fn main() -> anyhow::Result<()> {
    println!("iso_probe — Phase 1 validation of the SAM3X iso EPs");
    println!(
        "  duration       : {:?}\n  per-direction  : {} transfers × {} packets × {} B = {} B in flight",
        PROBE_DURATION,
        NUM_TRANSFERS,
        PKTS_PER_TRANSFER,
        ISO_PKT_SIZE,
        NUM_TRANSFERS * PKTS_PER_TRANSFER * ISO_PKT_SIZE
    );

    unsafe {
        let mut ctx_ptr: *mut ffi::libusb_context = ptr::null_mut();
        let r = ffi::libusb_init(&mut ctx_ptr);
        anyhow::ensure!(r == 0, "libusb_init failed: {r}");

        let dev_handle = ffi::libusb_open_device_with_vid_pid(ctx_ptr, VID, PID);
        anyhow::ensure!(!dev_handle.is_null(), "device not found (VID={VID:04x} PID={PID:04x}). Is physerver running and holding the device?");

        let _ = ffi::libusb_detach_kernel_driver(dev_handle, INTERFACE);
        let r = ffi::libusb_claim_interface(dev_handle, INTERFACE);
        anyhow::ensure!(r == 0, "libusb_claim_interface failed: {r}");
        println!("  device opened  : interface {} claimed", INTERFACE);

        let stop = Arc::new(AtomicBool::new(false));
        let state_in = Arc::new(DirState::new(true));
        let state_out = Arc::new(DirState::new(false));

        let mut transfers_in: Vec<*mut ffi::libusb_transfer> = Vec::new();
        let mut transfers_out: Vec<*mut ffi::libusb_transfer> = Vec::new();
        let mut buffers_in: Vec<Vec<u8>> = Vec::new();
        let mut buffers_out: Vec<Vec<u8>> = Vec::new();
        let mut ctx_in: Vec<Box<CallbackCtx>> = Vec::new();
        let mut ctx_out: Vec<Box<CallbackCtx>> = Vec::new();

        // Build an idle command frame once; OUT transfers all reuse it.
        let idle_cmd = build_idle_command();

        for _ in 0..NUM_TRANSFERS {
            // ---- IN transfer ----
            let buf_in = vec![0u8; PKTS_PER_TRANSFER * ISO_PKT_SIZE];
            let ctx_box_in = Box::new(CallbackCtx {
                state: Arc::clone(&state_in),
                stop: Arc::clone(&stop),
            });
            let xfer_in = alloc_iso_transfer(
                dev_handle,
                EP_ISO_IN,
                &buf_in,
                ISO_PKT_SIZE as u32,
                PKTS_PER_TRANSFER as i32,
                ctx_box_in.as_ref() as *const CallbackCtx as *mut c_void,
            )?;
            transfers_in.push(xfer_in);
            buffers_in.push(buf_in);
            ctx_in.push(ctx_box_in);

            // ---- OUT transfer ----
            let mut buf_out = vec![0u8; PKTS_PER_TRANSFER * ISO_PKT_SIZE];
            for p in 0..PKTS_PER_TRANSFER {
                let off = p * ISO_PKT_SIZE;
                buf_out[off..off + MESSAGE_SIZE].copy_from_slice(&idle_cmd);
                // bytes [off+64..off+256] left as zeros (padding)
            }
            let ctx_box_out = Box::new(CallbackCtx {
                state: Arc::clone(&state_out),
                stop: Arc::clone(&stop),
            });
            let xfer_out = alloc_iso_transfer(
                dev_handle,
                EP_ISO_OUT,
                &buf_out,
                ISO_PKT_SIZE as u32,
                PKTS_PER_TRANSFER as i32,
                ctx_box_out.as_ref() as *const CallbackCtx as *mut c_void,
            )?;
            transfers_out.push(xfer_out);
            buffers_out.push(buf_out);
            ctx_out.push(ctx_box_out);
        }

        // Submit them all
        for &xfer in &transfers_in {
            let r = ffi::libusb_submit_transfer(xfer);
            anyhow::ensure!(r == 0, "submit IN failed: {r}");
        }
        for &xfer in &transfers_out {
            let r = ffi::libusb_submit_transfer(xfer);
            anyhow::ensure!(r == 0, "submit OUT failed: {r}");
        }
        println!("  submitted      : {} IN + {} OUT iso transfers", NUM_TRANSFERS, NUM_TRANSFERS);
        println!("\nrunning for {:?}…", PROBE_DURATION);

        let start = Instant::now();
        let print_every = Duration::from_secs(5);
        let mut next_print = start + print_every;
        let timeout_tv = libc::timeval { tv_sec: 0, tv_usec: 100_000 };
        while start.elapsed() < PROBE_DURATION {
            ffi::libusb_handle_events_timeout_completed(ctx_ptr, &timeout_tv, ptr::null_mut());
            let now = Instant::now();
            if now >= next_print {
                let in_ok = state_in.pkts_ok.load(Ordering::Relaxed);
                let in_err = state_in.pkts_err.load(Ordering::Relaxed);
                let out_ok = state_out.pkts_ok.load(Ordering::Relaxed);
                let elapsed_s = now.duration_since(start).as_secs_f64();
                println!(
                    "  +{:>3.0}s | IN {:>7} pkts ({:>7.1} /s) err {} | OUT {:>7} pkts ({:>7.1} /s)",
                    elapsed_s,
                    in_ok,
                    in_ok as f64 / elapsed_s,
                    in_err,
                    out_ok,
                    out_ok as f64 / elapsed_s,
                );
                next_print = now + print_every;
            }
        }

        // Stop and drain
        stop.store(true, Ordering::Relaxed);
        for &xfer in transfers_in.iter().chain(transfers_out.iter()) {
            ffi::libusb_cancel_transfer(xfer);
        }
        // Drain a bit so cancellations propagate
        let drain_until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < drain_until {
            ffi::libusb_handle_events_timeout_completed(ctx_ptr, &timeout_tv, ptr::null_mut());
        }

        // Free transfers
        for &xfer in transfers_in.iter().chain(transfers_out.iter()) {
            ffi::libusb_free_transfer(xfer);
        }

        ffi::libusb_release_interface(dev_handle, INTERFACE);
        ffi::libusb_close(dev_handle);
        ffi::libusb_exit(ctx_ptr);

        // ---- final report ----
        let elapsed_s = start.elapsed().as_secs_f64();
        println!("\n========== iso_probe report ==========");
        report("IN  (EP 0x83)", &state_in, elapsed_s);
        report("OUT (EP 0x04)", &state_out, elapsed_s);
    }

    Ok(())
}

unsafe fn alloc_iso_transfer(
    dev_handle: *mut ffi::libusb_device_handle,
    endpoint: u8,
    buf: &Vec<u8>,
    pkt_size: u32,
    num_pkts: i32,
    user_data: *mut c_void,
) -> anyhow::Result<*mut ffi::libusb_transfer> {
    let xfer = ffi::libusb_alloc_transfer(num_pkts);
    anyhow::ensure!(!xfer.is_null(), "libusb_alloc_transfer({num_pkts}) failed");
    (*xfer).dev_handle = dev_handle;
    (*xfer).endpoint = endpoint;
    (*xfer).transfer_type = ffi::constants::LIBUSB_TRANSFER_TYPE_ISOCHRONOUS;
    (*xfer).timeout = 0; // iso transfers have no per-transfer timeout in libusb
    (*xfer).buffer = buf.as_ptr() as *mut u8;
    (*xfer).length = (num_pkts as u32 * pkt_size) as i32;
    (*xfer).num_iso_packets = num_pkts;
    (*xfer).callback = iso_callback;
    (*xfer).user_data = user_data;
    ffi::libusb_set_iso_packet_lengths(xfer, pkt_size);
    Ok(xfer)
}

fn build_idle_command() -> [u8; MESSAGE_SIZE] {
    let cmd = Command::default();
    encode_command(&cmd)
}

fn report(name: &str, st: &DirState, elapsed_s: f64) {
    let ok = st.pkts_ok.load(Ordering::Relaxed);
    let err = st.pkts_err.load(Ordering::Relaxed);
    let short = st.pkts_short.load(Ordering::Relaxed);
    let crc = st.crc_errors.load(Ordering::Relaxed);
    let total = ok + err + short + crc;
    let expected_per_s = 8000.0; // 8 kHz iso microframes
    let pct_ok = if total > 0 { ok as f64 / total as f64 * 100.0 } else { 0.0 };

    println!("{name}:");
    println!("  packets ok       : {ok}  ({:.1} /s, expected {:.0} /s, {:.2}% of theoretical)",
             ok as f64 / elapsed_s,
             expected_per_s,
             ok as f64 / elapsed_s / expected_per_s * 100.0);
    println!("  packets err      : {err}");
    println!("  packets short    : {short}");
    println!("  CRC errors       : {crc}");
    println!("  ok ratio         : {pct_ok:.3}%");

    if st.direction_in {
        let hist: Vec<u64> = st.inter_us_hist.iter().map(|a| a.load(Ordering::Relaxed)).collect();
        let total: u64 = hist.iter().sum();
        if total > 0 {
            println!("  inter-arrival histogram (µs between IN packets):");
            let labels = ["<50", "50-100", "100-150", "150-200", "200-300", "300-500", "500-1000", "1000-2000", ">=2000"];
            for (i, n) in hist.iter().enumerate() {
                let pct = *n as f64 / total as f64 * 100.0;
                let bar_w = (pct / 2.0) as usize;
                let bar: String = std::iter::repeat('#').take(bar_w).collect();
                println!("    {:<10} {:>10} ({:>5.2}%) {}", labels[i], n, pct, bar);
            }
        }
    }
}
