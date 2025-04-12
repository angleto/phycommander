"""Configuration loader.

Loads the instrument configuration (wavelengths, modulation frequencies,
driver kinds, physerver connection) from a TOML file. Provides a single
``Config`` dataclass that the rest of the application imports.

Default location: ``config/channels.toml`` relative to the application
package root. Overridable via the ``ANALYZER_CONFIG`` environment
variable.
"""

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass, field
from pathlib import Path


# Default sample rate of phycommander (USB bulk mode at 10 kHz)
FS_DEFAULT_HZ = 10_000.0

# ADC full scale (SAM3X8E 12-bit, 0-3.3 V)
ADC_FULL_SCALE = 4095
ADC_VREF = 3.3

# DAC full scale
DAC_FULL_SCALE = 4095
DAC_MID = 2048


@dataclass(frozen=True)
class Channel:
    """One optical channel in the multi-source photometer."""

    name: str
    wavelength_nm: float
    freq_hz: float
    gate_kind: str  # 'pwm', 'dac_compare', 'dout_toggle'
    gate_index: int  # which PWM / DAC / DOUT it drives (0..15)
    adc_index: int = 0  # which ADC channel sees the response (usually 0)
    enabled: bool = True
    safety_gated: bool = False  # True for laser channels — respects interlock
    notes: str = ""


@dataclass(frozen=True)
class PhyserverConnection:
    """How to connect to the physerver."""

    transport: str = "ipc"  # 'ipc' (shared memory) or 'ws' (WebSocket)
    ipc_path: str = "/dev/shm/phycmd_status"
    ws_url: str = "ws://localhost:8080/ws"
    timeout_s: float = 5.0


@dataclass(frozen=True)
class Config:
    """Top-level application configuration."""

    fs_hz: float
    integration_s: float
    channels: tuple[Channel, ...]
    physerver: PhyserverConnection
    database_path: Path
    reports_path: Path

    @property
    def n_channels(self) -> int:
        return len(self.channels)

    def get_channel(self, name: str) -> Channel:
        for ch in self.channels:
            if ch.name == name:
                return ch
        raise KeyError(f"Unknown channel: {name}")


def load_config(path: Path | str | None = None) -> Config:
    """Load the configuration from a TOML file.

    Resolution order:
    1. The ``path`` argument if provided.
    2. The ``ANALYZER_CONFIG`` environment variable.
    3. ``config/channels.toml`` relative to the application root.
    """
    if path is None:
        path = os.environ.get("ANALYZER_CONFIG")
    if path is None:
        # Look for config/channels.toml two levels up from this file
        here = Path(__file__).resolve().parent
        path = here.parent / "config" / "channels.toml"

    path = Path(path)
    if not path.exists():
        raise FileNotFoundError(f"Config file not found: {path}")

    with path.open("rb") as f:
        data = tomllib.load(f)

    fs_hz = float(data.get("fs_hz", FS_DEFAULT_HZ))
    integration_s = float(data.get("integration_s", 1.0))

    channels_raw = data.get("channels", [])
    channels = tuple(
        Channel(
            name=c["name"],
            wavelength_nm=float(c["wavelength_nm"]),
            freq_hz=float(c["freq_hz"]),
            gate_kind=c["gate_kind"],
            gate_index=int(c["gate_index"]),
            adc_index=int(c.get("adc_index", 0)),
            enabled=bool(c.get("enabled", True)),
            safety_gated=bool(c.get("safety_gated", False)),
            notes=c.get("notes", ""),
        )
        for c in channels_raw
    )

    phy_raw = data.get("physerver", {})
    physerver = PhyserverConnection(
        transport=phy_raw.get("transport", "ipc"),
        ipc_path=phy_raw.get("ipc_path", "/dev/shm/phycmd_status"),
        ws_url=phy_raw.get("ws_url", "ws://localhost:8080/ws"),
        timeout_s=float(phy_raw.get("timeout_s", 5.0)),
    )

    paths_raw = data.get("paths", {})
    home = Path.home()
    database_path = Path(
        paths_raw.get("database", home / ".phycommander/analyzer.db")
    )
    reports_path = Path(
        paths_raw.get("reports", home / ".phycommander/analyzer/reports")
    )

    return Config(
        fs_hz=fs_hz,
        integration_s=integration_s,
        channels=channels,
        physerver=physerver,
        database_path=database_path,
        reports_path=reports_path,
    )
