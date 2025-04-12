"""Measurement mode controllers.

Each mode defines *how* the instrument is configured and how the
lock-in results are interpreted. Modes are independent of assays: a
mode like "absorbance" just reports OD values on one or more
wavelengths, while an assay like "ethanol" uses the absorbance mode
but adds chemistry-specific calibration, reagents, and reporting.
"""

from .base import ModeBase

__all__ = ["ModeBase"]
