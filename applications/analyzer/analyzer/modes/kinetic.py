"""Kinetic absorbance mode.

Tracks absorbance on a selected channel over time and detects the
plateau (for endpoint assays like ADH/NADH ethanol) or the initial
rate (for Michaelis-Menten enzyme assays).

Unlike ``AbsorbanceMode``, which produces one result per call, this
mode runs a long acquisition loop, storing a time series, and emits
periodic updates.
"""

from __future__ import annotations

import logging
import math
import time
from dataclasses import dataclass

import numpy as np

from ..config import Config
from ..phyclient_ipc import IpcClient
from ..lockin import LockinBank
from .base import ModeBase, ModeResult


log = logging.getLogger(__name__)


@dataclass
class KineticTrace:
    """Time series of (t, absorbance) points during a kinetic run."""

    channel_name: str
    t: list[float]
    absorbance: list[float]

    def add(self, t: float, A: float) -> None:
        self.t.append(t)
        self.absorbance.append(A)

    def duration(self) -> float:
        if not self.t:
            return 0.0
        return self.t[-1] - self.t[0]

    def is_plateau(self, window_s: float = 60.0, slope_threshold: float = 0.001) -> bool:
        """Return True when the trailing ``window_s`` seconds have a slope
        whose magnitude is below ``slope_threshold`` OD per minute."""
        if self.duration() < window_s:
            return False
        t_arr = np.asarray(self.t)
        a_arr = np.asarray(self.absorbance)
        mask = t_arr >= (t_arr[-1] - window_s)
        if mask.sum() < 3:
            return False
        slope, _ = np.polyfit(t_arr[mask], a_arr[mask], 1)
        # Convert slope from OD/s to OD/min
        slope_per_min = slope * 60.0
        return abs(slope_per_min) < slope_threshold

    def initial_rate(self, first_seconds: float = 30.0) -> float:
        """Return the slope (OD/s) of the first ``first_seconds`` of the
        trace. Useful for initial-rate enzyme kinetics."""
        if self.duration() < first_seconds or len(self.t) < 3:
            return float("nan")
        t_arr = np.asarray(self.t)
        a_arr = np.asarray(self.absorbance)
        mask = t_arr - t_arr[0] <= first_seconds
        if mask.sum() < 3:
            return float("nan")
        slope, _ = np.polyfit(t_arr[mask], a_arr[mask], 1)
        return float(slope)


class KineticMode(ModeBase):
    mode_name = "kinetic"

    def __init__(
        self,
        config: Config,
        channel_name: str = "340nm",
        max_duration_s: float = 900.0,
    ) -> None:
        super().__init__(config)
        self._primary_channel = channel_name
        self.max_duration_s = max_duration_s
        self.traces: dict[str, KineticTrace] = {}
        self._blank_R: dict[str, float] = {}

    @property
    def channel_names(self) -> tuple[str, ...]:
        # In the default beer use case we only track 340 nm for NADH
        return (self._primary_channel,)

    def run_blank(self) -> None:
        """Record the blank (water cuvette with reagents but no enzyme)
        before starting a kinetic reaction."""
        log.info("Recording kinetic blank on channel %s", self._primary_channel)
        channels = [self.config.get_channel(self._primary_channel)]
        bank = LockinBank(
            freqs_hz=[channels[0].freq_hz],
            fs_hz=self.config.fs_hz,
            integration_s=self.integration_seconds,
            channel_names=(self._primary_channel,),
        )
        with IpcClient(self.config) as client:
            for k, (cmd, status) in enumerate(client.stream()):
                self._populate_command(cmd, bank, channels, k)
                result = bank.step(float(status.adc[0]), t=status.t_received)
                if result is not None:
                    self._blank_R[self._primary_channel] = float(result.R[0])
                    break
        log.info("Blank: R=%.3g", self._blank_R[self._primary_channel])

    def run_kinetic(
        self,
        sample_id: str,
        plateau_window_s: float = 60.0,
        plateau_slope_threshold: float = 0.001,
    ) -> KineticTrace:
        """Run until plateau is detected or ``max_duration_s`` elapses."""
        channels = [self.config.get_channel(self._primary_channel)]
        if self._primary_channel not in self._blank_R:
            raise RuntimeError(
                "Record a blank via run_blank() before starting a kinetic run."
            )
        blank_R = self._blank_R[self._primary_channel]

        bank = LockinBank(
            freqs_hz=[channels[0].freq_hz],
            fs_hz=self.config.fs_hz,
            integration_s=self.integration_seconds,
            channel_names=(self._primary_channel,),
        )

        trace = KineticTrace(channel_name=self._primary_channel, t=[], absorbance=[])
        t0 = time.monotonic()

        with IpcClient(self.config) as client:
            for k, (cmd, status) in enumerate(client.stream()):
                self._populate_command(cmd, bank, channels, k)
                result = bank.step(float(status.adc[0]), t=status.t_received)
                if result is None:
                    continue

                t_rel = status.t_received - t0
                R = float(result.R[0])
                ratio = R / blank_R if blank_R > 0 else 1.0
                A = -math.log10(max(ratio, 1e-12))
                trace.add(t_rel, A)

                log.debug(
                    "t=%.1f s  A340=%.4f  (R=%.3g)",
                    t_rel,
                    A,
                    R,
                )

                if t_rel >= self.max_duration_s:
                    log.info("Kinetic run stopped at max duration.")
                    break
                if trace.is_plateau(plateau_window_s, plateau_slope_threshold):
                    log.info("Plateau detected at t=%.1f s.", t_rel)
                    break

        self.traces[sample_id] = trace
        return trace

    @staticmethod
    def _populate_command(cmd, bank, channels, k: int) -> None:
        dout = 0
        for i, ch in enumerate(channels):
            if ch.gate_kind == "dout_toggle" and bank.dout_bit(i):
                dout |= 1 << ch.gate_index
            elif ch.gate_kind == "dac_compare":
                cmd.dac[ch.gate_index] = bank.dac_sample(i)
            elif ch.gate_kind == "pwm":
                cmd.pwm_freq_hz[ch.gate_index] = int(ch.freq_hz)
                cmd.pwm_duty_u16[ch.gate_index] = 32768
                cmd.pwm_enable[ch.gate_index] = True
        cmd.digital_out = dout
        cmd.seq_num = k & 0xFFFF

    def interpret(self, raw: dict[str, float]) -> ModeResult:
        return ModeResult(
            mode_name=self.mode_name,
            sample_id="",
            values={k: v for k, v in raw.items()},
            units={k: "V_rms (raw)" for k in raw},
            raw=dict(raw),
        )
