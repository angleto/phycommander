# `docs/applications/` — Applications documentation

This folder hosts the **documentation for every application** (instrument,
experiment, use case) built on top of phycommander. Each application lives
in its own subfolder, with a consistent internal structure, so the
documentation scales cleanly as more instruments are added.

## Currently documented applications

| Application | Folder | Status | Description |
|---|---|---|---|
| **Multi-function analyzer** | [`analyzer/`](analyzer/) | in build | Modular analytical bench instrument: 16 measurement modes (optical absorbance, nephelometry, fluorescence, Doppler, pH, conductivity, amperometry, temperature control, titration, ...), 11 application domains (beer, water, bioprocess, enzyme kinetics, nanoparticles, ...). €145 – €1080 build cost. |
| **Piezo Bode + energy harvester** | [`piezo_bode_harvester/`](piezo_bode_harvester/) | concept | Lock-in characterization of a piezo cantilever with a tip mass; injection of recorded vibration spectra to estimate harvested power. Mechanical analogue of the optical lock-in demo. Industrial parallel: shock-absorber test rig. |
| **Vision sorter (Coral + 2-axis arm)** | [`vision_sorter/`](vision_sorter/) | concept | Real-time waste/object classification on a Google Coral USB Edge TPU, 2-axis encoder arm for pick-and-place. Industrial parallel: vision-based optical sorting lines. |
| **Laser tracker (vision-driven pan-tilt)** | [`laser_tracker/`](laser_tracker/) | concept | Webcam-driven pan-tilt with host PID and DOUT-gated laser enable. Industrial parallel: laser marking, optical alignment, weld-head positioning. |

The `Status` column distinguishes between concept-stage design documents
(no code yet, no hardware build), `in build` (host code or hardware exists,
work in progress), and `validated` (full demo run, video shot, validation
experiments passed).

### Top-level design documents

In addition to the per-application subfolders, the three master design
documents for the multi-function analyzer live here at the applications
level because they are conceptually cross-cutting:

| File | Role |
|---|---|
| [`MULTIFUNCTION_ANALYZER.md`](MULTIFUNCTION_ANALYZER.md) | Master design document — 16 modes, 11 domains, modular architecture, 7-paper publication plan. ~1800 lines. |
| [`BEER_ANALYZER.md`](BEER_ANALYZER.md) | Specific configuration for beer QC — dedicated build guide. ~1100 lines. |
| [`LOCKIN_OPTICAL_DEMO.md`](LOCKIN_OPTICAL_DEMO.md) | Didactic starting point — minimum lock-in spectrometer demo. |

The implementation-level details for each of these live inside the
corresponding application folder (currently [`analyzer/`](analyzer/)).

## Structure conventions

Every application subfolder follows this layout:

```
<application-name>/
├── README.md                   entry point and navigation
├── build/                      hardware and firmware build references
│   ├── BOM.md
│   ├── HARDWARE_SCHEMATICS.md
│   └── FIRMWARE_SPEC_*.md
├── operate/                    user-facing operation guides
│   ├── FIRST_EXPERIMENT.md
│   └── PROTOCOL_*.md           per-assay protocols
├── specs/                      future specifications
├── papers/                     paper drafts
└── notes/                      lab notes, progress logs
```

When you add a new application:

1. Create a subfolder under `docs/applications/<new-app-name>/`.
2. Use the standard structure above.
3. Add an entry to the table at the top of this README.
4. Create the corresponding code folder at [`../../applications/<new-app-name>/`](../../applications/).
5. Add design documents either inside the new subfolder (for
   application-specific docs) or at this top level (for documents that
   span multiple applications).

## Related

- **Code side**: [`../../applications/`](../../applications/) — host-side software for all applications, matching this documentation tree one-for-one.
- **Technical reference** (cross-cutting, not application-specific): [`../technical/`](../technical/), [`../firmware/`](../firmware/).
- **User guide**: [`../user-guide/`](../user-guide/).
