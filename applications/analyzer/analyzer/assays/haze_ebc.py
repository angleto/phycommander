"""Beer haze (EBC turbidity units).

European Brewery Convention method 9.29: forward- or 90°-scatter at
650 nm, calibrated against formazin standards. The 650 nm wavelength is
specifically chosen because most beer chromophores have weak absorption
there, so the scattering signal is dominated by particulate haze.

    EBC_haze = slope × R_scatter

where slope comes from a formazin calibration curve (linear through
origin).
"""

from __future__ import annotations

import logging
from dataclasses import dataclass

from ..config import Config
from ..database import Database, Measurement, now_iso
from ..modes.nephelometry import NephelometryMode


log = logging.getLogger(__name__)


@dataclass
class EBCHazeResult:
    sample_id: str
    ebc_haze: float
    R_scatter: float
    calibration_id: int | None


class Assay:
    assay_name = "haze_ebc"

    def __init__(self, config: Config) -> None:
        self.config = config
        self._mode = NephelometryMode(config, channel_names=("650nm_laser",))

    def run(self, sample_id: str) -> EBCHazeResult:
        print(
            "\n=== Beer haze (EBC 9.29) ===\n"
            "650 nm laser, 90° scatter, formazin-calibrated.\n"
        )
        input(
            "Prep: degas the beer sample but do NOT clarify.\n"
            "      Haze is what you're measuring.\n[Enter]"
        )
        input("STEP 1 — Record scattering background (clean water). [Enter]")
        bg_result = self._mode.run(sample_id + "_bg")
        self._mode.record_background({k: v for k, v in bg_result.raw.items()})

        input("STEP 2 — Insert the beer sample. [Enter]")
        mode_result = self._mode.run(sample_id)
        R_scatter = mode_result.values.get("650nm_laser", 0.0)

        # Apply formazin calibration
        from ..calibration import CalibrationManager

        with Database(self.config.database_path) as db:
            mgr = CalibrationManager(db)
            try:
                cal = mgr.get("haze_ebc", "650nm_laser")
                ebc_haze = mgr.convert("haze_ebc", "650nm_laser", R_scatter)
                cal_id = cal.id
            except RuntimeError as exc:
                log.warning("No formazin calibration: %s", exc)
                ebc_haze = float("nan")
                cal_id = None

        result = EBCHazeResult(
            sample_id=sample_id,
            ebc_haze=ebc_haze,
            R_scatter=R_scatter,
            calibration_id=cal_id,
        )
        self._persist(result)
        print(f"\nRESULT: haze = {ebc_haze:.2f} EBC haze units (raw = {R_scatter:.4g})")
        return result

    def _persist(self, result: EBCHazeResult) -> None:
        with Database(self.config.database_path) as db:
            db.add_sample(result.sample_id)
            db.add_measurement(
                Measurement(
                    id=None,
                    timestamp=now_iso(),
                    sample_id=result.sample_id,
                    mode="nephelometry",
                    channel="650nm_laser",
                    raw_value=result.R_scatter,
                    calibrated_value=result.ebc_haze,
                    unit="EBC haze",
                    calibration_id=result.calibration_id,
                    temperature_C=None,
                    operator=None,
                    notes="EBC 9.29 haze",
                )
            )
