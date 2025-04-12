"""Total polyphenols — Folin-Ciocalteu assay.

Folin-Ciocalteu reagent is reduced by polyphenols to form a blue
complex with absorption maximum at ~765 nm. We measure on CH6 (740 nm)
with ~80 % sensitivity. Expressed as gallic acid equivalents (GAE).

Reference: Singleton, V. L. & Rossi, J. A. (1965) *Am. J. Enol. Vitic.*
16: 144-158.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass

from ..config import Config
from ..database import Database, Measurement, now_iso
from ..modes.absorbance import AbsorbanceMode


log = logging.getLogger(__name__)


@dataclass
class FolinResult:
    sample_id: str
    gae_mg_per_L: float
    A740: float
    calibration_id: int | None


class Assay:
    assay_name = "folin"

    def __init__(self, config: Config, dilution_factor: float = 10.0) -> None:
        self.config = config
        self.dilution_factor = dilution_factor
        self._mode = AbsorbanceMode(config)

    def run(self, sample_id: str) -> FolinResult:
        print(
            "\n=== Polyphenols — Folin-Ciocalteu ===\n"
            "Reagent: Folin-Ciocalteu 2N (Sigma F9252) + 20% Na2CO3\n"
            "Standard: gallic acid, expressed as GAE\n"
        )
        input(
            "STEP 1 — In a cuvette, mix:\n"
            f"  • 0.1 mL sample (diluted 1:{self.dilution_factor:.0f})\n"
            "  • 0.5 mL Folin-C reagent (freshly diluted 1:10)\n"
            "  • 2.0 mL 20% Na2CO3\n"
            "Incubate 30 minutes in the dark at room temperature.\n[Enter]"
        )

        input("STEP 2 — Record blank (reagents + water). [Enter]")
        self._mode.run_blank()

        input("STEP 3 — Insert sample. [Enter]")
        mode_result = self._mode.run(sample_id)
        A740 = mode_result.values.get("740nm", 0.0)

        from ..calibration import CalibrationManager

        with Database(self.config.database_path) as db:
            mgr = CalibrationManager(db)
            try:
                cal = mgr.get("folin", "740nm")
                c = mgr.convert("folin", "740nm", A740) * self.dilution_factor
                cal_id = cal.id
            except RuntimeError as exc:
                log.warning("No folin calibration: %s", exc)
                c = float("nan")
                cal_id = None

        result = FolinResult(
            sample_id=sample_id,
            gae_mg_per_L=c,
            A740=A740,
            calibration_id=cal_id,
        )
        self._persist(result)
        print(f"\nRESULT: polyphenols = {c:.1f} mg/L GAE (A740 = {A740:.4f})")
        return result

    def _persist(self, result: FolinResult) -> None:
        with Database(self.config.database_path) as db:
            db.add_sample(result.sample_id)
            db.add_measurement(
                Measurement(
                    id=None,
                    timestamp=now_iso(),
                    sample_id=result.sample_id,
                    mode="absorbance",
                    channel="740nm",
                    raw_value=result.A740,
                    calibrated_value=result.gae_mg_per_L,
                    unit="mg/L GAE",
                    calibration_id=result.calibration_id,
                    temperature_C=None,
                    operator=None,
                    notes="Folin-Ciocalteu polyphenols",
                )
            )
