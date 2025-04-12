"""Verify that the default modulation-frequency set satisfies the
constraints in ``docs/applications/MULTIFUNCTION_ANALYZER.md`` Appendix A.
"""

from __future__ import annotations

import pytest

from analyzer.lockin import verify_freq_set


# Default set from config/channels.toml and §3.3 of the design doc
DEFAULT_FREQS_HZ = [
    1013.0,  # CH0 — 340 nm (PWM0)
    1709.0,  # CH1 — 430 nm (PWM1)
    571.0,   # CH2 — 470 nm (DOUT0)
    881.0,   # CH3 — 525 nm (DOUT1)
    1223.0,  # CH4 — 590 nm (DOUT2)
    1439.0,  # CH5 — 650 nm laser (DOUT3, interlock-gated)
    1931.0,  # CH6 — 740 nm (DAC0 → comparator)
    2311.0,  # CH7 — 940 nm (DAC1 → comparator)
]


def test_default_set_passes_verification() -> None:
    ok, messages = verify_freq_set(
        DEFAULT_FREQS_HZ,
        integration_s=1.0,
        min_separation_hz=5.0,
        max_harmonic=3,
    )
    assert ok, f"Default set fails verification:\n" + "\n".join(messages)


def test_in_range() -> None:
    for f in DEFAULT_FREQS_HZ:
        assert 200 < f < 2500, f"{f} outside recommended [200, 2500] Hz range"


def test_bad_set_rejected() -> None:
    """A set with a near-duplicate should fail."""
    bad = [1000.0, 1003.0, 1500.0]
    ok, messages = verify_freq_set(bad, min_separation_hz=5.0)
    assert not ok
    assert any("1000" in m for m in messages)


def test_harmonic_collision_rejected() -> None:
    """A set where 2·f_i ≈ f_j should fail."""
    bad = [500.0, 1002.0, 1500.0]  # 2×500 collides with 1002
    ok, messages = verify_freq_set(bad, min_separation_hz=5.0, max_harmonic=3)
    assert not ok
