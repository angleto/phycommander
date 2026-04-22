# Changelog

All notable changes to PhyCMD will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.0] - 2026-04-22

Clean re-baseline of the project: the previous `v1.x` history was
retired from the public remote alongside this release. The v1.x
branches and tags are no longer available on `origin`; anyone who
had already cloned them may keep using that code under the old
licence terms, but no new v1.x fixes will be published. This is the
first release of the copyleft era.

### Changed — licensing (BREAKING)

Full relicensing of the project with explicit copyleft protection
across every layer. The previous `MIT OR Apache-2.0` arrangement is
retired for the software components. Sole author, no prior external
contributions required relicensing consent.

- **Software** (Rust server, `phycmd-core`, `phycmd-rust`, `phycmd-py`,
  dashboard HTML/JS): **AGPL-3.0-or-later** — network-aware copyleft
  to prevent silent SaaS appropriation.
- **Firmware** (SAM3X8E C code), **deployment plumbing**, and
  **CLI scripts**: **GPL-3.0-or-later**.
- **Hardware designs** (KiCad project, panel SVGs, mechanical CAD):
  **CERN-OHL-S v2** (Strongly Reciprocal).
- **Prose documentation + photographs**: unchanged, **CC-BY-SA 4.0**.
- **Project name + logo**: now covered by an explicit trademark
  policy (`TRADEMARKS.md`). Forks that diverge from upstream must
  rename before distribution.
- Every source file gained an `SPDX-License-Identifier` header.
- `LICENSE-MIT` and `LICENSE-APACHE` removed; `LICENSE-AGPL-3.0`,
  `LICENSE-GPL-3.0`, and `LICENSE-CERN-OHL-S-2.0` added.
- Workspace and firmware version bumped to `2.0.0` across all
  manifests (`physerver`, `phycmd-core`, `phycmd-rust`, `phycmd-py`,
  `pyproject.toml`, firmware `USB_DEVICE_MAJOR_VERSION`).

### Changed — DAC reactive path (BREAKING)

The `reactive_dac_write` CPU-to-`DACC_CDR` direct write race was
structurally broken pre-2.0. Fixed by routing LUT / THRESHOLD / PID
output through a per-channel `reactive_value` field that the PDC
refill loop emits on every sample. Downstream firmware/drivers that
depended on the old (non-working) behaviour must retest.

### Added — host-side auto-reconnect

Iso transport now detects firmware watchdog resets: an I/O thread
session loop monitors `iso_in_pkts_ok` and, after 4 s of no progress,
tears down + re-opens the libusb device handle. Firmware now actually
enables the SAM3X WDT (2 s timeout, 500 ms SysTick kick) so a wedged
firmware reboots itself and the host re-establishes the stream with
no operator action.

### Added — API

- `CommandStaging::set_*_with_timeout(value, Duration)` for bounded
  waits on the staging buffer; new `StagingError::Timeout` variant.
- `CHAN_STATE_FLAG_RESERVED` exposed in the protocol and surfaced as
  `ChannelStateView.reserved` so clients can render non-backed
  channels (currently PWM 4..7 on Due) as unavailable.
- `/api/waveform/coexistence` REST endpoint + dashboard badge that
  warns when host-side `WaveformBank` and firmware on-chip generator
  overlap on the same DAC/PWM channel.
- `scripts/rt_benchmark.py` stdlib-only harness for RT-loop / iso
  metrics capture; `docs/technical/XHCI_NOTES.md` with the
  measurement protocol.
- IPC `IpcServer` / `IpcClient` now implement `Send + Sync`
  (documented safety argument); `read_command_with_seq` added.
  Shared memory is now actually populated on every status frame, and
  commands pushed by external clients via IPC reach staging. Fixes a
  silent regression from the iso-mode rework.
- Iso DMA buffers allocated via custom `AlignedBuffer` with 64 B
  cache-line alignment (required on non-coherent ARM hosts).
- libusb version logged at startup.

### Fixed

- `threshold_eval` hysteresis condition restored (`|| true` residue
  removed).
- `Capabilities.num_pwm` now matches `WAVE_NUM_PWM_ACTIVE` (4) instead
  of falsely advertising 8 BUILTIN-capable PWM channels.
- `refill_buffer` MANUAL fallback holds the last applied DAC value
  instead of snapping to mid-rail on a GENERATOR → MANUAL transition.
- Dead `ADC_Handler` + `update_adc_irq_needed` stub removed from
  firmware; ADC IRQ is now unambiguously off and reactive evaluation
  documented as SysTick-driven.
- `tests/selftest.rs` rewritten to use `physerver::protocol`
  (CRC-validated) instead of the stale pre-CRC raw layout.
- Obsolete `NUM_TX_SLOTS` comment updated from 16 to 64.

## [1.1.0] - 2026-04-14

_Removed from the public remote in 2.0.0; notes preserved for
archival purposes._

First public open-source release. Repo renamed from `phycmd` →
`phycommander`, default branch is now `v1.1`. Older branches deleted.

### Added — firmware (SAM3X8E)
- On-chip function generator: BUILTIN / ARBITRARY / LUT / THRESHOLD /
  PULSE_TRIG / PID per channel.
- MODE_PWM_DUTY on PWM0..PWM3 (PIOC21..24 → Due pins 9/8/7/6).
- `loop_time_us` populated via DWT cycle counter.
- `waveform_reactive_dout_mask()` so iso Command frames don't
  overwrite reactive-driven DOUTs.
- DOUT reactive shape echoed in `get_state` so the dashboard
  indicator turns green on Apply.
- Reactive ADC eval moved from ADC_Handler (≈60 kHz ENDRX) to the
  1 kHz SysTick — avoids the USB stack hang we hit when the per-ISR
  evaluation rate starved UOTGHS microframes.

### Added — host (Rust physerver)
- 8 kHz isochronous USB transport with per-packet `seq_num` and
  per-microframe round-trip latency tracking.
- URB-level jitter measurement against the 1 ms expected cadence.
- `IsoTransport::set_telemetry_detail()` toggle to pause the
  per-packet seq + latency tracking in the iso hot path.
- `POST /api/reset_telemetry` zeroes RT-stats counters + IsoStats
  + error baseline.
- `GET / POST /api/telemetry/detail` JSON endpoint.
- Dashboard-served logo (`/logo.svg`, `/logo-mark.svg`) with
  `Cache-Control: public, max-age=3600`.
- `Cache-Control: no-store` on the dashboard HTML to prevent
  browsers serving stale JS after a redeploy.

### Added — dashboard
- On-Chip Function Generator panel: 2 DAC + 4 PWM + 16 DOUT rows,
  per-row mode dropdown, parameter form with `userDirty` flag
  protecting in-progress edits from the 1 s state poll.
- 16-row DOUT strip with green/grey indicator + click-to-expand
  config + one-click stop.
- Oscilloscope: analytic canvas-pixel rendering for DAC sine /
  square / triangle / sawtooth and for on-chip PWM squares
  (bypasses the WebSocket 250 Hz throttle).
- Collapsible panels with persistent `localStorage` state.
- Φ> logo in the header + as the favicon.
- Reset Telemetry button in the RT Scheduler Stats panel.
- Click-to-toggle Seq badge in the status bar.
- Fixed-width measurement table columns with `tabular-nums` so
  values don't shift horizontally as digits change width.

### Added — release infrastructure
- Dual-licensed MIT OR Apache-2.0 (software) + CC-BY-SA 4.0
  (hardware designs, panel CAD, photos, long-form docs).
  Three canonical license files committed alongside the top-level
  LICENSE summary and a NOTICE file.
- `Φ>` logo (full mark + favicon variant) in `physerver/static/`.
- `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1) and
  `SECURITY.md` (private report channel + 90-day disclosure).
- `docs/hardware/FrontPanel_2014.dwg` — original AutoCAD source
  for the steel front panel.
- 4 hardware photos + 4 dashboard screenshots in `docs/images/`.
- BOM section in README listing the reference Amazon IT chassis
  ASINs and the through-hole signal-conditioning parts.
- 2014 → 2026 history paragraph in the README.
- `xHCI vs EHCI` performance note in the BOM (xHCI gives extra
  headroom past the 8 kHz EHCI ceiling).

### Changed
- Cargo.toml `authors` → `Angelo Leto <angelo@leto.blue>`,
  `license` → `MIT OR Apache-2.0`.
- Default branch on the GitHub repo: `v1.0` → `v1.1`.
- Repo renamed: `angleto/phycmd` → `angleto/phycommander`.

### Fixed
- `update_adc_irq_needed()` was never called from `play_lut` /
  `play_threshold` / `stop` — reactive modes were armed but their
  evaluator never ran.
- `set_dig_out_value` was overwriting reactive DOUT writes at
  microframe rate. Now it skips the bits in
  `waveform_reactive_dout_mask()`.
- LUT default `output_mask` for a per-DOUT row was 0xFFFF — picking
  LUT on one DOUT drove all 16 pins. Now defaults to `1 << idx`.
- Fmt sweep across the workspace so the CI fmt gate passes.

## [1.0.0] - 2025-11-22

### Added
- Initial release
- Dual transport system (USB Bulk + USB CDC Serial)
- REST API server with Axum
- WebSocket streaming support
- Shared memory IPC for low-latency communication
- Real-time scheduling support
- Web dashboard interface
- Complete documentation suite (200+ pages)
- Example client implementations
- Systemd service configuration
- TOML-based configuration
- CRC-16 protocol validation
- Performance benchmarks

## [1.0.0] - 2025-11-22

### Added
- Initial release
- Dual transport system (USB Bulk + USB CDC Serial)
- REST API server with Axum
- WebSocket streaming support
- Shared memory IPC for low-latency communication
- Real-time scheduling support
- Web dashboard interface
- Complete documentation suite (200+ pages)
- Example client implementations
- Systemd service configuration
- TOML-based configuration
- CRC-16 protocol validation
- Performance benchmarks

### Features
- **Transport**:
  - USB Bulk transport (10 kHz, 120µs latency)
  - USB CDC Serial transport (1 kHz, 750µs latency)
  - Auto-detection with fallback
  - Modular transport abstraction

- **API**:
  - REST API for command/status
  - WebSocket for real-time streaming
  - Shared memory IPC
  - GPIO, ADC, DAC control

- **Real-Time**:
  - RT scheduler support (SCHED_FIFO)
  - Memory locking (mlockall)
  - CPU affinity
  - DMA latency control

- **Documentation**:
  - User manual
  - API reference
  - Deployment guide
  - Configuration guide
  - Architecture documentation
  - Protocol specification
  - Performance analysis
  - Real-time OS configuration guide

### Hardware Support
- ATSAM3X8E (Arduino Due)
- 16 digital I/O
- 8-channel 12-bit ADC
- 2-channel 12-bit DAC
- DMA-based ADC sampling

### Known Limitations
- Web interface lacks built-in authentication (use reverse proxy)
- Firmware protocol updates pending (documented in FIRMWARE_UPDATES.md)
- IPC only available on Linux/macOS

## [0.9.0] - Development versions

Development and testing versions. Not released.

---

## Versioning

Given a version number MAJOR.MINOR.PATCH:

1. MAJOR version for incompatible API changes
2. MINOR version for backwards-compatible functionality additions
3. PATCH version for backwards-compatible bug fixes

---

[Unreleased]: https://github.com/angleto/phycommander/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/angleto/phycommander/releases/tag/v2.0.0
[1.1.0]: https://github.com/angleto/phycommander/releases/tag/v1.1.0
[1.0.0]: https://github.com/angleto/phycommander/releases/tag/v1.0.0
