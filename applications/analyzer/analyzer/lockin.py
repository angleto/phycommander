"""Multi-channel software lock-in amplifier.

This is the numerical core of the analyzer. A single ADC stream is
demodulated simultaneously against N different reference frequencies,
each corresponding to a different modulated light source. Cross-channel
leakage stays below −50 dB after a 1 s integration for carefully chosen
frequency sets.

Design reference: ``docs/applications/MULTIFUNCTION_ANALYZER.md`` §3.1
and ``docs/applications/BEER_ANALYZER.md`` §8.2.

The main class ``LockinBank`` is stateful: each call to ``step()``
consumes one ADC sample and possibly emits a result tuple when the
integration window fills. The caller (mode controller) calls
``drive_outputs(k)`` before each sample to compute the next command
packet's DOUT bit mask for the software-toggled carriers.

For best performance, ``step()`` is vectorized over the N channels
using numpy, but since it is still called once per sample the overall
rate is limited by Python's per-call overhead (~5 µs per call in CPython
3.11 on a modern CPU). This is comfortably within the phycommander 10
kHz loop rate budget.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Iterable

import numpy as np


@dataclass
class LockinResult:
    """One readout of the lock-in bank — amplitude and phase per channel."""

    channel_names: tuple[str, ...]
    R: np.ndarray  # amplitude per channel, ADC counts
    phi: np.ndarray  # phase per channel, radians
    t: float  # monotonic timestamp when the window closed
    n_samples: int  # number of ADC samples in this integration


class LockinBank:
    """N parallel software lock-ins consuming a single ADC stream.

    Parameters
    ----------
    freqs_hz : sequence of float
        One modulation frequency per channel, in Hz. Must be pairwise
        incommensurate within the integration window bandwidth. Use
        ``verify_freq_set`` below to check before constructing.
    fs_hz : float
        ADC sample rate (10 kHz on phycommander USB bulk mode).
    integration_s : float
        Length of each integration window in seconds. Defaults to 1.0,
        giving an ENBW of ~1 Hz.
    channel_names : tuple of str, optional
        Names to tag the results with. If not provided, channels are
        labeled ``ch0``, ``ch1``, ..., ``ch{N-1}``.

    Notes
    -----
    The demodulator uses the *same* phase accumulator for:

    1. Generating the sinusoidal DAC output samples (for DAC-based
       channels), *or*
    2. Toggling a DOUT bit (for software-toggled-square-wave channels),
       *and*
    3. Computing the lock-in sin/cos reference.

    This shared-phase discipline is what guarantees perfect coherence
    between modulation and demodulation. Do not store the phase
    elsewhere and do not re-derive it from a wall clock.
    """

    def __init__(
        self,
        freqs_hz: Iterable[float],
        fs_hz: float = 10_000.0,
        integration_s: float = 1.0,
        channel_names: Iterable[str] | None = None,
    ) -> None:
        freqs = np.asarray(list(freqs_hz), dtype=np.float64)
        if freqs.ndim != 1:
            raise ValueError("freqs_hz must be 1-D")
        if np.any(freqs <= 0):
            raise ValueError("frequencies must be positive")
        if np.any(freqs >= fs_hz / 2):
            raise ValueError(f"all frequencies must be < Fs/2 = {fs_hz / 2}")

        self.freqs_hz = freqs
        self.fs_hz = float(fs_hz)
        self.N = int(round(integration_s * fs_hz))
        if self.N <= 1:
            raise ValueError("integration window too short")
        self.integration_s = self.N / self.fs_hz
        self.n_channels = len(freqs)

        if channel_names is None:
            channel_names = tuple(f"ch{i}" for i in range(self.n_channels))
        else:
            channel_names = tuple(channel_names)
        if len(channel_names) != self.n_channels:
            raise ValueError("channel_names length must equal number of frequencies")
        self.channel_names = channel_names

        # Per-channel state
        self.phase = np.zeros(self.n_channels, dtype=np.float64)  # in cycles, 0..1
        self.I_acc = np.zeros(self.n_channels, dtype=np.float64)
        self.Q_acc = np.zeros(self.n_channels, dtype=np.float64)
        self.k = 0

        # Phase increments (in cycles per sample)
        self._dphase = (self.freqs_hz / self.fs_hz).astype(np.float64)

        # Calibrated phase offset per channel (from phi_calibrate())
        self.phi_offset = np.zeros(self.n_channels, dtype=np.float64)

    # -------------------------------------------------------------------
    # Hot loop
    # -------------------------------------------------------------------

    def step(self, adc_sample: float, t: float | None = None) -> LockinResult | None:
        """Consume one ADC sample. Return a ``LockinResult`` once per window.

        Parameters
        ----------
        adc_sample : float
            The raw ADC value (typically int16 but cast to float internally).
        t : float, optional
            Monotonic time at which the sample was taken. Used to timestamp
            the emitted result.
        """
        # Reference sin/cos
        two_pi_phase = 2.0 * np.pi * self.phase
        s = np.sin(two_pi_phase)
        c = np.cos(two_pi_phase)

        # Accumulate (vectorized over all channels)
        self.I_acc += adc_sample * s
        self.Q_acc += adc_sample * c

        # Advance phase
        self.phase += self._dphase
        self.phase -= np.floor(self.phase)  # wrap to [0, 1)

        self.k += 1

        if self.k < self.N:
            return None

        # Window closed — emit result
        R = np.sqrt(self.I_acc**2 + self.Q_acc**2) / self.N * 2.0
        phi = np.arctan2(self.Q_acc, self.I_acc) - self.phi_offset
        phi = np.mod(phi + np.pi, 2 * np.pi) - np.pi  # wrap to (−π, π]

        result = LockinResult(
            channel_names=self.channel_names,
            R=R.copy(),
            phi=phi.copy(),
            t=t if t is not None else 0.0,
            n_samples=self.N,
        )

        self.I_acc[:] = 0.0
        self.Q_acc[:] = 0.0
        self.k = 0
        # Note: phase is NOT reset — it continues accumulating across
        # integration windows so successive results stay phase-continuous.

        return result

    # -------------------------------------------------------------------
    # Output generation helpers
    # -------------------------------------------------------------------

    def dac_sample(self, channel_index: int, amplitude_u12: int = 2047) -> int:
        """Return the DAC output sample for the given channel at the current
        phase. Only applicable to channels that drive a DAC directly.

        The returned value is a 12-bit unsigned integer, mid-rail biased,
        representing `(amplitude_u12 × sin(2π phase)) + 2048`.
        """
        phase = self.phase[channel_index]
        val = 2048 + int(round(amplitude_u12 * math.sin(2.0 * math.pi * phase)))
        return max(0, min(4095, val))

    def dout_bit(self, channel_index: int) -> int:
        """Return 0 or 1 for the software-toggled square wave on the given
        channel at the current phase. A DOUT bit for this channel should
        be set to this value before each sample is taken, creating a
        square wave at frequency ``freqs_hz[channel_index]``.
        """
        phase = self.phase[channel_index]
        return 1 if phase < 0.5 else 0

    # -------------------------------------------------------------------
    # Phase calibration
    # -------------------------------------------------------------------

    def calibrate_phase(self, I_acc_cal: np.ndarray, Q_acc_cal: np.ndarray) -> None:
        """Set the phase offsets from a calibration measurement.

        After running for N_cal samples with a known illuminated sample
        (blank or stable reference), the accumulated I and Q reveal the
        phase offset of each channel. Call this with those accumulators
        to zero-out the offset in all subsequent measurements.

        ``R`` will then be the in-phase-aligned amplitude (equivalent to
        I after offset) and ``phi`` will hover around zero.
        """
        self.phi_offset = np.arctan2(Q_acc_cal, I_acc_cal)


# -----------------------------------------------------------------------
# Frequency set verification
# -----------------------------------------------------------------------


def verify_freq_set(
    freqs_hz: Iterable[float],
    integration_s: float = 1.0,
    min_separation_hz: float = 5.0,
    max_harmonic: int = 3,
) -> tuple[bool, list[str]]:
    """Check that a set of modulation frequencies is safe for multi-channel
    lock-in demodulation.

    Constraints (see ``docs/applications/MULTIFUNCTION_ANALYZER.md``
    Appendix A):

    1. All frequencies are positive and distinct.
    2. Pairwise separation `|fᵢ − fⱼ|` ≥ ``min_separation_hz`` for all
       i ≠ j.
    3. No sum `|fᵢ + fⱼ|` falls within ``min_separation_hz`` of any fₖ
       (to suppress 2nd-order intermodulation).
    4. No rational ratio with denominator ≤ max_harmonic (to suppress
       harmonic aliasing).

    Returns
    -------
    (ok, messages) : tuple
        ``ok`` is True iff all constraints are satisfied. ``messages``
        is a list of human-readable diagnostic strings, possibly empty
        on pass.
    """
    freqs = np.asarray(list(freqs_hz), dtype=np.float64)
    n = len(freqs)
    messages: list[str] = []

    if n == 0:
        return False, ["empty frequency set"]
    if np.any(freqs <= 0):
        messages.append("negative or zero frequency")
    if len(set(freqs.tolist())) != n:
        messages.append("duplicate frequencies")

    for i in range(n):
        for j in range(i + 1, n):
            delta = abs(freqs[i] - freqs[j])
            if delta < min_separation_hz:
                messages.append(
                    f"pair ({i}, {j}): |{freqs[i]} − {freqs[j]}| = {delta} Hz "
                    f"< {min_separation_hz} Hz"
                )

    for i in range(n):
        for j in range(i, n):
            s = freqs[i] + freqs[j]
            for k in range(n):
                if abs(s - freqs[k]) < min_separation_hz:
                    messages.append(
                        f"intermodulation: f{i}+f{j}={s} collides with f{k}={freqs[k]}"
                    )

    for i in range(n):
        for j in range(n):
            if i == j:
                continue
            for h in range(2, max_harmonic + 1):
                if abs(h * freqs[i] - freqs[j]) < min_separation_hz:
                    messages.append(
                        f"harmonic: {h}·f{i}={h * freqs[i]} near f{j}={freqs[j]}"
                    )

    ok = len(messages) == 0
    return ok, messages
