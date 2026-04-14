//! Minimal raw test of PipelinedUsbLoopbackTransport.
//! No PhyCommander, no scheduler, no staging — just submit/reap in a loop.

use phycmd::{Command, PipelinedTransport, PipelinedUsbLoopbackTransport};
use std::time::{Duration, Instant};

fn main() {
    let mut t = PipelinedUsbLoopbackTransport::new().expect("open transport");
    println!("transport opened");

    let n = 100;
    let t0 = Instant::now();
    let mut ok = 0u32;
    let mut err = 0u32;

    for i in 0..n {
        let cmd = Command { seq_num: (i & 0xFF) as u8, dac: [i as u16, 0], ..Default::default() };
        if let Err(e) = t.submit(&cmd) {
            eprintln!("submit #{i}: {e}");
            err += 1;
            continue;
        }
        std::thread::sleep(Duration::from_millis(1));
        match t.reap(Duration::from_millis(100)) {
            Ok(st) => {
                if st.seq_num == cmd.seq_num {
                    ok += 1;
                } else {
                    eprintln!("seq mismatch: sent {} got {}", cmd.seq_num, st.seq_num);
                    err += 1;
                }
            }
            Err(e) => {
                eprintln!("reap #{i}: {e}");
                err += 1;
            }
        }
    }

    let elapsed = t0.elapsed();
    println!("{n} exchanges in {:.3}s: ok={ok} err={err}", elapsed.as_secs_f64());
}
