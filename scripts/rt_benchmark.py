#!/usr/bin/env python3
"""
rt_benchmark.py — record PhyCMD RT-scheduler / iso-transport metrics.

Polls /api/rt_stats on a running physerver, computes per-window
deltas (so the output is tick-rate / error-rate, not monotonic
cumulatives), and writes a CSV + a short summary.

The script is transport-agnostic: it works equally well against EHCI
and xHCI hosts, and against bulk and iso modes. Its original use case
was the comparative xHCI vs. EHCI characterisation requested in the
project README ("Contributions wanted" → xHCI benchmarking), but it
is also useful for ongoing regression tracking: run it for 10 minutes
before and after a firmware change and diff the summaries.

Usage
-----
  scripts/rt_benchmark.py --host phycmd.local --duration 60
  scripts/rt_benchmark.py --url http://192.168.0.21:8080 --duration 600 --out run.csv

The `summary` block at the end reports:

  effective_hz         mean tick rate over the run
  success_ratio        tick_ok / tick_count
  mean_latency_us      round-trip USB latency (bulk mode) or 0 (iso)
  jitter_mean_abs_us   average |jitter|
  jitter_p99_us        99th-percentile jitter from the server histogram
  iso_in_crc_errors    firmware → host CRC mismatches over the run
  iso_out_errors       host → firmware missed packets over the run

No auth required (the REST endpoints are open on the LAN). If the
server is on a remote host make sure the port is reachable.
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any, Optional


@dataclass
class Snapshot:
    t_mono: float
    tick_count: int
    tick_ok: int
    missed_ticks: int
    transport_errors: int
    mean_latency_us: float
    jitter_min_us: int
    jitter_max_us: int
    mean_abs_jitter_us: float
    jitter_histogram: list[int]
    iso_in_pkts_ok: int
    iso_in_errors: int
    iso_in_crc_errors: int
    iso_out_pkts_ok: int
    iso_out_errors: int


def fetch(url: str) -> dict[str, Any]:
    with urllib.request.urlopen(url, timeout=3) as r:
        return json.loads(r.read().decode("utf-8"))


def snap(base_url: str) -> Snapshot:
    j = fetch(base_url + "/api/rt_stats")
    iso = j.get("iso") or {}
    return Snapshot(
        t_mono=time.monotonic(),
        tick_count=j.get("tick_count", 0),
        tick_ok=j.get("tick_ok", 0),
        missed_ticks=j.get("missed_ticks", 0),
        transport_errors=j.get("transport_errors", 0),
        mean_latency_us=j.get("mean_latency_us", 0.0),
        jitter_min_us=j.get("jitter_min_us", 0),
        jitter_max_us=j.get("jitter_max_us", 0),
        mean_abs_jitter_us=j.get("mean_abs_jitter_us", 0.0),
        jitter_histogram=j.get("jitter_histogram", [0] * 9),
        iso_in_pkts_ok=iso.get("iso_in_pkts_ok", 0),
        iso_in_errors=iso.get("iso_in_errors", 0),
        iso_in_crc_errors=iso.get("iso_in_crc_errors", 0),
        iso_out_pkts_ok=iso.get("iso_out_pkts_ok", 0),
        iso_out_errors=iso.get("iso_out_errors", 0),
    )


def p99_from_histogram(buckets: list[int], bounds_us: list[int]) -> float:
    """Estimate p99 from the server's jitter_histogram + published
    bucket boundaries (see stats.rs::JITTER_BUCKET_BOUNDS_US).
    Returns the upper edge of the bucket that contains cumulative 99%.
    Overflow bucket → inf, encoded as the last bound × 2 for CSV.
    """
    total = sum(buckets)
    if total == 0:
        return 0.0
    threshold = 0.99 * total
    cum = 0
    for i, n in enumerate(buckets):
        cum += n
        if cum >= threshold:
            if i < len(bounds_us):
                return float(bounds_us[i])
            return float(bounds_us[-1]) * 2.0  # overflow, pick a marker
    return float(bounds_us[-1]) * 2.0


BUCKET_BOUNDS_US = [0, 10, 25, 50, 100, 200, 500, 1000]  # matches stats.rs


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--host", default=None, help="physerver host (e.g. phycmd.local)")
    ap.add_argument(
        "--url",
        default=None,
        help="full base URL (takes precedence over --host, e.g. http://...:8080)",
    )
    ap.add_argument("--port", type=int, default=8080, help="physerver port (default 8080)")
    ap.add_argument(
        "--duration",
        type=float,
        default=60.0,
        help="total run length in seconds (default 60)",
    )
    ap.add_argument(
        "--interval",
        type=float,
        default=1.0,
        help="sampling interval in seconds (default 1.0)",
    )
    ap.add_argument(
        "--out",
        default="rt_benchmark.csv",
        help="CSV output path (default rt_benchmark.csv)",
    )
    ap.add_argument("--reset", action="store_true", help="reset telemetry before the run")
    args = ap.parse_args()

    if args.url:
        base = args.url.rstrip("/")
    elif args.host:
        base = f"http://{args.host}:{args.port}"
    else:
        ap.error("one of --url or --host is required")

    if args.reset:
        try:
            req = urllib.request.Request(base + "/api/reset_telemetry", method="POST")
            urllib.request.urlopen(req, timeout=3).read()
        except urllib.error.URLError as e:
            print(f"warning: reset_telemetry failed: {e}", file=sys.stderr)

    # Baseline snapshot at t=0.
    try:
        s0 = snap(base)
    except urllib.error.URLError as e:
        print(f"fatal: cannot reach {base}: {e}", file=sys.stderr)
        return 2

    samples: list[Snapshot] = [s0]
    deadline = time.monotonic() + args.duration
    print(
        f"benchmark: {base}  duration={args.duration}s  interval={args.interval}s",
        file=sys.stderr,
    )

    while time.monotonic() < deadline:
        time.sleep(args.interval)
        try:
            samples.append(snap(base))
        except urllib.error.URLError as e:
            print(f"warning: poll failed: {e}", file=sys.stderr)

    if len(samples) < 2:
        print("fatal: no samples collected", file=sys.stderr)
        return 2

    # Per-window deltas → CSV.
    header = [
        "t_mono_s",
        "window_s",
        "window_tick_count",
        "window_tick_ok",
        "window_missed",
        "window_transport_errors",
        "window_iso_in_pkts_ok",
        "window_iso_in_crc_errors",
        "window_iso_in_errors",
        "window_iso_out_errors",
        "mean_latency_us",
        "mean_abs_jitter_us",
        "jitter_min_us",
        "jitter_max_us",
    ]
    rows: list[list[Any]] = []
    for prev, cur in zip(samples[:-1], samples[1:]):
        dt = cur.t_mono - prev.t_mono
        rows.append(
            [
                f"{cur.t_mono - samples[0].t_mono:.3f}",
                f"{dt:.3f}",
                cur.tick_count - prev.tick_count,
                cur.tick_ok - prev.tick_ok,
                cur.missed_ticks - prev.missed_ticks,
                cur.transport_errors - prev.transport_errors,
                cur.iso_in_pkts_ok - prev.iso_in_pkts_ok,
                cur.iso_in_crc_errors - prev.iso_in_crc_errors,
                cur.iso_in_errors - prev.iso_in_errors,
                cur.iso_out_errors - prev.iso_out_errors,
                f"{cur.mean_latency_us:.2f}",
                f"{cur.mean_abs_jitter_us:.2f}",
                cur.jitter_min_us,
                cur.jitter_max_us,
            ]
        )

    with open(args.out, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(header)
        w.writerows(rows)

    # Summary from full-run deltas.
    first, last = samples[0], samples[-1]
    total_dt = last.t_mono - first.t_mono
    tick_count = last.tick_count - first.tick_count
    tick_ok = last.tick_ok - first.tick_ok
    # Jitter histogram on the server is cumulative; the run's histogram
    # is (last - first) bucket-wise.
    run_hist = [b - a for a, b in zip(first.jitter_histogram, last.jitter_histogram)]
    p99 = p99_from_histogram(run_hist, BUCKET_BOUNDS_US)

    iso_mode = last.iso_in_pkts_ok > first.iso_in_pkts_ok
    summary = {
        "duration_s": round(total_dt, 3),
        "effective_hz": round(tick_ok / total_dt, 1) if total_dt > 0 else 0.0,
        "success_ratio": round(tick_ok / tick_count, 6) if tick_count > 0 else 1.0,
        "mean_latency_us": round(last.mean_latency_us, 2),
        "jitter_mean_abs_us": round(last.mean_abs_jitter_us, 2),
        "jitter_p99_us": round(p99, 1),
        "jitter_min_us": last.jitter_min_us,
        "jitter_max_us": last.jitter_max_us,
        "iso_mode": iso_mode,
        "iso_in_pkts_ok": last.iso_in_pkts_ok - first.iso_in_pkts_ok,
        "iso_in_crc_errors": last.iso_in_crc_errors - first.iso_in_crc_errors,
        "iso_in_errors": last.iso_in_errors - first.iso_in_errors,
        "iso_out_errors": last.iso_out_errors - first.iso_out_errors,
        "csv_rows": len(rows),
        "csv_path": args.out,
    }
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
