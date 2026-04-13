"""
phycmd — hard-real-time Python bindings for PhyCommander.

Thin Python shim that re-exports the compiled Rust extension symbols.

The two top-level classes are:

* :class:`PhyCommander` — full hard-RT streaming stack (iso + command
  staging + status subscribers). Use when you want 8 kHz-updated
  DAC/DOUT control with an observer-pattern callback on every frame.

* :class:`WaveformDev` — lightweight, standalone client for the
  on-chip function generator (firmware-side waveforms up to 1 MSPS,
  plus LUT / threshold / pulse / PID reactive modes). Does not bring
  up the iso scheduler; just opens the device, claims the interface,
  and issues EP0 control transfers. Typically used from quick scripts
  or when ``physerver`` isn't running. Must be closed (via ``with`` or
  explicit ``del``) for the interface to be released.

Example::

    import phycmd
    with phycmd.WaveformDev.open() as dev:
        dev.play_sine("dac0", freq_hz=1000, amplitude=4000, offset=2048)
        dev.stop("dac0")
"""

from .phycmd_ext import (  # type: ignore[attr-defined]
    PhyCommander,
    WriteMode,
    WaveformDev,
    __version__,
)

__all__ = ["PhyCommander", "WriteMode", "WaveformDev", "__version__"]
