/// Criterion benchmarks for protocol operations
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use physerver::{
    protocol::{crc::crc16_ccitt_table, decode_status, encode_command},
    Command, CommandFlags,
};

fn bench_encode_command(c: &mut Criterion) {
    let cmd = Command {
        digital_out: 0xFFFF,
        dac: [2047, 4095],
        pwm: [32768, 65535],
        flags: CommandFlags {
            adc_enable: true,
            dac_enable: true,
            pwm_enable: true,
            reset_seq: false,
            watchdog_disable: false,
        },
        seq_num: 42,
    };

    c.bench_function("encode_command", |b| b.iter(|| encode_command(black_box(&cmd))));
}

fn bench_decode_status(c: &mut Criterion) {
    let mut data = [0u8; 64];
    data[0] = 0xAA;
    data[1] = 0x55;
    let crc = crc16_ccitt_table(&data[0..24]);
    data[24] = (crc & 0xFF) as u8;
    data[25] = (crc >> 8) as u8;

    c.bench_function("decode_status", |b| b.iter(|| decode_status(black_box(&data)).unwrap()));
}

fn bench_crc_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc16");

    for size in [8, 16, 24, 32, 64].iter() {
        let data = vec![0xAAu8; *size];
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| crc16_ccitt_table(black_box(&data)))
        });
    }

    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let cmd = Command::default();

    c.bench_function("encode_decode_roundtrip", |b| {
        b.iter(|| {
            let bytes = encode_command(black_box(&cmd));
            // In real scenario, we'd decode the response
            black_box(bytes)
        })
    });
}

criterion_group!(
    benches,
    bench_encode_command,
    bench_decode_status,
    bench_crc_calculation,
    bench_roundtrip
);
criterion_main!(benches);
