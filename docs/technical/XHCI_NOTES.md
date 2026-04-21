# xHCI vs. EHCI benchmarking

The reference PhyCommander host (Intel DN2800MT, Cedar Trail) only
exposes an **EHCI** (USB 2.0) controller. On EHCI, high-speed
isochronous transfers are scheduled at exactly one packet per
microframe (125 µs), so the effective tick rate is hard-capped at
8 kHz. Several of the non-reference deployments listed under
"Contributions wanted" in the repo README use newer boards with an
**xHCI** (USB 3.0+) controller, whose iso scheduler is designed
around per-bus rings rather than per-frame slots and is expected to
push effective rates past 8 kHz with lower jitter.

This note describes the reproducible measurement protocol we use to
characterise either controller. The same script runs identically on
both; the values in the per-host column below are what we measured,
and what we'd like to see reported for any additional host.

## Prerequisites

- A fully configured physerver (iso mode) streaming to a SAM3X with
  the firmware that ships in this repo.
- Python 3.9+ on the measurement host (no extra dependencies; stdlib
  only).
- Ideally a PREEMPT_RT kernel on the measurement host, but the script
  also runs on a vanilla distribution: the delta over a stock kernel
  is useful information in its own right.

## Running the benchmark

```bash
# Baseline: reset counters, stream for 10 minutes, write a CSV.
scripts/rt_benchmark.py --host phycmd.local --duration 600 --reset --out baseline.csv

# Under load: open the dashboard in a browser, run a Python script
# that hammers /api/command at 500 Hz, and capture again.
scripts/rt_benchmark.py --host phycmd.local --duration 600 --out under_load.csv
```

The script polls `/api/rt_stats` once a second and writes per-window
deltas. A JSON summary goes to stdout at the end — save it alongside
the CSV for the run report.

## Interpreting the output

| Column | Meaning | EHCI (reference) | xHCI (target) |
|---|---|---|---|
| `effective_hz` | mean tick rate | 7990 ± 5 Hz (iso), 1000 Hz (bulk) | expect > 8000 Hz |
| `success_ratio` | tick_ok / tick_count | ≥ 0.9995 | ≥ 0.9999 |
| `mean_latency_us` | USB round-trip (bulk only) | 300–400 µs | 80–150 µs (unconfirmed) |
| `jitter_p99_us` | 99th percentile jitter | < 50 µs | < 10 µs |
| `iso_in_crc_errors` | PhyCMD-64 CRC mismatches | 0 | 0 |

### Red flags in a report

- `success_ratio < 0.99`: the scheduler is slipping. Either the
  transport is too slow (bulk over serial) or the host has high
  latency (missing PREEMPT_RT, CPU frequency scaling, busy neighbour).
- `iso_in_crc_errors > 0`: electrical noise on the USB line or the
  iso packet is being truncated. On a short cable this should never
  happen; investigate with `lsusb -v` and `dmesg` first.
- `mean_latency_us` growing monotonically during a run: a transport
  buffer leak. Worth filing an issue.

## What we want in a contribution

If you characterise a new host, please send a PR that adds a row to
the table above with:

1. Host identifier (CPU model + "EHCI" or "xHCI" on the primary
   internal controller).
2. `effective_hz`, `success_ratio`, `jitter_p99_us`, `mean_latency_us`
   from the JSON summary block.
3. The raw CSV from a 10-minute run, committed to
   `docs/technical/benchmarks/<host-tag>.csv`.
4. Any tuning you applied (PREEMPT_RT patch version, CPU governor,
   CPU affinity, `/dev/cpu_dma_latency` setting).

The benchmarking script is transport-agnostic: it does not need to
know whether iso or bulk mode is active, whether the host has
PREEMPT_RT, or what the USB controller is. All of that ends up in
the CSV and summary via the counters exposed by the server, and the
numbers end up directly comparable.
