"""
phycmd — hard-real-time Python bindings for PhyCommander.

Thin Python shim that re-exports the compiled Rust extension symbols.
"""

from .phycmd_ext import (  # type: ignore[attr-defined]
    PhyCommander,
    WriteMode,
    __version__,
)

__all__ = ["PhyCommander", "WriteMode", "__version__"]
