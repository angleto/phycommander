# `analyzer` — host application for the multi-function analytical bench

Python 3.11+ application that drives the phycommander multi-function
analytical bench instrument. Implements the multi-channel lock-in
engine, the measurement mode controllers, the per-assay protocols, and
the calibration / reporting database.

Design documents and hardware references live one level up:

- [`../../docs/applications/MULTIFUNCTION_ANALYZER.md`](../../docs/applications/MULTIFUNCTION_ANALYZER.md) — master design document (16 modes, 11 application domains)
- [`../../docs/applications/BEER_ANALYZER.md`](../../docs/applications/BEER_ANALYZER.md) — specific beer QC configuration
- [`../../docs/applications/analyzer/`](../../docs/applications/analyzer/) — implementation reference (BOM, schematics, firmware spec, first experiment)

## Requirements

- **Hardware**: phycommander (Arduino Due + analyzer base station + at least one optical head). See [`../../docs/applications/analyzer/build/BOM.md`](../../docs/applications/analyzer/build/BOM.md).
- **Firmware**: phycommander ≥ v1.1 flashed on the ATSAM3X8E. See [`../../docs/applications/analyzer/build/FIRMWARE_SPEC_V1.1.md`](../../docs/applications/analyzer/build/FIRMWARE_SPEC_V1.1.md).
- **Host OS**: Linux with PREEMPT_RT (recommended), or any Linux / macOS for non-real-time use. Windows is untested.
- **Python**: 3.11 or newer.
- **physerver**: running and configured. See [`../../physerver/README.md`](../../physerver/README.md).

## Install

```bash
cd applications/analyzer
python -m venv .venv
source .venv/bin/activate
pip install -e .
```

For development:

```bash
pip install -e .[dev]
```

## Usage

### Main application

```bash
# Start the interactive TUI
python -m analyzer.main

# Run a specific mode
python -m analyzer.main --mode absorbance --channel 650

# Run a specific assay
python -m analyzer.main --assay ethanol

# List available modes/assays
python -m analyzer.main --list
```

### Standalone scripts

```bash
# Electronic self-test (DAC loopback, interlock check, noise floor)
python -m scripts.self_test

# Methylene blue Beer-Lambert validation (the go/no-go first experiment)
python -m scripts.methylene_blue_run
```

### Configuration

Channels (wavelengths, modulation frequencies, driver kinds) and
calibration data live in [`config/channels.toml`](config/channels.toml).
Edit this file if your hardware build uses different wavelengths or
frequencies.

Per-assay calibration curves are stored in the SQLite database at
`~/.phycommander/analyzer.db` and are expected to be created by running
the calibration wizard (`python -m analyzer.main --calibrate <assay>`)
with known standards. Default/seed calibrations are in
[`config/default_calibration.toml`](config/default_calibration.toml).

## Architecture

```
  ┌─────────────────────────┐
  │ UI (TUI / live plot)    │
  ├─────────────────────────┤
  │ assay_runner            │  per-assay state machines
  ├─────────────────────────┤
  │ mode controllers        │  absorbance, kinetic, nephelometry, ...
  ├─────────────────────────┤
  │ lockin engine (shared)  │  N parallel lock-ins
  │ database / calibration  │
  │ reporting               │
  ├─────────────────────────┤
  │ phyclient_ipc           │  shared memory or WebSocket to physerver
  ├─────────────────────────┤
  │ physerver (Rust)        │
  ├─────────────────────────┤
  │ phycommander firmware   │
  └─────────────────────────┘
```

Each layer is a thin abstraction over the one below it. The lock-in
engine in [`analyzer/lockin.py`](analyzer/lockin.py) is the core — the
rest of the application is configuration, orchestration, and
persistence around it.

## Directory layout

```
applications/analyzer/
├── README.md                  (this file)
├── pyproject.toml
│
├── analyzer/                  the Python package
│   ├── __init__.py
│   ├── main.py                CLI entry point
│   ├── config.py              global configuration loader
│   ├── phyclient_ipc.py       interface to physerver (shared memory or WS)
│   ├── lockin.py              multi-channel lock-in engine
│   ├── database.py            SQLite persistence
│   ├── calibration.py         calibration manager
│   ├── reporting.py           CSV / PDF export
│   │
│   ├── modes/                 measurement mode controllers
│   │   ├── base.py            AbstractMode base class
│   │   ├── absorbance.py      8-channel absorbance mode
│   │   ├── kinetic.py         enzyme kinetic mode
│   │   └── nephelometry.py    90° scattering mode
│   │
│   ├── assays/                chemistry protocols
│   │   ├── ethanol.py         ADH/NADH enzymatic at 340 nm
│   │   ├── bradford.py        protein at 590 nm
│   │   ├── color_ebc.py       beer color at 430 nm
│   │   ├── haze_ebc.py        beer haze at 650 nm scatter
│   │   └── folin.py           polyphenols at 740 nm
│   │
│   ├── hardware/              hardware abstraction
│   │   └── modules.py         module enumeration via I²C EEPROM
│   │
│   └── ui/
│       └── live_plot.py       matplotlib live plotting
│
├── tests/                     pytest tests
│   ├── test_lockin.py
│   └── test_freq_set.py
│
├── scripts/                   standalone executables
│   ├── self_test.py
│   └── methylene_blue_run.py
│
└── config/                    TOML configuration
    └── channels.toml
```

## Tests

```bash
pytest
```

Current test coverage:

- `test_lockin.py` — verifies the lock-in engine's amplitude recovery,
  cross-channel rejection, and phase calibration convergence on
  synthetic signals.
- `test_freq_set.py` — verifies that the default modulation-frequency
  set (§3.3 of the design document) satisfies the pairwise-incommensurate
  constraint and keeps cross-channel leakage below −50 dB.

## License

CERN-OHL-S v2 (matches the phycommander project license for open
hardware).
