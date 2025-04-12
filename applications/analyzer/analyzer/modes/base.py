"""Base class for measurement modes."""

from __future__ import annotations

import abc
import logging
from dataclasses import dataclass

from ..config import Config
from ..database import Database, Measurement, now_iso
from ..phyclient_ipc import IpcClient
from ..lockin import LockinBank


log = logging.getLogger(__name__)


@dataclass
class ModeResult:
    mode_name: str
    sample_id: str
    values: dict[str, float]  # channel_name → calibrated or raw value
    units: dict[str, str]
    raw: dict[str, float]  # channel_name → raw lock-in R


class ModeBase(abc.ABC):
    """Abstract base for all measurement modes.

    A mode:

    1. Configures the lock-in bank with the subset of channels it cares about.
    2. Opens the phyclient IPC.
    3. Enters the hot loop (generate commands, read ADC, step the lock-in).
    4. Collects one or more lock-in results.
    5. Interprets them, optionally applies calibration, and returns a
       ``ModeResult``.
    6. Persists the result to the database.

    Subclasses must implement ``channel_names``, ``integration_seconds``,
    and ``interpret()``.
    """

    mode_name: str = "abstract"

    def __init__(self, config: Config) -> None:
        self.config = config

    @property
    @abc.abstractmethod
    def channel_names(self) -> tuple[str, ...]:
        """Names of the channels this mode uses, referencing config.channels."""
        ...

    @property
    def integration_seconds(self) -> float:
        return self.config.integration_s

    @abc.abstractmethod
    def interpret(self, raw: dict[str, float]) -> ModeResult:
        """Convert raw lock-in amplitudes to a reportable result.

        Subclasses apply blank subtraction, calibration, unit conversion,
        etc. The ``raw`` dict maps channel_name → lock-in R amplitude.
        """
        ...

    def run(self, sample_id: str) -> ModeResult:
        """Run one measurement, persist it, and return the result."""
        channels = [self.config.get_channel(n) for n in self.channel_names]
        bank = LockinBank(
            freqs_hz=[ch.freq_hz for ch in channels],
            fs_hz=self.config.fs_hz,
            integration_s=self.integration_seconds,
            channel_names=self.channel_names,
        )

        with IpcClient(self.config) as client:
            result_tuple = None
            for k, (cmd, status) in enumerate(client.stream()):
                # Compose the DOUT bitmask for software-toggled channels
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

                # Step the lock-in on the primary ADC channel
                adc_value = float(status.adc[channels[0].adc_index])
                result = bank.step(adc_value, t=status.t_received)
                if result is not None:
                    result_tuple = result
                    break

        assert result_tuple is not None
        raw = {
            name: float(R)
            for name, R in zip(result_tuple.channel_names, result_tuple.R)
        }
        mode_result = self.interpret(raw)

        self._persist(sample_id, mode_result)
        return mode_result

    def _persist(self, sample_id: str, result: ModeResult) -> None:
        with Database(self.config.database_path) as db:
            db.add_sample(sample_id)
            for channel_name, value in result.values.items():
                db.add_measurement(
                    Measurement(
                        id=None,
                        timestamp=now_iso(),
                        sample_id=sample_id,
                        mode=self.mode_name,
                        channel=channel_name,
                        raw_value=result.raw.get(channel_name),
                        calibrated_value=value,
                        unit=result.units.get(channel_name),
                        calibration_id=None,
                        temperature_C=None,
                        operator=None,
                        notes=None,
                    )
                )
