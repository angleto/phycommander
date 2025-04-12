"""Calibration manager.

Fits linear calibration curves from standard series, stores them in the
database, and retrieves the most recent calibration for use by a mode
or assay controller. Refuses to use calibrations older than a
configurable expiry window.

Each calibration is a linear regression ``y = slope × x + intercept``
where ``x`` is a known standard value (e.g. concentration in µM,
percent ethanol, etc.) and ``y`` is the instrument's raw reading (e.g.
absorbance, raw lock-in R value). To convert a new reading back to
physical units, the application computes ``(y − intercept) / slope``.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timedelta

import numpy as np

from .database import Calibration, Database, now_iso


# Default expiry (days) for each assay's calibration.
DEFAULT_EXPIRY_DAYS: dict[str, int] = {
    "ethanol": 30,
    "color_ebc": 30,
    "haze_ebc": 30,
    "bradford": 14,
    "folin": 30,
    "dns_sugars": 30,
    "iron": 60,
    "absorbance_linearity": 90,
}


@dataclass
class CalibrationFit:
    slope: float
    intercept: float
    r_squared: float
    n_points: int
    residual_stddev: float
    max_residual: float


def linear_fit(x: np.ndarray, y: np.ndarray) -> CalibrationFit:
    """Ordinary least-squares linear regression."""
    x = np.asarray(x, dtype=np.float64)
    y = np.asarray(y, dtype=np.float64)
    if x.shape != y.shape or x.ndim != 1:
        raise ValueError("x and y must be 1-D arrays of the same length")
    if len(x) < 2:
        raise ValueError("need at least 2 points")

    n = len(x)
    xm = x.mean()
    ym = y.mean()
    dx = x - xm
    dy = y - ym
    ss_xx = float(np.sum(dx * dx))
    ss_xy = float(np.sum(dx * dy))
    ss_yy = float(np.sum(dy * dy))

    if ss_xx == 0:
        raise ValueError("all x values are equal")

    slope = ss_xy / ss_xx
    intercept = ym - slope * xm
    y_hat = slope * x + intercept
    resid = y - y_hat
    ss_res = float(np.sum(resid * resid))

    r_squared = 1.0 - ss_res / ss_yy if ss_yy > 0 else 1.0
    residual_stddev = float(np.sqrt(ss_res / max(n - 2, 1)))
    max_residual = float(np.max(np.abs(resid)))

    return CalibrationFit(
        slope=slope,
        intercept=intercept,
        r_squared=r_squared,
        n_points=n,
        residual_stddev=residual_stddev,
        max_residual=max_residual,
    )


def forced_origin_fit(x: np.ndarray, y: np.ndarray) -> CalibrationFit:
    """Linear regression forced through the origin (zero intercept).

    Appropriate for Beer-Lambert calibration where the blank is
    defined to be at c = 0, A = 0 (the blank itself becomes the
    reference baseline).
    """
    x = np.asarray(x, dtype=np.float64)
    y = np.asarray(y, dtype=np.float64)
    if x.shape != y.shape or x.ndim != 1:
        raise ValueError("x and y must be 1-D arrays of the same length")
    if len(x) < 2:
        raise ValueError("need at least 2 points")

    slope = float(np.sum(x * y) / np.sum(x * x))
    y_hat = slope * x
    resid = y - y_hat
    ss_res = float(np.sum(resid * resid))
    ss_tot = float(np.sum(y * y))
    r_squared = 1.0 - ss_res / ss_tot if ss_tot > 0 else 1.0
    return CalibrationFit(
        slope=slope,
        intercept=0.0,
        r_squared=r_squared,
        n_points=len(x),
        residual_stddev=float(np.sqrt(ss_res / max(len(x) - 1, 1))),
        max_residual=float(np.max(np.abs(resid))),
    )


class CalibrationManager:
    """Retrieves calibrations from the database, enforces expiry, and
    provides the raw → physical-units conversion."""

    def __init__(self, db: Database) -> None:
        self.db = db

    def get(self, mode: str, channel: str) -> Calibration:
        cal = self.db.latest_calibration(mode, channel)
        if cal is None:
            raise RuntimeError(
                f"No calibration found for mode={mode!r}, channel={channel!r}. "
                f"Run the calibration wizard first."
            )
        self._check_expiry(mode, cal)
        return cal

    def _check_expiry(self, mode: str, cal: Calibration) -> None:
        expiry_days = DEFAULT_EXPIRY_DAYS.get(mode, 30)
        cal_date = datetime.fromisoformat(cal.date.replace("Z", "+00:00"))
        age = datetime.utcnow().replace(tzinfo=cal_date.tzinfo) - cal_date
        if age > timedelta(days=expiry_days):
            raise RuntimeError(
                f"Calibration for {mode!r}/{cal.channel!r} is {age.days} days old "
                f"(expiry: {expiry_days} days). Recalibrate before reporting values."
            )

    def convert(self, mode: str, channel: str, raw_value: float) -> float:
        cal = self.get(mode, channel)
        if cal.slope == 0:
            raise ValueError(f"Calibration slope is zero for {mode}/{channel}")
        return (raw_value - cal.intercept) / cal.slope

    def store_fit(
        self,
        mode: str,
        channel: str,
        method: str,
        fit: CalibrationFit,
        standard_lot: str | None = None,
        operator: str | None = None,
        notes: str | None = None,
    ) -> int:
        cal = Calibration(
            id=None,
            mode=mode,
            channel=channel,
            method=method,
            slope=fit.slope,
            intercept=fit.intercept,
            r_squared=fit.r_squared,
            standard_lot=standard_lot,
            operator=operator,
            date=now_iso(),
            notes=notes,
        )
        return self.db.add_calibration(cal)
