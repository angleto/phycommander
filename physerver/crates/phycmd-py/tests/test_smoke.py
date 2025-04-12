"""
Smoke test for the `phycmd` Python module.

Runs a full PhyCommander instance against the MockTransport (no
hardware required), exercises the three write modes, the per-call
override, the observer callback, stats, and clean teardown.

Run with:
    cd crates/phycmd-py
    .venv/bin/maturin develop --release
    .venv/bin/python tests/test_smoke.py
"""

import sys
import threading
import time

import phycmd


# -------------------------------------------------------------------------
#   Test helpers
# -------------------------------------------------------------------------

def assert_that(condition, msg):
    if not condition:
        print(f"  FAIL: {msg}")
        sys.exit(1)
    print(f"  ok:   {msg}")


def main():
    print("phycmd python smoke test")
    print(f"  phycmd version: {phycmd.__version__}")

    # ---------------------------------------------------------------
    #   Construction
    # ---------------------------------------------------------------
    phy = phycmd.PhyCommander.open_mock(
        rate_hz=2000,
        mock_latency_us=100,
        default_mode=phycmd.WriteMode.BlockUntilSent,
    )
    print(f"  constructed: running={phy.is_running()}")
    assert_that(phy.is_running(), "PhyCommander is running after open_mock")
    assert_that(
        phy.default_mode() == phycmd.WriteMode.BlockUntilSent,
        "default_mode is BlockUntilSent",
    )

    # ---------------------------------------------------------------
    #   Basic writes in BlockUntilSent mode
    # ---------------------------------------------------------------
    for i in range(20):
        phy.set_dac0(i * 10)
        phy.set_dac1(i * 20)
    assert_that(phy.is_running(), "still running after 20 writes each on dac0/dac1")

    # ---------------------------------------------------------------
    #   Per-call mode override (Coalesce should not block even when
    #   sticky mode is BlockUntilSent)
    # ---------------------------------------------------------------
    t0 = time.perf_counter()
    for i in range(1000):
        phy.set_dac0(i, mode=phycmd.WriteMode.Coalesce)
    elapsed = time.perf_counter() - t0
    assert_that(
        elapsed < 0.3,
        f"1000 Coalesce writes took {elapsed*1000:.1f} ms (<300 ms budget)",
    )

    # ---------------------------------------------------------------
    #   Digital outputs: per-bit and mass
    # ---------------------------------------------------------------
    for bit in range(16):
        phy.set_digital_out_bit(bit, True)
    phy.set_digital_out(0x5A5A)

    # ---------------------------------------------------------------
    #   Observer callback
    # ---------------------------------------------------------------
    received = []
    received_lock = threading.Lock()

    def on_status(frame):
        with received_lock:
            received.append(frame["cmd_seq"])

    phy.subscribe(on_status)
    time.sleep(0.3)

    with received_lock:
        n = len(received)
        monotonic = all(b > a for a, b in zip(received, received[1:]))
    assert_that(n >= 100, f"observer received ≥100 frames (got {n})")
    assert_that(monotonic, "observer received frames in strictly monotonic cmd_seq order")

    # ---------------------------------------------------------------
    #   Stats
    # ---------------------------------------------------------------
    s = phy.stats()
    print(f"  stats: tick_ok={s['tick_ok']} missed={s['missed_ticks']} "
          f"mean_lat={s['mean_latency_us']:.1f}us "
          f"mean_jit={s['mean_abs_jitter_us']:.1f}us")
    assert_that(s["tick_ok"] > 0, "tick_ok > 0")
    assert_that(s["transport_errors"] == 0, "transport_errors == 0")
    assert_that(
        s["missed_ticks"] < s["tick_count"] // 10,
        "missed_ticks < 10% of tick_count",
    )
    assert_that(isinstance(s["jitter_histogram"], list), "jitter_histogram is a list")
    assert_that(
        len(s["jitter_histogram"]) == 9,
        f"jitter_histogram has 9 buckets (got {len(s['jitter_histogram'])})",
    )

    # ---------------------------------------------------------------
    #   Reset + re-verify stats
    # ---------------------------------------------------------------
    phy.reset_stats()
    s2 = phy.stats()
    assert_that(s2["tick_ok"] == 0, "tick_ok = 0 after reset_stats")

    # ---------------------------------------------------------------
    #   Teardown
    # ---------------------------------------------------------------
    phy.stop()
    time.sleep(0.1)
    assert_that(not phy.is_running(), "PhyCommander stopped cleanly")

    # ---------------------------------------------------------------
    #   Re-open after drop: make sure no resource leak prevents it
    # ---------------------------------------------------------------
    phy2 = phycmd.PhyCommander.open_mock(rate_hz=500, mock_latency_us=100)
    assert_that(phy2.is_running(), "second instance starts")
    phy2.stop()

    print()
    print("PASS")


if __name__ == "__main__":
    main()
