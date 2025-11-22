# Changelog

All notable changes to PhyCMD will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- CI/CD pipeline with GitHub Actions
- LICENSE file (MIT)
- CHANGELOG.md
- Production readiness report
- Security audit workflow

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
