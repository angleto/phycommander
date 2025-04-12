"""Multi-channel absorbance mode.

Measures ``A(λ) = −log10(I_sample(λ) / I_blank(λ))`` for every enabled
channel in the instrument simultaneously. The instrument must have a
blank recorded in the calibration database (mode = "absorbance_blank")
before real samples can be measured.

A typical session:

1. Insert water cuvette, run ``run_blank()`` — stores ``I0`` per channel.
2. Insert sample cuvettes in sequence, each call to ``run(sample_id)``
   returns the 8-channel absorbance vector.
"""

from __future__ import annotations

import math
import logging

import numpy as np

from ..config import Config
from .base import ModeBase, ModeResult


log = logging.getLogger(__name__)


class AbsorbanceMode(ModeBase):
    mode_name = "absorbance"

    def __init__(self, config: Config) -> None:
        super().__init__(config)
        # Each enabled channel
        self._names = tuple(
            ch.name for ch in config.channels if ch.enabled
        )
        # In-memory blank cache. In a full implementation this is loaded
        # from the calibration database at startup and refreshed by
        # run_blank().
        self._blank_R: dict[str, float] = {}

    @property
    def channel_names(self) -> tuple[str, ...]:
        return self._names

    def run_blank(self) -> dict[str, float]:
        """Measure the current sample as the new blank reference.

        Insert a clean water cuvette first, then call this. The
        resulting ``I0`` dict is stored in memory and used as the
        denominator for subsequent ``run()`` calls.
        """
        log.info("Recording new absorbance blank on channels: %s", self._names)
        # Reuse run() but capture the raw dict before interpretation
        channels = [self.config.get_channel(n) for n in self._names]
        from ..lockin import LockinBank
        from ..phyclient_ipc import IpcClient

        bank = LockinBank(
            freqs_hz=[ch.freq_hz for ch in channels],
            fs_hz=self.config.fs_hz,
            integration_s=self.integration_seconds,
            channel_names=self._names,
        )
        with IpcClient(self.config) as client:
            for k, (cmd, status) in enumerate(client.stream()):
                self._populate_command(cmd, bank, channels, k)
                adc_value = float(status.adc[channels[0].adc_index])
                result = bank.step(adc_value, t=status.t_received)
                if result is not None:
                    break
        assert result is not None
        self._blank_R = {name: float(R) for name, R in zip(result.channel_names, result.R)}
        log.info("Blank recorded: %s", self._blank_R)
        return dict(self._blank_R)

    @staticmethod
    def _populate_command(cmd, bank, channels, k: int) -> None:
        """Populate a command packet with outputs for the current phase state."""
        dout = 0
        for i, ch in enumerate(channels):
            if ch.gate_kind == "dout_toggle":
                if bank.dout_bit(i):
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
        values: dict[str, float] = {}
        units: dict[str, str] = {}
        for name, R in raw.items():
            if name in self._blank_R and self._blank_R[name] > 0:
                ratio = R / self._blank_R[name]
                if ratio <= 0:
                    A = math.inf
                else:
                    A = -math.log10(ratio)
                values[name] = A
                units[name] = "OD"
            else:
                # No blank yet — report raw amplitude
                values[name] = R
                units[name] = "V_rms (raw, no blank)"
        return ModeResult(
            mode_name=self.mode_name,
            sample_id="",  # will be filled by base class
            values=values,
            units=units,
            raw=dict(raw),
        )
