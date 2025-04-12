"""Per-assay chemistry protocols.

Each assay is a small class with a ``run(sample_id)`` method that
encodes the chemistry of a specific analytical test: which reagents,
which dilution, which mode, which calibration, how to report the final
value. The instrument mode (``AbsorbanceMode``, ``KineticMode``, etc.)
does the physics; the assay does the chemistry interpretation.
"""

__all__ = ["ethanol", "bradford", "color_ebc", "haze_ebc", "folin"]
