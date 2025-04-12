"""Ethanol assay — alcohol dehydrogenase enzymatic method.

Reaction:

    CH3CH2OH + NAD+  --ADH-->  CH3CHO + NADH + H+

Monitored at 340 nm (NADH absorption peak). Plateau absorbance is
stoichiometrically proportional to the ethanol concentration in the
reaction mixture. A typical 5 % v/v beer diluted 1:1000 gives a
plateau ΔA340 of ~0.27 in a 1 cm cuvette.

Protocol reference: ``docs/applications/BEER_ANALYZER.md`` §10.1.
Method reference: Megazyme K-ETOH kit insert; Bergmeyer (1974)
*Methods of Enzymatic Analysis*, Vol. 3, p. 1499.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass

from ..config import Config
from ..database import Database, Measurement, now_iso
from ..modes.kinetic import KineticMode, KineticTrace


log = logging.getLogger(__name__)


# Physical constants
NADH_EPSILON_340 = 6220.0  # M^-1 cm^-1
PATH_LENGTH_CM = 1.0
ETHANOL_MW = 46.07  # g/mol
ETHANOL_DENSITY = 0.789  # g/mL at 20 °C


@dataclass
class EthanolResult:
    sample_id: str
    percent_v_v: float
    delta_A340: float
    plateau_time_s: float
    dilution_factor: float
    calibration_lot: str | None
    notes: str | None


class Assay:
    """Ethanol by ADH/NAD+ enzymatic method."""

    assay_name = "ethanol"

    def __init__(
        self,
        config: Config,
        dilution_factor: float = 1000.0,
    ) -> None:
        self.config = config
        self.dilution_factor = dilution_factor
        self._mode = KineticMode(config, channel_name="340nm")

    def prompt(self, message: str) -> None:
        """Interactive prompt for the operator. Override in headless mode."""
        input(message + "\n[press Enter to continue] ")

    def run(self, sample_id: str) -> EthanolResult:
        log.info("Starting ethanol assay for sample %r", sample_id)

        print(
            "\n=== Ethanol enzymatic assay (ADH/NADH) ===\n"
            "This is a kinetic assay. You will need:\n"
            "  - the beer sample (degassed)\n"
            "  - distilled water\n"
            "  - Megazyme K-ETOH kit (or equivalent ADH + NAD+ + PPi buffer)\n"
            "  - a UV-transparent cuvette\n"
            "  - a 1 mL micropipette\n"
        )

        self.prompt(
            "STEP 1 — Prepare the reaction mixture:\n"
            f"  Dilute the beer sample 1:{self.dilution_factor:.0f} in distilled water.\n"
            "  Pipette the following into a UV cuvette:\n"
            "    • 0.1 mL diluted sample\n"
            "    • 2.0 mL PPi buffer (from kit)\n"
            "    • 0.2 mL NAD+ solution (from kit)\n"
            "  Mix by inversion. Insert the cuvette."
        )

        print("\nRecording A340 baseline for 60 s (no enzyme yet)...\n")
        self._mode.run_blank()
        log.info("Baseline captured.")

        self.prompt(
            "STEP 2 — Start the reaction:\n"
            "  Pipette 0.05 mL ADH enzyme into the cuvette.\n"
            "  Mix by inversion. Replace the cuvette immediately."
        )

        print("\nAcquiring kinetic trace until plateau or 15 min...\n")
        trace = self._mode.run_kinetic(sample_id=sample_id)

        plateau_A340 = trace.absorbance[-1]
        baseline_A340 = trace.absorbance[0]
        delta_A = plateau_A340 - baseline_A340
        plateau_time = trace.t[-1]

        # Stoichiometric conversion: NADH produced = ΔA340 / ε
        # Then apply dilution factor to get ethanol in the original beer.
        c_NADH_M = delta_A / (NADH_EPSILON_340 * PATH_LENGTH_CM)  # mol/L in cuvette
        # In the cuvette, 2.35 mL total volume, 0.1 mL was the diluted sample
        # (dilution_factor already applied to the beer). Ethanol concentration
        # in the diluted sample:
        dilution_in_cuvette = 2.35 / 0.1  # 23.5×
        c_EtOH_M_in_diluted = c_NADH_M * dilution_in_cuvette
        # Back to the original beer:
        c_EtOH_M_in_beer = c_EtOH_M_in_diluted * self.dilution_factor
        # Convert to % v/v:
        g_per_L = c_EtOH_M_in_beer * ETHANOL_MW
        mL_per_L = g_per_L / ETHANOL_DENSITY
        percent_v_v = mL_per_L / 10.0  # mL/100 mL

        result = EthanolResult(
            sample_id=sample_id,
            percent_v_v=percent_v_v,
            delta_A340=delta_A,
            plateau_time_s=plateau_time,
            dilution_factor=self.dilution_factor,
            calibration_lot=None,
            notes=None,
        )

        self._persist(result)
        print(f"\nRESULT: ethanol = {percent_v_v:.2f} % v/v")
        print(f"        (ΔA340 = {delta_A:.4f}, plateau at {plateau_time:.0f} s)")
        return result

    def _persist(self, result: EthanolResult) -> None:
        with Database(self.config.database_path) as db:
            db.add_sample(result.sample_id)
            db.add_measurement(
                Measurement(
                    id=None,
                    timestamp=now_iso(),
                    sample_id=result.sample_id,
                    mode="kinetic",
                    channel="340nm",
                    raw_value=result.delta_A340,
                    calibrated_value=result.percent_v_v,
                    unit="% v/v",
                    calibration_id=None,
                    temperature_C=None,
                    operator=None,
                    notes=(
                        f"plateau_time_s={result.plateau_time_s:.0f}, "
                        f"dilution={result.dilution_factor:.0f}"
                    ),
                )
            )
