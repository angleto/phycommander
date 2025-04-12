# `analyzer/` — Multi-function analytical bench instrument

This folder contains **everything instrument-specific** for the multi-
function analytical bench built on phycommander: hardware build docs,
firmware specs, operation procedures, lab notes, paper drafts, and
future specifications.

It is organized into subfolders by *purpose*, so the hierarchy can
grow as more specs, more papers, and more procedures are added without
becoming a flat mess.

## Structure

```
analyzer/
├── README.md                   ← you are here
│
├── build/                      ← implementation references (hardware + firmware)
│   ├── BOM.md                   bill of materials (tiered)
│   ├── HARDWARE_SCHEMATICS.md   analog circuits (TIA, drivers, interlock, modules)
│   └── FIRMWARE_SPEC_V1.1.md   required firmware additions on ATSAM3X8E
│
├── operate/                    ← user-facing operation and procedures
│   └── FIRST_EXPERIMENT.md      methylene blue Beer-Lambert validation (go/no-go)
│
├── specs/                      ← future specifications
│                                 (additional sensors, new modules, protocol extensions)
│
├── papers/                     ← draft manuscripts
│                                 (HardwareX, JOH, AJP, EJP drafts as they're written)
│
└── notes/                      ← lab notes, progress logs, photographs
                                  (build diary, calibration records, anomalies observed)
```

## Relationship to parent documents

Three master design documents sit one level up at [`../`](../):

| Document | Role |
|---|---|
| [`../MULTIFUNCTION_ANALYZER.md`](../MULTIFUNCTION_ANALYZER.md) | The master vision — 16 measurement modes, 11 application domains, modular architecture. Read this first if you want to understand *what* the instrument is and *why* it is built this way. |
| [`../BEER_ANALYZER.md`](../BEER_ANALYZER.md) | A specific configuration of the instrument for beer QC. Read this when you want to understand one concrete application in depth. |
| [`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md) | A didactic starting point: the minimum lock-in spectrometer. Read this if you're new to lock-in detection or want the smallest possible first build. |

The files *inside* `analyzer/` are the concrete reference that makes
those design documents buildable: actual part numbers, actual schematics,
actual firmware-level fields, actual step-by-step procedures.

## How to use these files

### First-time builder

Follow this sequence:

1. **Understand**: [`../MULTIFUNCTION_ANALYZER.md`](../MULTIFUNCTION_ANALYZER.md) §1–§5 for the vision and the modular architecture.
2. **Decide a tier**: [`build/BOM.md`](build/BOM.md) §9 lists tiered configurations from €145 (starter) to €1080 (everything). Pick one matching your goal.
3. **Order components**: [`build/BOM.md`](build/BOM.md) §10 has a shopping checklist with part numbers and suppliers.
4. **Update firmware**: [`build/FIRMWARE_SPEC_V1.1.md`](build/FIRMWARE_SPEC_V1.1.md) is the implementation reference for the ATSAM3X8E changes needed.
5. **Build the hardware**: [`build/HARDWARE_SCHEMATICS.md`](build/HARDWARE_SCHEMATICS.md) is the wiring reference.
6. **Install the host software**: [`../../../../applications/analyzer/README.md`](../../../../applications/analyzer/README.md).
7. **Validate the build**: [`operate/FIRST_EXPERIMENT.md`](operate/FIRST_EXPERIMENT.md) — one hour, €2 of reagents, clear pass/fail verdict. **Do not skip.**
8. **Run your first real assay**: [`../BEER_ANALYZER.md`](../BEER_ANALYZER.md) §10.1 for ethanol, or [`../MULTIFUNCTION_ANALYZER.md`](../MULTIFUNCTION_ANALYZER.md) §14 for other application domains.

### Contributor

Found a better part number, a cleaner circuit, a successful build
photograph, or a new measurement protocol? Add it in the right folder:

- **New hardware option / better component** → edit [`build/BOM.md`](build/BOM.md)
- **Circuit revision** → edit [`build/HARDWARE_SCHEMATICS.md`](build/HARDWARE_SCHEMATICS.md)
- **Firmware extension** → new file in [`specs/`](specs/) named `FIRMWARE_SPEC_V1.2.md` etc., then cross-reference from the README
- **New assay protocol** → new file in [`operate/`](operate/) named `PROTOCOL_<assay>.md`
- **Paper draft** → new file in [`papers/`](papers/) named `paper_<topic>.md` or similar
- **Build notes / diary / anomalies** → new file in [`notes/`](notes/)

Open an issue or pull request on the phycommander repository with
label `analyzer`.

## Related

- **Code**: [`../../../../applications/analyzer/`](../../../../applications/analyzer/) — Python application that runs on the host and drives the instrument.
- **Phycommander core**: [`../../../../physerver/`](../../../../physerver/) — Rust server handling the USB transport to the hardware.
- **Phycommander firmware**: [`../../../../ATSAM3X8E_FW/`](../../../../ATSAM3X8E_FW/) — Arduino Due firmware (target of v1.1 spec).
- **Protocol reference**: [`../../../technical/PROTOCOL.md`](../../../technical/PROTOCOL.md).
