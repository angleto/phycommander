# Changelog

All notable changes to PhyCMD will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0] - 2026-04-14

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

[Unreleased]: https://github.com/your-org/phycmd/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/your-org/phycmd/releases/tag/v1.0.0
