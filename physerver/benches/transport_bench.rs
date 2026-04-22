// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Angelo Leto <angelo@leto.blue>

//! Transport-level throughput benchmarks.
//!
//! The existing `protocol_bench.rs` covers the encode/decode/CRC
//! primitives in isolation. This suite exercises the fuller
//! Transport trait path: a command goes in, a status comes out, and
//! we measure how fast the round-trip pump sustains through the
//! MockTransport loopback (no USB, no I/O thread — pure compute).
//!
//! Numbers here are the lower bound of what the real iso/bulk paths
//! can achieve; they isolate the Rust-side overhead so regressions
//! in the hot path show up before they reach an RT measurement on
//! hardware.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use phycmd_core::transport::{MockTransport, Transport};
use physerver::{Command, CommandFlags};

fn make_cmd(seq: u8) -> Command {
    Command {
        digital_out: 0xA5A5,
        dac: [2047, 3072],
        pwm: [16384, 49152],
        flags: CommandFlags {
            adc_enable: true,
            dac_enable: true,
            pwm_enable: true,
            reset_seq: false,
            watchdog_disable: false,
        },
        seq_num: seq,
    }
}

/// Baseline: how fast can we push commands *through* a transport?
/// The MockTransport loopback owns its own state, so each iteration
/// exercises encode → transport → decode → decode-status.
fn bench_mock_roundtrip(c: &mut Criterion) {
    let mut transport = MockTransport::new();
    let mut seq: u8 = 0;

    c.bench_function("mock_transport_roundtrip", |b| {
        b.iter(|| {
            let cmd = make_cmd(seq);
            transport.send_command(black_box(&cmd)).unwrap();
            let status = transport.receive_status().unwrap();
            seq = seq.wrapping_add(1);
            black_box(status)
        });
    });
}

/// Sustained-rate test: how many roundtrips per wall-clock second can
/// MockTransport do in a tight loop? Useful as a regression canary.
/// The reciprocal of the per-iteration time is the effective Hz.
fn bench_mock_sustained(c: &mut Criterion) {
    let mut group = c.benchmark_group("mock_sustained_batch");
    group.sample_size(20);

    for batch in [100usize, 1_000, 10_000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(batch), batch, |b, &n| {
            b.iter(|| {
                let mut transport = MockTransport::new();
                for i in 0..n {
                    let cmd = make_cmd(i as u8);
                    transport.send_command(black_box(&cmd)).unwrap();
                    let _ = transport.receive_status().unwrap();
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_mock_roundtrip, bench_mock_sustained);
criterion_main!(benches);
