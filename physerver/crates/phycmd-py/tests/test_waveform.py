"""
Smoke test for phycmd.WaveformDev — requires the Arduino Due to be
plugged in, flashed with v3 firmware (commit 7e97fc9+), and NOT held
by another process (stop physerver first with
``sudo systemctl stop physerver``).

Skips cleanly with exit code 77 if the device is not reachable, so
CI that runs without USB hardware doesn't fail the suite.

Run with:
    sudo systemctl stop physerver
    cd crates/phycmd-py
    .venv/bin/maturin develop --release
    .venv/bin/python tests/test_waveform.py
"""

import struct
import sys
import time

import phycmd


def assert_that(condition, msg):
    if not condition:
        print(f"  FAIL: {msg}")
        sys.exit(1)
    print(f"  ok:   {msg}")


def main():
    print("phycmd.WaveformDev smoke test")

    try:
        dev = phycmd.WaveformDev.open()
    except RuntimeError as e:
        print(f"  SKIP: device not reachable ({e})")
        sys.exit(77)

    try:
        # ---- Capabilities ----
        caps = dev.caps()
        print(f"  caps: {caps}")
        assert_that(caps["protocol_version"] == 1, "protocol_version == 1")
        assert_that(caps["num_dac"] == 2, "num_dac == 2")
        # DAC modes must include at least MANUAL(1)+BUILTIN(2)+ARBITRARY(4)+LUT(8)+THRESHOLD(16)+PID(64) = 0x5F
        assert_that(caps["modes_dac"] & 0x5F == 0x5F,
                    f"modes_dac has all v1+v2+v3 bits (got 0x{caps['modes_dac']:02x})")

        # ---- Initial state ----
        s0 = dev.state("dac0")
        print(f"  state dac0 initial: {s0['shape_name']}")
        assert_that(s0["shape"] == 0, "dac0 starts in SHAPE_OFF")

        # ---- PLAY_SINE ----
        dev.play_sine("dac0", freq_hz=1000, amplitude=4000, offset=2048)
        time.sleep(0.05)
        s1 = dev.state("dac0")
        print(f"  after play_sine: shape={s1['shape_name']}, freq_mhz={s1['freq_mhz']}")
        assert_that(s1["shape"] == 2, f"dac0 shape == SHAPE_SINE (got {s1['shape']})")
        assert_that(s1["freq_mhz"] == 1000000, "freq_mhz == 1_000_000 (= 1 kHz)")

        # ---- Phase advances ----
        p0 = s1["cur_phase_q24_8"]
        time.sleep(0.1)
        s2 = dev.state("dac0")
        print(f"  phase delta in 100ms: 0x{(s2['cur_phase_q24_8']-p0) & 0xFFFFFFFF:08x}")

        # ---- STOP ----
        dev.stop("dac0")
        s3 = dev.state("dac0")
        assert_that(s3["shape"] == 0, "dac0 back to SHAPE_OFF after stop")

        # ---- PLAY_ARBITRARY ----
        samples = [i * 4095 // 15 for i in range(16)]       # linear ramp
        dev.play_arbitrary("dac0", samples=samples, sample_rate_hz=10000, loop_count=0)
        s4 = dev.state("dac0")
        assert_that(s4["shape"] == 6, f"dac0 in SHAPE_ARBITRARY (got {s4['shape']})")
        assert_that(s4["arb_n_samples"] == 16, "arb_n_samples == 16")
        dev.stop("dac0")

        # ---- PLAY_LUT ----
        entries = list(range(0, 4096, 256))     # 16 entries
        dev.play_lut("dac0", input_src="adc", input_arg=0, entries=entries)
        s5 = dev.state("dac0")
        assert_that(s5["shape"] == 16, f"dac0 in SHAPE_LUT (got {s5['shape']})")
        dev.stop("dac0")

        # ---- PLAY_THRESHOLD on DAC0 ----
        dev.play_threshold("dac0", input_src="adc", input_arg=0,
                           thr_high=2100, thr_low=2000, val_high=4095, val_low=0)
        s6 = dev.state("dac0")
        assert_that(s6["shape"] == 17, "dac0 in SHAPE_THRESHOLD")
        dev.stop("dac0")

        # ---- PLAY_PID ----
        dev.play_pid("dac0", input_arg=0, setpoint=2048, kp=0.5, ki=0.001)
        s7 = dev.state("dac0")
        assert_that(s7["shape"] == 19, f"dac0 in SHAPE_PID (got {s7['shape']})")
        dev.stop("dac0")

        # ---- PLAY_PULSE_TRIG on DOUT ----
        dev.play_pulse_trig("dout5", din_bit=0, edge="rising",
                            active_level=1, duration_us=100000, cooldown_us=0)
        s8 = dev.state("dout5")
        assert_that(s8["channel_kind"] == 2, "dout5 channel_kind == DOUT")
        dev.stop("dout5")

        # ---- DAC clock + ADC rate roundtrip ----
        dev.dac_set_clock(500000)
        assert_that(dev.dac_get_clock() == 500000, "dac_clock roundtrip")
        dev.dac_set_clock(1000000)                  # restore

        dev.adc_set_rate(5000)
        assert_that(dev.adc_get_rate() == 5000, "adc_rate roundtrip")

        print()
        print("PASS")
    finally:
        del dev


if __name__ == "__main__":
    main()
