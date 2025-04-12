"""Nephelometry mode — 90° light scattering.

Uses the secondary TIA (on ADC1) behind the 90° port of the optical
head. Same lock-in demodulation as ``AbsorbanceMode``, but the signal
represents *scattered* intensity, not transmitted intensity.

For the beer haze use case, only channel CH5 (650 nm laser) is active.
For multi-angle SLS (see §3.4 of MULTIFUNCTION_ANALYZER.md), several
channels can be active simultaneously, each corresponding to a laser
at a different scattering angle.
"""

from __future__ import annotations

import logging

from ..config import Config
from .base import ModeBase, ModeResult


log = logging.getLogger(__name__)


class NephelometryMode(ModeBase):
    mode_name = "nephelometry"

    def __init__(self, config: Config, channel_names: tuple[str, ...] = ("650nm_laser",)) -> None:
        super().__init__(config)
        self._names = channel_names
        # Map channel_name → raw scatter amplitude recorded as the "blank"
        # (this is the instrumental background with pure water in the
        # cuvette, *not* a sample-specific blank).
        self._background_R: dict[str, float] = {}

    @property
    def channel_names(self) -> tuple[str, ...]:
        return self._names

    def record_background(self, R_values: dict[str, float]) -> None:
        self._background_R.update(R_values)
        log.info("Nephelometer background stored: %s", R_values)

    def interpret(self, raw: dict[str, float]) -> ModeResult:
        values: dict[str, float] = {}
        units: dict[str, str] = {}
        for name, R in raw.items():
            if name in self._background_R:
                # Subtract the instrumental background. The result is
                # proportional to the scattered intensity from the
                # sample — formazin-calibrated units are applied by the
                # assay layer (see assays/haze_ebc.py).
                values[name] = max(R - self._background_R[name], 0.0)
                units[name] = "V_rms (scatter)"
            else:
                values[name] = R
                units[name] = "V_rms (raw, no background)"
        return ModeResult(
            mode_name=self.mode_name,
            sample_id="",
            values=values,
            units=units,
            raw=dict(raw),
        )

    def _persist_adc_channel(self) -> int:
        """Override: nephelometry reads ADC1, not ADC0."""
        return 1
