"""Multi-function analytical bench instrument — host application.

This package drives the phycommander-based multi-function analyzer. It
exposes a multi-channel lock-in engine, per-mode controllers, per-assay
chemistry protocols, a calibration / reporting database, and a live
plotting UI.

See ``docs/applications/MULTIFUNCTION_ANALYZER.md`` in the repository
for the full design reference.
"""

__version__ = "0.1.0"
__all__ = [
    "config",
    "lockin",
    "phyclient_ipc",
    "database",
    "calibration",
    "reporting",
]
