"""Methylene blue Beer-Lambert validation — the first-experiment script.

Interactively prompts the operator through a 7-point serial dilution
of methylene blue, measures A(650) for each, fits the data to
``A = slope × c``, and prints PASS or FAIL against the criteria in
``docs/applications/analyzer/operate/FIRST_EXPERIMENT.md``.

Usage:

    python -m scripts.methylene_blue_run
"""

from __future__ import annotations

import logging
import sys

import numpy as np

from analyzer.calibration import CalibrationManager, forced_origin_fit
from analyzer.config import load_config
from analyzer.database import Database
from analyzer.modes.absorbance import AbsorbanceMode


log = logging.getLogger(__name__)

DILUTION_POINTS_UM = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0]

# Theoretical Beer-Lambert slope at 650 nm for methylene blue
# (epsilon ~ 75 000 M^-1 cm^-1, path 1 cm → 0.075 OD/µM)
EXPECTED_SLOPE_OD_PER_UM = 0.075
SLOPE_TOLERANCE = 0.10  # ±10 %
R_SQUARED_MIN = 0.999
BLANK_MAX_OD = 0.005
MAX_RESIDUAL_OD = 0.01


def prompt(msg: str) -> None:
    input(msg + "\n[press Enter to continue] ")


def main() -> int:
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    config = load_config()

    print("=" * 60)
    print("  First experiment: methylene blue Beer-Lambert validation")
    print("=" * 60)
    print("  Run this once after building the analyzer and before any")
    print("  real sample. Full protocol:")
    print("    docs/applications/analyzer/operate/FIRST_EXPERIMENT.md")
    print()
    prompt(
        "Prepare a 100 µM methylene blue stock and 7 dilution tubes at "
        "concentrations:\n"
        "    0 / 0.5 / 1 / 2 / 5 / 10 / 20 µM\n"
        "Put laser safety glasses on. Close the enclosure. Warm up 15 minutes."
    )

    mode = AbsorbanceMode(config)

    # Record blank (tube 0)
    print("\nSTEP 1 — Insert tube 0 (blank, 0 µM).")
    prompt("Ready?")
    mode.run_blank()

    print("\nSTEP 2 — Measure each sample tube in sequence.")
    raw_A = []
    conc = []
    for c_um in DILUTION_POINTS_UM[1:]:
        prompt(f"Insert tube at {c_um} µM.")
        result = mode.run(sample_id=f"mb_{c_um:g}uM")
        A = result.values.get("650nm_laser", float("nan"))
        print(f"  → A(650) = {A:.4f}")
        raw_A.append(A)
        conc.append(c_um)

    x = np.asarray(conc, dtype=np.float64)
    y = np.asarray(raw_A, dtype=np.float64)

    fit = forced_origin_fit(x, y)
    print("\n" + "=" * 60)
    print("  Regression: A = slope × c")
    print(f"    slope         = {fit.slope:.4f} OD/µM")
    print(f"    R²            = {fit.r_squared:.5f}")
    print(f"    max residual  = {fit.max_residual:.4f} OD")
    print(f"    residual σ    = {fit.residual_stddev:.4f} OD")
    print("=" * 60)

    # Apply pass/fail criteria
    slope_ok = (
        (1 - SLOPE_TOLERANCE) * EXPECTED_SLOPE_OD_PER_UM
        < fit.slope
        < (1 + SLOPE_TOLERANCE) * EXPECTED_SLOPE_OD_PER_UM
    )
    r2_ok = fit.r_squared >= R_SQUARED_MIN
    residual_ok = fit.max_residual <= MAX_RESIDUAL_OD

    overall = slope_ok and r2_ok and residual_ok

    print("\nPass/fail criteria:")
    print(
        f"  slope in ±10 % of {EXPECTED_SLOPE_OD_PER_UM:.3f} OD/µM: "
        f"{'PASS' if slope_ok else 'FAIL'} ({fit.slope:.4f})"
    )
    print(
        f"  R² ≥ {R_SQUARED_MIN}: "
        f"{'PASS' if r2_ok else 'FAIL'} ({fit.r_squared:.5f})"
    )
    print(
        f"  max residual ≤ {MAX_RESIDUAL_OD:.3f} OD: "
        f"{'PASS' if residual_ok else 'FAIL'} ({fit.max_residual:.4f})"
    )

    print()
    if overall:
        print("  OVERALL: PASS — the instrument is validated at 650 nm.")
        # Store the calibration so future runs can use it
        with Database(config.database_path) as db:
            mgr = CalibrationManager(db)
            mgr.store_fit(
                mode="absorbance_linearity",
                channel="650nm_laser",
                method="methylene_blue_serial_dilution",
                fit=fit,
                standard_lot=None,
                operator=None,
                notes=f"DILUTION_POINTS_UM={DILUTION_POINTS_UM}",
            )
            print("  Calibration saved to the database.")
        return 0
    else:
        print("  OVERALL: FAIL — see operate/FIRST_EXPERIMENT.md §7 for diagnosis.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
