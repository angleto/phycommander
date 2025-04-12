"""SQLite persistence layer for measurements, samples, and calibrations.

Schema:

- ``samples``: one row per sample ID (the physical thing being measured,
  e.g. a beer batch, a reaction mixture, a calibration standard).
- ``measurements``: one row per measurement of one parameter on one
  sample at one time. Many rows per sample per session.
- ``calibrations``: one row per calibration curve. Each measurement
  references the calibration used to convert raw readings to physical
  units.

Database file defaults to ``~/.phycommander/analyzer.db``. Override via
the ``Config.database_path`` setting.
"""

from __future__ import annotations

import sqlite3
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Iterator


SCHEMA = """
CREATE TABLE IF NOT EXISTS samples (
    sample_id TEXT PRIMARY KEY,
    description TEXT,
    source TEXT,
    collection_date TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS calibrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mode TEXT NOT NULL,
    channel TEXT NOT NULL,
    method TEXT,
    slope REAL NOT NULL,
    intercept REAL NOT NULL DEFAULT 0.0,
    r_squared REAL,
    standard_lot TEXT,
    operator TEXT,
    date TEXT NOT NULL,
    notes TEXT
);

CREATE INDEX IF NOT EXISTS idx_calibrations_mode_channel
    ON calibrations (mode, channel, date DESC);

CREATE TABLE IF NOT EXISTS measurements (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    sample_id TEXT NOT NULL,
    mode TEXT NOT NULL,
    channel TEXT,
    raw_value REAL,
    calibrated_value REAL,
    unit TEXT,
    calibration_id INTEGER,
    temperature_C REAL,
    operator TEXT,
    notes TEXT,
    FOREIGN KEY (sample_id) REFERENCES samples (sample_id),
    FOREIGN KEY (calibration_id) REFERENCES calibrations (id)
);

CREATE INDEX IF NOT EXISTS idx_measurements_sample
    ON measurements (sample_id, timestamp);

CREATE INDEX IF NOT EXISTS idx_measurements_mode
    ON measurements (mode, timestamp);
"""


@dataclass
class Calibration:
    id: int | None
    mode: str
    channel: str
    method: str
    slope: float
    intercept: float
    r_squared: float | None
    standard_lot: str | None
    operator: str | None
    date: str
    notes: str | None


@dataclass
class Measurement:
    id: int | None
    timestamp: str
    sample_id: str
    mode: str
    channel: str | None
    raw_value: float | None
    calibrated_value: float | None
    unit: str | None
    calibration_id: int | None
    temperature_C: float | None
    operator: str | None
    notes: str | None


class Database:
    """Thin wrapper around SQLite for the analyzer's persistence needs."""

    def __init__(self, path: Path | str) -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._conn: sqlite3.Connection | None = None

    def __enter__(self) -> "Database":
        self.open()
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    def open(self) -> None:
        if self._conn is None:
            self._conn = sqlite3.connect(self.path)
            self._conn.row_factory = sqlite3.Row
            self._conn.executescript(SCHEMA)
            self._conn.commit()

    def close(self) -> None:
        if self._conn is not None:
            self._conn.close()
            self._conn = None

    def conn(self) -> sqlite3.Connection:
        if self._conn is None:
            raise RuntimeError("Database is not open")
        return self._conn

    # -------------------------------------------------------------
    # Samples
    # -------------------------------------------------------------

    def add_sample(
        self,
        sample_id: str,
        description: str = "",
        source: str = "",
        collection_date: str | None = None,
        notes: str = "",
    ) -> None:
        self.conn().execute(
            "INSERT OR REPLACE INTO samples (sample_id, description, source, "
            "collection_date, notes) VALUES (?, ?, ?, ?, ?)",
            (sample_id, description, source, collection_date, notes),
        )
        self.conn().commit()

    # -------------------------------------------------------------
    # Calibrations
    # -------------------------------------------------------------

    def add_calibration(self, cal: Calibration) -> int:
        cur = self.conn().execute(
            "INSERT INTO calibrations (mode, channel, method, slope, intercept, "
            "r_squared, standard_lot, operator, date, notes) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                cal.mode,
                cal.channel,
                cal.method,
                cal.slope,
                cal.intercept,
                cal.r_squared,
                cal.standard_lot,
                cal.operator,
                cal.date,
                cal.notes,
            ),
        )
        self.conn().commit()
        return int(cur.lastrowid or 0)

    def latest_calibration(self, mode: str, channel: str) -> Calibration | None:
        row = self.conn().execute(
            "SELECT * FROM calibrations WHERE mode = ? AND channel = ? "
            "ORDER BY date DESC LIMIT 1",
            (mode, channel),
        ).fetchone()
        if row is None:
            return None
        return Calibration(
            id=row["id"],
            mode=row["mode"],
            channel=row["channel"],
            method=row["method"],
            slope=row["slope"],
            intercept=row["intercept"],
            r_squared=row["r_squared"],
            standard_lot=row["standard_lot"],
            operator=row["operator"],
            date=row["date"],
            notes=row["notes"],
        )

    # -------------------------------------------------------------
    # Measurements
    # -------------------------------------------------------------

    def add_measurement(self, m: Measurement) -> int:
        cur = self.conn().execute(
            "INSERT INTO measurements (timestamp, sample_id, mode, channel, "
            "raw_value, calibrated_value, unit, calibration_id, temperature_C, "
            "operator, notes) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                m.timestamp,
                m.sample_id,
                m.mode,
                m.channel,
                m.raw_value,
                m.calibrated_value,
                m.unit,
                m.calibration_id,
                m.temperature_C,
                m.operator,
                m.notes,
            ),
        )
        self.conn().commit()
        return int(cur.lastrowid or 0)

    def sample_measurements(self, sample_id: str) -> list[Measurement]:
        rows = self.conn().execute(
            "SELECT * FROM measurements WHERE sample_id = ? ORDER BY timestamp",
            (sample_id,),
        ).fetchall()
        return [
            Measurement(
                id=r["id"],
                timestamp=r["timestamp"],
                sample_id=r["sample_id"],
                mode=r["mode"],
                channel=r["channel"],
                raw_value=r["raw_value"],
                calibrated_value=r["calibrated_value"],
                unit=r["unit"],
                calibration_id=r["calibration_id"],
                temperature_C=r["temperature_C"],
                operator=r["operator"],
                notes=r["notes"],
            )
            for r in rows
        ]


def now_iso() -> str:
    """Return the current time as an ISO-8601 UTC string."""
    return datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%S.%fZ")
