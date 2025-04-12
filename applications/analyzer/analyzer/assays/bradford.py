"""Bradford protein assay.

Coomassie Brilliant Blue G-250 binds to proteins and shifts its
absorbance peak from ~465 nm (free dye) to ~595 nm (bound complex).
The absorbance at 590-595 nm is approximately linear with protein
concentration in the range 10 – 1000 µg/mL.

Reference: Bradford, M. M. (1976) *Anal. Biochem.* 72: 248-254.
Kit: Bio-Rad 500-0006 or equivalent, with BSA standards.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass

from ..config import Config
from ..database import Database, Measurement, now_iso
from ..modes.absorbance import AbsorbanceMode


log = logging.getLogger(__name__)


@dataclass
class BradfordResult:
    sample_id: str
    protein_mg_per_L: float
    A590: float
    calibration_id: int | None


class Assay:
    assay_name = "bradford"

    def __init__(self, config: Config, dilution_factor: float = 5.0) -> None:
        self.config = config
        self.dilution_factor = dilution_factor
        self._mode = AbsorbanceMode(config)

    def run(self, sample_id: str) -> BradfordResult:
        print(
            "\n=== Bradford protein assay ===\n"
            "Reagent: Coomassie G-250 (Bio-Rad 500-0006 or equivalent)\n"
            "Standard: BSA calibration curve required (use --calibrate bradford)\n"
        )
        input(
            "STEP 1 — Mix 0.1 mL of diluted sample with 5.0 mL Bradford reagent.\n"
            "         Invert to mix. Incubate 5 minutes at room temperature.\n"
            "         Insert cuvette.\n[Enter]"
        )

        input("STEP 2 — Record blank (Bradford reagent + water). [Enter]")
        self._mode.run_blank()

        input("STEP 3 — Insert the sample cuvette. [Enter]")
        mode_result = self._mode.run(sample_id)
        A590 = mode_result.values.get("590nm", 0.0)

        # Apply calibration
        from ..calibration import CalibrationManager

        with Database(self.config.database_path) as db:
            mgr = CalibrationManager(db)
            try:
                cal = mgr.get("bradford", "590nm")
                c = mgr.convert("bradford", "590nm", A590) * self.dilution_factor
                cal_id = cal.id
            except RuntimeError as exc:
                log.warning("No calibration for bradford/590nm: %s", exc)
                c = float("nan")
                cal_id = None

        result = BradfordResult(
            sample_id=sample_id,
            protein_mg_per_L=c,
            A590=A590,
            calibration_id=cal_id,
        )
        self._persist(result)
        print(f"\nRESULT: protein = {c:.1f} mg/L (A590 = {A590:.4f})")
        return result

    def _persist(self, result: BradfordResult) -> None:
        with Database(self.config.database_path) as db:
            db.add_sample(result.sample_id)
            db.add_measurement(
                Measurement(
                    id=None,
                    timestamp=now_iso(),
                    sample_id=result.sample_id,
                    mode="absorbance",
                    channel="590nm",
                    raw_value=result.A590,
                    calibrated_value=result.protein_mg_per_L,
                    unit="mg/L",
                    calibration_id=result.calibration_id,
                    temperature_C=None,
                    operator=None,
                    notes="Bradford assay",
                )
            )
