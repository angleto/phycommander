"""Hardware abstraction layer.

The instrument is modular — different optical heads, electrochemical
modules, and actuator boards can be plugged into the base station.
This package provides the machinery for detecting which modules are
present (by reading their I²C identification EEPROMs) and exposing
their capabilities to the application layer.
"""

from .modules import ModuleType, ModuleInfo, enumerate_modules

__all__ = ["ModuleType", "ModuleInfo", "enumerate_modules"]
