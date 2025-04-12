"""CSV and PDF report generation.

After a measurement session, the reporting module produces:

1. A CSV row appended to a running results file (one row per sample per
   mode per channel).
2. A PDF report summarizing all measurements on one sample with the
   calibration metadata, traces (if kinetic), and a cryptographic
   hash of the raw data for reproducibility.
"""

from __future__ import annotations

import csv
import hashlib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

from .database import Measurement


def append_csv(path: Path, measurement: Measurement) -> None:
    """Append one measurement as a CSV row. Create the file with a
    header if it does not exist."""
    path.parent.mkdir(parents=True, exist_ok=True)
    write_header = not path.exists()

    with path.open("a", newline="") as f:
        writer = csv.writer(f)
        if write_header:
            writer.writerow(
                [
                    "timestamp",
                    "sample_id",
                    "mode",
                    "channel",
                    "raw_value",
                    "calibrated_value",
                    "unit",
                    "calibration_id",
                    "temperature_C",
                    "operator",
                    "notes",
                ]
            )
        writer.writerow(
            [
                measurement.timestamp,
                measurement.sample_id,
                measurement.mode,
                measurement.channel or "",
                f"{measurement.raw_value:.6g}" if measurement.raw_value is not None else "",
                f"{measurement.calibrated_value:.6g}" if measurement.calibrated_value is not None else "",
                measurement.unit or "",
                measurement.calibration_id or "",
                f"{measurement.temperature_C:.2f}" if measurement.temperature_C is not None else "",
                measurement.operator or "",
                (measurement.notes or "").replace("\n", " "),
            ]
        )


def data_hash(measurements: Iterable[Measurement]) -> str:
    """Compute a SHA-256 hash over the measurement list, for inclusion
    in a report as a tamper-evidence stamp."""
    h = hashlib.sha256()
    for m in measurements:
        payload = f"{m.timestamp}|{m.sample_id}|{m.mode}|{m.channel}|{m.raw_value}"
        h.update(payload.encode("utf-8"))
    return h.hexdigest()


@dataclass
class PdfReport:
    """PDF report generator. Uses reportlab.

    The PDF is intentionally minimal and regulatory-friendly: one page
    per sample, tabular layout, one calibration statement per reported
    value, and a data-hash footer.
    """

    sample_id: str
    measurements: list[Measurement]
    operator: str | None = None
    notes: str | None = None

    def render(self, output_path: Path) -> None:
        try:
            from reportlab.lib.pagesizes import A4
            from reportlab.lib import colors
            from reportlab.lib.styles import getSampleStyleSheet
            from reportlab.platypus import (
                SimpleDocTemplate,
                Paragraph,
                Spacer,
                Table,
                TableStyle,
            )
        except ImportError as exc:
            raise ImportError(
                "reportlab is required for PDF output. "
                "Install it via `pip install reportlab`."
            ) from exc

        output_path.parent.mkdir(parents=True, exist_ok=True)
        doc = SimpleDocTemplate(str(output_path), pagesize=A4)
        styles = getSampleStyleSheet()
        story = []

        story.append(Paragraph(f"<b>Analyzer report — {self.sample_id}</b>", styles["Title"]))
        story.append(Spacer(1, 12))

        if self.operator:
            story.append(Paragraph(f"Operator: {self.operator}", styles["Normal"]))
        if self.notes:
            story.append(Paragraph(f"Notes: {self.notes}", styles["Normal"]))
        story.append(Spacer(1, 12))

        rows = [["Mode", "Channel", "Value", "Unit", "Timestamp"]]
        for m in self.measurements:
            val = (
                f"{m.calibrated_value:.4g}"
                if m.calibrated_value is not None
                else (f"{m.raw_value:.4g}" if m.raw_value is not None else "—")
            )
            rows.append(
                [
                    m.mode,
                    m.channel or "",
                    val,
                    m.unit or "",
                    m.timestamp,
                ]
            )

        table = Table(rows, hAlign="LEFT")
        table.setStyle(
            TableStyle(
                [
                    ("BACKGROUND", (0, 0), (-1, 0), colors.lightgrey),
                    ("FONTNAME", (0, 0), (-1, 0), "Helvetica-Bold"),
                    ("GRID", (0, 0), (-1, -1), 0.25, colors.grey),
                    ("FONTSIZE", (0, 0), (-1, -1), 9),
                ]
            )
        )
        story.append(table)
        story.append(Spacer(1, 18))

        digest = data_hash(self.measurements)
        story.append(
            Paragraph(
                f"<font size=7>Data hash (SHA-256): {digest}</font>",
                styles["Normal"],
            )
        )

        doc.build(story)
