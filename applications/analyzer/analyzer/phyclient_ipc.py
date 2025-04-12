"""Interface to physerver (Rust transport layer).

Two transport options:

- **Shared memory IPC** (preferred for the real-time loop): physerver
  publishes the latest status packet to a shared memory region; the
  Python client reads it and writes command packets back. Zero-copy,
  sub-100 µs latency.
- **WebSocket** (fallback for development, remote operation, or when
  shared memory is not available): standard JSON-over-WS at ~100 Hz
  polling rate. Not sufficient for lock-in at 10 kHz but useful for
  development and debugging.

The 64-byte command and status packet layouts are defined in
``docs/technical/PROTOCOL.md`` of the phycommander repository. The v1.1
extensions (PWM fields, interlock flags) are in
``docs/applications/analyzer/build/FIRMWARE_SPEC_V1.1.md``.

This module exposes a simple iterator-style API: ``stream()`` yields
``(command_out, status_in)`` pairs at the phycommander loop rate. The
caller (lock-in engine) writes to ``command_out`` and reads from
``status_in`` each iteration.
"""

from __future__ import annotations

import struct
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterator

import numpy as np

from .config import Config, PhyserverConnection


@dataclass
class Command:
    """v1.1 command packet payload (host → firmware)."""

    digital_out: int = 0  # 16-bit GPIO output mask
    dac: list[int] = field(default_factory=lambda: [2048, 2048])  # 12-bit
    pwm_freq_hz: list[int] = field(default_factory=lambda: [0, 0])
    pwm_duty_u16: list[int] = field(default_factory=lambda: [0, 0])
    pwm_enable: list[bool] = field(default_factory=lambda: [False, False])
    cmd_flags_ext: int = 0
    seq_num: int = 0

    def encode(self) -> bytes:
        """Pack into 64 bytes matching the v1.1 command layout."""
        buf = bytearray(64)
        # Bytes 0-1: seq_num
        struct.pack_into("<H", buf, 0, self.seq_num & 0xFFFF)
        # Bytes 2-3: digital_out
        struct.pack_into("<H", buf, 2, self.digital_out & 0xFFFF)
        # Bytes 4-7: dac[0], dac[1]
        struct.pack_into("<HH", buf, 4, self.dac[0] & 0xFFFF, self.dac[1] & 0xFFFF)
        # Bytes 8-27: reserved / existing command fields (flags, etc.)
        # ...
        # Bytes 28-43: new v1.1 PWM + ext flags
        struct.pack_into(
            "<HHBBHHBBHH",
            buf,
            28,
            self.pwm_freq_hz[0],
            self.pwm_duty_u16[0],
            1 if self.pwm_enable[0] else 0,
            0,  # padding
            self.pwm_freq_hz[1],
            self.pwm_duty_u16[1],
            1 if self.pwm_enable[1] else 0,
            0,  # padding
            self.cmd_flags_ext,
            0,  # padding
        )
        # Bytes 44-61: reserved
        # Bytes 62-63: CRC-16-CCITT, computed below
        crc = crc16_ccitt(bytes(buf[:62]))
        struct.pack_into("<H", buf, 62, crc)
        return bytes(buf)


@dataclass
class Status:
    """v1.1 status packet payload (firmware → host)."""

    digital_in: int = 0
    adc: np.ndarray = field(default_factory=lambda: np.zeros(8, dtype=np.int16))
    status_flags_ext: int = 0
    loop_time_us: int = 0
    firmware_version: int = 0
    seq_num: int = 0
    # Timestamp of when this status was read by the host (monotonic, seconds)
    t_received: float = 0.0

    @property
    def interlock_open(self) -> bool:
        return bool(self.status_flags_ext & 0x0001)

    @classmethod
    def decode(cls, buf: bytes) -> "Status":
        if len(buf) != 64:
            raise ValueError(f"Expected 64-byte status, got {len(buf)}")
        received_crc = struct.unpack_from("<H", buf, 62)[0]
        expected_crc = crc16_ccitt(bytes(buf[:62]))
        if received_crc != expected_crc:
            raise ValueError(
                f"CRC mismatch: expected {expected_crc:04x}, got {received_crc:04x}"
            )
        seq_num = struct.unpack_from("<H", buf, 0)[0]
        digital_in = struct.unpack_from("<H", buf, 2)[0]
        adc = np.frombuffer(buf, dtype=np.int16, count=8, offset=4).copy()
        status_flags_ext = struct.unpack_from("<H", buf, 44)[0]
        loop_time_us = struct.unpack_from("<H", buf, 46)[0]
        firmware_version = struct.unpack_from("<H", buf, 62 - 4)[0]
        return cls(
            digital_in=digital_in,
            adc=adc,
            status_flags_ext=status_flags_ext,
            loop_time_us=loop_time_us,
            firmware_version=firmware_version,
            seq_num=seq_num,
            t_received=time.monotonic(),
        )


def crc16_ccitt(data: bytes, poly: int = 0x1021, init: int = 0xFFFF) -> int:
    """CRC-16-CCITT as used by phycommander PhyCMD-64 protocol."""
    crc = init
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            if crc & 0x8000:
                crc = ((crc << 1) ^ poly) & 0xFFFF
            else:
                crc = (crc << 1) & 0xFFFF
    return crc


class IpcClient:
    """Shared-memory IPC client for physerver.

    Provides a streaming iterator that yields ``(Command, Status)``
    pairs at the phycommander loop rate.
    """

    def __init__(self, config: Config) -> None:
        self.config = config
        self.conn = config.physerver
        self._open = False

    def __enter__(self) -> "IpcClient":
        self.open()
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    def open(self) -> None:
        # In a full implementation this opens the /dev/shm region via mmap.
        # For now, stub: mark as open and defer to a simulated path if the
        # shared-memory path is not present on the host.
        path = Path(self.conn.ipc_path)
        if path.exists():
            self._open = True
        else:
            # Fallback to simulation mode for development on a host without
            # physerver running.
            self._open = True

    def close(self) -> None:
        self._open = False

    def is_connected(self) -> bool:
        return self._open

    def stream(self) -> Iterator[tuple[Command, Status]]:
        """Yield (command, status) pairs at ``fs_hz`` rate.

        Real implementation: read the shared-memory buffer, yield the
        status to the caller, let the caller populate the command, then
        write the command back to shared memory.

        This stub implementation generates synthetic status packets for
        development / testing without real hardware. A brighter-eyed
        implementation will replace the simulation block below with an
        mmap-backed ring buffer once physerver exposes one.
        """
        if not self._open:
            raise RuntimeError("IpcClient is not open. Call .open() first.")

        cmd = Command()
        dt = 1.0 / self.config.fs_hz
        k = 0
        t0 = time.monotonic()

        while True:
            status = Status(seq_num=k & 0xFFFF, t_received=time.monotonic())
            # Simulate an ADC response for development:
            status.adc = np.full(8, 2048, dtype=np.int16)

            yield cmd, status

            # Let the caller populate `cmd` for the next iteration.
            k += 1
            next_t = t0 + k * dt
            sleep_time = next_t - time.monotonic()
            if sleep_time > 0:
                time.sleep(sleep_time)
