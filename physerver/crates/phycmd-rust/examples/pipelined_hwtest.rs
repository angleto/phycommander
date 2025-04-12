//! Quick hardware throughput test for the pipelined USB transport.
//!
//! Opens a `PipelinedUsbLoopbackTransport` against the Step 2
//! firmware, runs the RT scheduler for a configurable duration, and
//! prints effective rate + missed tick stats.
//!
//! Usage:
//! ```text
//! sudo ./target/release/examples/pipelined_hwtest --rate 5000 --duration-ms 10000 --enable-rt
//! ```

use std::env;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use phycmd::{PhyCommander, PipelinedUsbLoopbackTransport, RtConfig, WriteMode};

fn main() -> ExitCode {
    let mut rate_hz: u32 = 5_000;
    let mut duration_ms: u64 = 5_000;
    let mut enable_rt = false;
    let mut miss_budget_pct = 2.0f64;
    let argv: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--rate" => { rate_hz = argv[i+1].parse().unwrap(); i += 2; }
            "--duration-ms" => { duration_ms = argv[i+1].parse().unwrap(); i += 2; }
            "--enable-rt" => { enable_rt = true; i += 1; }
            "--miss-budget-pct" => { miss_budget_pct = argv[i+1].parse().unwrap(); i += 2; }
            _ => { eprintln!("unknown: {}", argv[i]); return ExitCode::from(2); }
        }
    }

    println!("pipelined_hwtest: rate={rate_hz} duration={duration_ms}ms rt={enable_rt}");

    let transport = PipelinedUsbLoopbackTransport::new()
        .expect("open pipelined USB loopback (2341:003e)");

    let config = RtConfig {
        rate_hz,
        default_write_mode: WriteMode::BlockUntilSent,
        enable_rt,
        lock_memory: enable_rt,
        dma_latency_us: None,
        cpu_affinity: None,
        ..Default::default()
    };

    let phy = Arc::new(
        PhyCommander::open_pipelined(config, Box::new(transport))
            .expect("open_pipelined"),
    );

    // Spawn a writer thread that sets dac0 continuously
    let phy2 = Arc::clone(&phy);
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sf = Arc::clone(&stop_flag);
    let writer = thread::spawn(move || {
        let mut n = 0u64;
        while !sf.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = phy2.set_dac0((n & 0xFFF) as u16);
            n += 1;
        }
        n
    });

    let t0 = Instant::now();
    thread::sleep(Duration::from_millis(duration_ms));
    stop_flag.store(true, std::sync::atomic::Ordering::Release);
    let writes = writer.join().unwrap();

    phy.stop();
    let elapsed = t0.elapsed();
    let snap = phy.stats();

    println!("--- results ---");
    println!("  elapsed     : {:.3} s", elapsed.as_secs_f64());
    println!("  tick_count  : {}", snap.tick_count);
    println!("  tick_ok     : {}", snap.tick_ok);
    println!("  missed      : {}", snap.missed_ticks);
    println!("  errors      : {}", snap.transport_errors);
    println!(
        "  effective   : {:.1} Hz ({:.1}%)",
        snap.effective_hz(elapsed.as_secs_f64()),
        snap.effective_hz(elapsed.as_secs_f64()) / rate_hz as f64 * 100.0
    );
    println!("  lat mean    : {:.2} us", snap.mean_latency_us);
    println!("  lat max     : {} us", snap.latency_max_us);
    println!("  jit mean    : {:.2} us", snap.mean_abs_jitter_us);
    println!("  jit range   : {:+}..{:+} us", snap.jitter_min_us, snap.jitter_max_us);
    println!("  writer ops  : {writes}");

    let eff = snap.effective_hz(elapsed.as_secs_f64());
    let missed_pct = snap.missed_ticks as f64 / snap.tick_count.max(1) as f64 * 100.0;
    let lo = rate_hz as f64 * 0.90;
    let hi = rate_hz as f64 * 1.10;

    let pass = eff >= lo && eff <= hi
        && missed_pct < miss_budget_pct
        && snap.transport_errors == 0;

    println!();
    if pass {
        println!("PASS");
        ExitCode::SUCCESS
    } else {
        if eff < lo || eff > hi {
            println!("  FAIL: rate {eff:.1} outside [{lo:.0}, {hi:.0}]");
        }
        if missed_pct >= miss_budget_pct {
            println!("  FAIL: missed {:.2}% >= {miss_budget_pct}%", missed_pct);
        }
        if snap.transport_errors > 0 {
            println!("  FAIL: {} transport errors", snap.transport_errors);
        }
        ExitCode::from(1)
    }
}
