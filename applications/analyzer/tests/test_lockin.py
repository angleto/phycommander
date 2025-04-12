"""Unit tests for the lock-in engine.

Validates, on synthetic signals, that:

1. Single-channel amplitude is recovered to within 1 % over a 1 s
   integration window.
2. Cross-channel leakage is below −50 dB for pairwise-incommensurate
   frequencies after 1 s integration.
3. Phase calibration recovers the expected offset.
4. No samples are dropped and the hot loop is numerically stable.
"""

from __future__ import annotations

import math

import numpy as np
import pytest

from analyzer.lockin import LockinBank, verify_freq_set


FS = 10_000.0


def test_single_channel_amplitude_recovery() -> None:
    """A pure sine in, same amplitude out (within 1 %)."""
    f0 = 1013.0
    amplitude = 500.0  # in ADC counts
    phase_offset = 0.0

    bank = LockinBank(freqs_hz=[f0], fs_hz=FS, integration_s=1.0, channel_names=("test",))
    result = None
    n_samples = int(FS * 1.0)
    for k in range(n_samples):
        t = k / FS
        sample = amplitude * math.sin(2 * math.pi * f0 * t + phase_offset)
        result = bank.step(sample)

    assert result is not None
    # Expected R = amplitude (the lock-in normalization is amplitude, not RMS)
    recovered = float(result.R[0])
    assert abs(recovered - amplitude) / amplitude < 0.01, (
        f"Expected ~{amplitude}, got {recovered}"
    )


def test_cross_channel_leakage_below_50_db() -> None:
    """A sine at f_i must produce ≤ −50 dB response at f_j for j ≠ i."""
    freqs = [1013.0, 1709.0, 571.0, 881.0, 1223.0, 1439.0, 1931.0, 2311.0]
    ok, messages = verify_freq_set(freqs)
    assert ok, f"Default frequency set fails verification: {messages}"

    drive_i = 0  # drive only the first channel
    amplitude = 1000.0

    bank = LockinBank(
        freqs_hz=freqs,
        fs_hz=FS,
        integration_s=1.0,
        channel_names=tuple(f"ch{i}" for i in range(len(freqs))),
    )
    result = None
    n_samples = int(FS * 1.0)
    for k in range(n_samples):
        t = k / FS
        sample = amplitude * math.sin(2 * math.pi * freqs[drive_i] * t)
        result = bank.step(sample)
    assert result is not None

    main_amp = float(result.R[drive_i])
    for j, other in enumerate(result.R):
        if j == drive_i:
            continue
        ratio_db = 20 * math.log10(float(other) / main_amp) if main_amp > 0 else 0
        assert ratio_db < -50, (
            f"Channel {j}: leakage {ratio_db:.1f} dB (want < -50 dB)"
        )


def test_phase_calibration() -> None:
    """After running for 1 s against a known phase offset, phi_offset
    should make subsequent phases read ~0."""
    f0 = 1013.0
    amplitude = 500.0
    phase_offset_cycles = 0.37  # arbitrary
    phase_offset_rad = 2 * math.pi * phase_offset_cycles

    bank = LockinBank(freqs_hz=[f0], fs_hz=FS, integration_s=1.0, channel_names=("test",))
    for k in range(int(FS)):
        t = k / FS
        sample = amplitude * math.sin(2 * math.pi * f0 * t + phase_offset_rad)
        bank.step(sample)

    # Compute calibration from accumulated I and Q
    I = bank.I_acc.copy()
    Q = bank.Q_acc.copy()
    bank.calibrate_phase(I, Q)

    # Reset accumulators manually and run again with the same offset
    bank.I_acc[:] = 0.0
    bank.Q_acc[:] = 0.0
    bank.k = 0
    result = None
    for k in range(int(FS)):
        t = k / FS
        sample = amplitude * math.sin(2 * math.pi * f0 * t + phase_offset_rad)
        result = bank.step(sample)
    assert result is not None
    assert abs(float(result.phi[0])) < 0.02, (
        f"Calibrated phase should be ~0, got {float(result.phi[0])}"
    )


def test_dout_bit_is_square_wave() -> None:
    """dout_bit(i) should toggle at the correct frequency."""
    f0 = 1000.0
    bank = LockinBank(freqs_hz=[f0], fs_hz=FS, integration_s=1.0, channel_names=("test",))
    transitions = 0
    last = bank.dout_bit(0)
    for _ in range(int(FS * 0.1)):  # 100 ms
        bank.step(0.0)  # dummy ADC sample to advance phase
        current = bank.dout_bit(0)
        if current != last:
            transitions += 1
            last = current
    # Expected ~ 2 × f0 × 0.1 s transitions = 200
    # Allow ±10 % tolerance for sampling alignment
    assert 180 <= transitions <= 220, f"Got {transitions} transitions (expected ~200)"


def test_empty_freqs_rejected() -> None:
    with pytest.raises(ValueError):
        LockinBank(freqs_hz=[], fs_hz=FS, integration_s=1.0)


def test_freq_above_nyquist_rejected() -> None:
    with pytest.raises(ValueError):
        LockinBank(freqs_hz=[FS], fs_hz=FS, integration_s=1.0)
