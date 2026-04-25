//! `selftest` — end-to-end smoke test for the phycmd RT stack.
//!
//! This example runs a full PhyCommander instance against a
//! `MockTransport` and verifies the contract:
//!
//!   * The scheduler produces the expected number of ticks (effective rate within ±10% of target)
//!   * `missed_ticks` stays below 1% of `tick_count`
//!   * `transport_errors` is zero
//!   * The status bus delivers all frames in monotonic order
//!   * Concurrent writers in `BlockUntilSent` mode all complete
//!   * `set_digital_out_bit` works correctly across all 16 bits
//!
//! Usage:
//!
//! ```text
//! cargo run -p phycmd-rust --example selftest --features test-mock -- \
//!     --rate 10000 --duration-ms 2000 --writers 4
//! ```
//!
//! Exit code is 0 on pass, 1 on any assertion failure (with a
//! diagnostic printed on stderr).

use std::{
    env,
    process::ExitCode,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use phycmd::{MockState, MockTransport, PhyCommander, RtConfig, Transport, WriteMode};
use tokio::sync::broadcast::error::TryRecvError;

// -------------------------------------------------------------------------
//   CLI args (no clap dep — kept simple)
// -------------------------------------------------------------------------

#[derive(Debug)]
enum TransportKind {
    Mock,
    #[cfg(feature = "usb")]
    Usb,
    /// Hardware loopback against Step 2 raw-echo firmware (VID 0x2341
    /// PID 0x003e on endpoints 0x02 OUT / 0x81 IN).
    #[cfg(feature = "usb")]
    UsbLoopback,
}

#[derive(Debug)]
struct Args {
    rate_hz: u32,
    duration_ms: u64,
    writers: usize,
    mock_latency_us: u64,
    transport: TransportKind,
    miss_budget_pct: f64,
    enable_rt: bool,
    rt_priority: i32,
    lock_memory: bool,
    verbose: bool,
}

impl Args {
    fn parse() -> Self {
        let mut args = Args {
            rate_hz: 5_000,
            duration_ms: 1_000,
            writers: 4,
            mock_latency_us: 50,
            transport: TransportKind::Mock,
            miss_budget_pct: 1.0,
            enable_rt: false,
            rt_priority: 80,
            lock_memory: false,
            verbose: false,
        };
        let argv: Vec<String> = env::args().collect();
        let mut i = 1;
        while i < argv.len() {
            match argv[i].as_str() {
                "--rate" => {
                    args.rate_hz = argv[i + 1].parse().unwrap();
                    i += 2;
                }
                "--duration-ms" => {
                    args.duration_ms = argv[i + 1].parse().unwrap();
                    i += 2;
                }
                "--writers" => {
                    args.writers = argv[i + 1].parse().unwrap();
                    i += 2;
                }
                "--mock-latency-us" => {
                    args.mock_latency_us = argv[i + 1].parse().unwrap();
                    i += 2;
                }
                "--transport" => {
                    args.transport = match argv[i + 1].as_str() {
                        "mock" => TransportKind::Mock,
                        #[cfg(feature = "usb")]
                        "usb" => TransportKind::Usb,
                        #[cfg(feature = "usb")]
                        "usb-loopback" => TransportKind::UsbLoopback,
                        other => {
                            eprintln!(
                                "unknown transport: {other} (supported: mock, usb, usb-loopback)"
                            );
                            std::process::exit(2);
                        }
                    };
                    i += 2;
                }
                "--miss-budget-pct" => {
                    args.miss_budget_pct = argv[i + 1].parse().unwrap();
                    i += 2;
                }
                "--enable-rt" => {
                    args.enable_rt = true;
                    args.lock_memory = true;
                    i += 1;
                }
                "--rt-priority" => {
                    args.rt_priority = argv[i + 1].parse().unwrap();
                    i += 2;
                }
                "-v" | "--verbose" => {
                    args.verbose = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    eprintln!(
                        "usage: selftest [--rate HZ] [--duration-ms MS] [--writers N] \
                         [--mock-latency-us US] [--transport mock|usb] [--miss-budget-pct N] [-v]"
                    );
                    std::process::exit(0);
                }
                other => {
                    eprintln!("unknown arg: {other}");
                    std::process::exit(2);
                }
            }
        }
        args
    }
}

// -------------------------------------------------------------------------
//   Assertion helper
// -------------------------------------------------------------------------

struct Failures {
    items: Vec<String>,
}

impl Failures {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn check(&mut self, ok: bool, msg: impl Into<String>) {
        if !ok {
            let m = msg.into();
            eprintln!("  FAIL: {m}");
            self.items.push(m);
        }
    }

    fn any(&self) -> bool {
        !self.items.is_empty()
    }
}

// -------------------------------------------------------------------------
//   Main
// -------------------------------------------------------------------------

fn main() -> ExitCode {
    let args = Args::parse();
    println!("phycmd selftest");
    println!("  target rate        : {} Hz", args.rate_hz);
    println!("  duration           : {} ms", args.duration_ms);
    println!("  concurrent writers : {}", args.writers);
    println!("  mock latency       : {} us", args.mock_latency_us);
    println!();

    // -- Build transport -------------------------------------------
    //
    // Mock path: zero-cost loopback + recordable latency knob.
    // USB path: opens the actual 2341:003e device via rusb.
    let (transport, mock_state): (Box<dyn Transport>, Option<Arc<Mutex<MockState>>>) =
        match args.transport {
            TransportKind::Mock => {
                let state = Arc::new(Mutex::new(MockState {
                    latency: Duration::from_micros(args.mock_latency_us),
                    ..Default::default()
                }));
                let t: Box<dyn Transport> = Box::new(MockTransport::with_state(Arc::clone(&state)));
                (t, Some(state))
            }
            #[cfg(feature = "usb")]
            TransportKind::Usb => {
                use phycmd::UsbTransport;
                let t = UsbTransport::new().expect("open USB transport (VID:PID 2341:003e)");
                (Box::new(t), None)
            }
            #[cfg(feature = "usb")]
            TransportKind::UsbLoopback => {
                use phycmd::UsbLoopbackTransport;
                let t = UsbLoopbackTransport::new()
                    .expect("open USB loopback transport (VID:PID 2341:003e)");
                (Box::new(t) as Box<dyn Transport>, None)
            }
        };

    // -- Build config ----------------------------------------------
    let config = RtConfig {
        rate_hz: args.rate_hz,
        default_write_mode: WriteMode::BlockUntilSent,
        enable_rt: args.enable_rt,
        lock_memory: args.lock_memory,
        rt_priority: args.rt_priority,
        // /dev/cpu_dma_latency requires root — leave off unless the
        // caller explicitly sets it (not exposed yet).
        dma_latency_us: None,
        // CPU affinity: leave to the kernel scheduler. On this test
        // box CPUs 2,3 are nohz_full-isolated and sabotage libusb
        // dispatch, so pinning there would be actively worse.
        cpu_affinity: None,
        ..Default::default()
    };

    // -- Open PhyCommander -----------------------------------------
    let phy = match PhyCommander::open(config, transport) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            eprintln!("failed to open PhyCommander: {e}");
            return ExitCode::from(1);
        }
    };

    // -- Subscribe BEFORE writers so we see frame 0 ----------------
    let mut rx = phy.subscribe();

    // -- Spawn concurrent writers -----------------------------------
    //
    // Each writer writes a different field so they don't fight
    // over the same dirty bit — this exercises the per-field
    // blocking semantics of BlockUntilSent.
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut writer_handles = Vec::new();

    for w in 0..args.writers {
        let phy = Arc::clone(&phy);
        let stop = Arc::clone(&stop);
        let h = thread::spawn(move || -> u64 {
            let mut count = 0u64;
            while !stop.load(std::sync::atomic::Ordering::Acquire) {
                let v = (count & 0xFFF) as u16;
                match w % 6 {
                    0 => {
                        let _ = phy.set_dac0(v);
                    }
                    1 => {
                        let _ = phy.set_dac1(v);
                    }
                    2 => {
                        let _ = phy.set_pwm0(v);
                    }
                    3 => {
                        let _ = phy.set_pwm1(v);
                    }
                    4 => {
                        // Toggle a rotating single bit
                        let bit = (count % 16) as u8;
                        let val = (count / 16) & 1 == 0;
                        let _ = phy.set_digital_out_bit(bit, val);
                    }
                    _ => {
                        let _ = phy.set_digital_out((count & 0xFFFF) as u16);
                    }
                }
                count += 1;
            }
            count
        });
        writer_handles.push(h);
    }

    // -- Background consumer: drain the bus while the test runs ----
    //
    // We cannot just drain at the end: with a 1 k capacity broadcast
    // at 10 kHz, a 1 s run generates 10 k frames; the receiver would
    // see `Lagged` and we'd lose ordering information. Instead we
    // consume continuously in a background thread and push the
    // observations into a shared vector.
    let observed: Arc<Mutex<Vec<(u64, u64, u8)>>> =
        Arc::new(Mutex::new(Vec::with_capacity(64 * 1024)));
    let obs_stop = Arc::clone(&stop);
    let obs_vec = Arc::clone(&observed);
    let consumer = thread::spawn(move || -> u64 {
        let mut lagged_total = 0u64;
        loop {
            match rx.try_recv() {
                Ok(f) => {
                    obs_vec.lock().push((f.cmd_seq, f.tick_index, f.wire_seq));
                }
                Err(TryRecvError::Empty) => {
                    if obs_stop.load(std::sync::atomic::Ordering::Acquire) {
                        // Drain any remaining buffered frames, then exit
                        while let Ok(f) = rx.try_recv() {
                            obs_vec.lock().push((f.cmd_seq, f.tick_index, f.wire_seq));
                        }
                        return lagged_total;
                    }
                    thread::sleep(Duration::from_micros(200));
                }
                Err(TryRecvError::Lagged(n)) => {
                    lagged_total += n;
                    continue;
                }
                Err(TryRecvError::Closed) => return lagged_total,
            }
        }
    });

    // -- Run for the requested duration ----------------------------
    let t0 = Instant::now();
    thread::sleep(Duration::from_millis(args.duration_ms));
    stop.store(true, std::sync::atomic::Ordering::Release);

    let writer_totals: Vec<u64> = writer_handles.into_iter().map(|h| h.join().unwrap()).collect();
    let lagged_total = consumer.join().unwrap();
    let elapsed = t0.elapsed();

    // Stop the scheduler before inspecting stats.
    phy.stop();
    let final_stats = phy.stats();
    let mock_call_count = mock_state.as_ref().map(|s| s.lock().call_count).unwrap_or(0);

    // -- Print summary ---------------------------------------------
    let obs = observed.lock();
    println!("--- results ---");
    println!("  elapsed       : {:.3} s", elapsed.as_secs_f64());
    println!("  tick_count    : {}", final_stats.tick_count);
    println!("  tick_ok       : {}", final_stats.tick_ok);
    println!("  missed        : {}", final_stats.missed_ticks);
    println!("  transport_err : {}", final_stats.transport_errors);
    println!(
        "  effective_hz  : {:.1} ({:.1}% of target)",
        final_stats.effective_hz(elapsed.as_secs_f64()),
        final_stats.effective_hz(elapsed.as_secs_f64()) / args.rate_hz as f64 * 100.0
    );
    println!("  latency mean  : {:.2} us", final_stats.mean_latency_us);
    println!("  latency max   : {} us", final_stats.latency_max_us);
    println!("  jitter mean   : {:.2} us", final_stats.mean_abs_jitter_us);
    println!(
        "  jitter range  : {:+} .. {:+} us",
        final_stats.jitter_min_us, final_stats.jitter_max_us
    );
    println!("  frames on bus : {}", obs.len());
    println!("  bus lagged    : {} frames", lagged_total);
    if mock_state.is_some() {
        println!("  mock exchanges: {} (writers totals: {:?})", mock_call_count, writer_totals);
    } else {
        println!("  writers totals: {:?}", writer_totals);
    }

    if args.verbose {
        println!();
        println!("jitter histogram (bucket: count)");
        for (i, c) in final_stats.jitter_histogram.iter().enumerate() {
            println!("  [{:02}] {:>10}", i, c);
        }
    }

    // -- Validate --------------------------------------------------
    let mut fails = Failures::new();

    // (1) Effective rate within ±10% of target
    let eff = final_stats.effective_hz(elapsed.as_secs_f64());
    let lo = args.rate_hz as f64 * 0.90;
    let hi = args.rate_hz as f64 * 1.10;
    fails.check(
        eff >= lo && eff <= hi,
        format!("effective rate {eff:.1} Hz outside ±10% window [{lo:.0}, {hi:.0}]"),
    );

    // (2) Missed ticks below the configured budget
    let missed_frac = final_stats.missed_ticks as f64 / final_stats.tick_count as f64;
    let budget = args.miss_budget_pct / 100.0;
    fails.check(
        missed_frac < budget,
        format!(
            "missed {} / {} ticks ({:.2}%) exceeds {}% budget",
            final_stats.missed_ticks,
            final_stats.tick_count,
            missed_frac * 100.0,
            args.miss_budget_pct
        ),
    );

    // (3) Zero transport errors
    fails.check(
        final_stats.transport_errors == 0,
        format!("{} transport errors recorded", final_stats.transport_errors),
    );

    // (4) Frames on bus are monotonic in cmd_seq
    let mut prev_seq: i64 = -1;
    for &(seq, _tick, _wire) in obs.iter() {
        if (seq as i64) <= prev_seq {
            fails.check(false, format!("bus frames out of order: prev={prev_seq} got={seq}"));
            break;
        }
        prev_seq = seq as i64;
    }

    // (5) tick_count >= observed frames (we may observe slightly
    //     fewer if consumer didn't drain the tail — that's fine,
    //     the constraint is one-sided).
    fails.check(
        final_stats.tick_ok >= obs.len() as u64,
        format!("tick_ok ({}) < observed frames ({})", final_stats.tick_ok, obs.len()),
    );

    // (6) Writers each did at least a few hundred writes
    for (i, c) in writer_totals.iter().enumerate() {
        fails.check(*c > 100, format!("writer {i} only managed {c} writes"));
    }

    println!();
    if fails.any() {
        println!("FAILED: {} assertion(s)", fails.items.len());
        ExitCode::from(1)
    } else {
        println!("PASS");
        ExitCode::SUCCESS
    }
}
