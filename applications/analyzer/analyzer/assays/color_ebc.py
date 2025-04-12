"""Beer color (EBC units).

European Brewery Convention method (EBC 9.6): measure absorbance of
degassed, clarified beer at 430 nm in a 1 cm cell against water.

    EBC = 25 × A_430

Reference: EBC Analytica, method 9.6.
"""

from __future__ import annotations

from dataclasses import dataclass

from ..config import Config
from ..database import Database, Measurement, now_iso
from ..modes.absorbance import AbsorbanceMode


EBC_FACTOR = 25.0  # A430 × 25 = EBC


@dataclass
class EBCColorResult:
    sample_id: str
    ebc: float
    A430: float


class Assay:
    assay_name = "color_ebc"

    def __init__(self, config: Config) -> None:
        self.config = config
        self._mode = AbsorbanceMode(config)

    def run(self, sample_id: str) -> EBCColorResult:
        print(
            "\n=== Beer color (EBC) ===\n"
            "Official EBC method: A430 × 25 = EBC units\n"
        )
        input(
            "Prep: degas and clarify the beer sample.\n"
            "      If haze > 1 EBC, centrifuge 5 min @ 5000 g or filter 0.45 µm.\n"
            "      Haze interferes with the color measurement.\n[Enter]"
        )
        input("STEP 1 — Blank with distilled water. [Enter]")
        self._mode.run_blank()

        input("STEP 2 — Insert clarified beer sample. [Enter]")
        mode_result = self._mode.run(sample_id)
        A430 = mode_result.values.get("430nm", 0.0)
        ebc = A430 * EBC_FACTOR

        result = EBCColorResult(sample_id=sample_id, ebc=ebc, A430=A430)
        self._persist(result)
        print(f"\nRESULT: color = {ebc:.2f} EBC (A430 = {A430:.4f})")
        return result

    def _persist(self, result: EBCColorResult) -> None:
        with Database(self.config.database_path) as db:
            db.add_sample(result.sample_id)
            db.add_measurement(
                Measurement(
                    id=None,
                    timestamp=now_iso(),
                    sample_id=result.sample_id,
                    mode="absorbance",
                    channel="430nm",
                    raw_value=result.A430,
                    calibrated_value=result.ebc,
                    unit="EBC",
                    calibration_id=None,
                    temperature_C=None,
                    operator=None,
                    notes="EBC 9.6 color",
                )
            )
