"""Module enumeration.

When the analyzer base station is powered on, it reads an I²C EEPROM
on each expansion header to identify what module is connected. This
module handles the reading and parsing of those EEPROMs.

Requires phycommander firmware v1.2 (which exposes the I²C peripheral).
Until v1.2 is available, module identification is done manually via
the ``ANALYZER_MODULES`` environment variable or the ``modules`` key
in ``config/channels.toml``.
"""

from __future__ import annotations

import enum
import logging
import os
import struct
from dataclasses import dataclass


log = logging.getLogger(__name__)


class ModuleType(enum.IntEnum):
    """Recognized module types.

    The numeric values are what the EEPROM stores in byte 0 of its
    identification record. Keep this enum in sync with the firmware
    spec in ``docs/applications/analyzer/build/HARDWARE_SCHEMATICS.md``.
    """

    NONE = 0x00
    ABSORBANCE_HEAD = 0x10
    FLUORESCENCE_HEAD = 0x11
    MULTI_ANGLE_HEAD = 0x12
    DOPPLER_HEAD = 0x13
    FLOW_CELL_HEAD = 0x14
    ELECTROCHEMICAL = 0x20
    ACTUATOR = 0x30
    UNKNOWN = 0xFF

    @classmethod
    def parse(cls, byte: int) -> "ModuleType":
        try:
            return cls(byte)
        except ValueError:
            return cls.UNKNOWN


@dataclass(frozen=True)
class ModuleInfo:
    header: str  # "A" or "B"
    module_type: ModuleType
    hardware_version: tuple[int, int]  # (major, minor)
    serial_number: int
    manufacturer: str
    calibration_date: str  # ISO date or empty
    raw: bytes

    @property
    def present(self) -> bool:
        return self.module_type != ModuleType.NONE


def parse_eeprom(raw: bytes, header: str) -> ModuleInfo:
    """Parse a 32-byte EEPROM record into a ``ModuleInfo``.

    Layout (32 bytes):

    ===   ==========  ========================================
    Off   Size        Field
    ===   ==========  ========================================
    0     1           module_type
    1     1           hw_version_major
    2     1           hw_version_minor
    3     4           serial_number (little endian)
    7     10          manufacturer (ASCII, null-padded)
    17    10          calibration_date (ISO YYYY-MM-DD + null)
    27    5           reserved / future
    ===   ==========  ========================================
    """
    if len(raw) < 32:
        raise ValueError(f"EEPROM record too short: {len(raw)} bytes")

    module_type = ModuleType.parse(raw[0])
    hw_major = raw[1]
    hw_minor = raw[2]
    serial_number = struct.unpack_from("<I", raw, 3)[0]
    manufacturer = raw[7:17].decode("ascii", errors="replace").rstrip("\x00")
    calibration_date = raw[17:27].decode("ascii", errors="replace").rstrip("\x00")

    return ModuleInfo(
        header=header,
        module_type=module_type,
        hardware_version=(hw_major, hw_minor),
        serial_number=serial_number,
        manufacturer=manufacturer,
        calibration_date=calibration_date,
        raw=raw[:32],
    )


def enumerate_modules(client=None) -> list[ModuleInfo]:
    """Return the list of modules currently attached to the base station.

    Until firmware v1.2 exposes I²C, this function falls back to
    environment-variable based identification. Set
    ``ANALYZER_MODULES=absorbance,electrochemical`` in your shell to
    declare what's attached.
    """
    env = os.environ.get("ANALYZER_MODULES")
    if env:
        return _parse_env(env)

    if client is None:
        log.warning(
            "I²C enumeration not available (needs firmware v1.2). "
            "Set ANALYZER_MODULES env var or use the config file."
        )
        return []

    # Placeholder for the real implementation:
    # 1. Send an I²C read command at address 0x50 (header A EEPROM)
    # 2. Send an I²C read command at address 0x51 (header B EEPROM)
    # 3. Parse each as a 32-byte record
    return []


def _parse_env(env: str) -> list[ModuleInfo]:
    names = [n.strip().lower() for n in env.split(",") if n.strip()]
    name_to_type = {
        "absorbance": ModuleType.ABSORBANCE_HEAD,
        "fluorescence": ModuleType.FLUORESCENCE_HEAD,
        "multiangle": ModuleType.MULTI_ANGLE_HEAD,
        "multi-angle": ModuleType.MULTI_ANGLE_HEAD,
        "doppler": ModuleType.DOPPLER_HEAD,
        "flow": ModuleType.FLOW_CELL_HEAD,
        "electrochemical": ModuleType.ELECTROCHEMICAL,
        "electrochem": ModuleType.ELECTROCHEMICAL,
        "actuator": ModuleType.ACTUATOR,
    }
    modules: list[ModuleInfo] = []
    for i, name in enumerate(names):
        mt = name_to_type.get(name, ModuleType.UNKNOWN)
        header = "A" if i == 0 else "B"
        modules.append(
            ModuleInfo(
                header=header,
                module_type=mt,
                hardware_version=(0, 0),
                serial_number=0,
                manufacturer="env",
                calibration_date="",
                raw=b"",
            )
        )
    return modules
