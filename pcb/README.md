# PCB — phycommander I/O Backplane v3 (Arduino Due Shield)

KiCad project skeleton. Open `pcb_backplane_v3.kicad_pro` with KiCad 8.x to start.

## What's in this skeleton

- **`pcb_backplane_v3.kicad_pro`** — KiCad project file (minimal, defaults populated by KiCad on first open)
- **`pcb_backplane_v3.kicad_pcb`** — PCB layout containing:
  - Board outline 101.60 × 53.34 mm (Arduino Due footprint exact)
  - 4 × M3 mounting holes (NPTH, Ø 3.2 mm) at Due-compatible positions
  - Silkscreen text guides indicating where to place the 9 D-Sub headers
  - Title block + board ID silkscreen
  - **No** routed traces — that's the user's job (see workflow below)
  - **No** connector footprints yet — drag in from KiCad library

## What's NOT here yet

The skeleton is INTENTIONALLY minimal so you can drive the rest in KiCad's GUI:
- Schematic file (`.kicad_sch`) — create from scratch in Eeschema (no auto-link to PCB needed if you work directly in PCB editor)
- Connector footprints (D-Sub headers + Arduino mating headers) — drag from KiCad standard library
- Traces — route in PCB editor with push-and-shove router or autoroute
- Ground pour — add via `Place → Filled Zone`
- Netlist — define via schematic, or add nets directly in PCB editor

## Workflow (estimated 3–4 hours total for first iteration)

### Step 1 — Open project (1 min)
```
$ cd pcb/
$ open pcb_backplane_v3.kicad_pro
```
Or just double-click the `.kicad_pro` file in Finder. KiCad will show the project manager. Open the PCB by clicking the PCB icon.

### Step 2 — Verify board outline + holes (2 min)
You should see:
- Yellow rectangle = board outline (Edge.Cuts layer)
- 4 small circles labelled MH1–MH4 = mounting holes
- Silkscreen text indicating where each connector group goes

If the board looks correct (101.6 × 53.34 mm rectangle with 4 holes), proceed.

### Step 3 — Add D-Sub male pin headers on TOP side (15 min)

For each of 9 connectors, use the **Add Footprint** tool (`A` shortcut):

| Reference | Footprint (KiCad library) | Position (mm) | Layer |
|---|---|---|---|
| J1 (DB25 #1) | `Connector_PinHeader_2.54mm:PinHeader_2x13_P2.54mm_Vertical` | (19, 10) | F.Cu |
| J2 (DB25 #2) | `Connector_PinHeader_2.54mm:PinHeader_2x13_P2.54mm_Vertical` | (19, 23) | F.Cu |
| J3 (DB25 #3) | `Connector_PinHeader_2.54mm:PinHeader_2x13_P2.54mm_Vertical` | (19, 39) | F.Cu |
| J4 (DB15 #1) | `Connector_PinHeader_2.54mm:PinHeader_2x08_P2.54mm_Vertical` | (56, 10) | F.Cu |
| J5 (DB15 #2) | `Connector_PinHeader_2.54mm:PinHeader_2x08_P2.54mm_Vertical` | (56, 23) | F.Cu |
| J6 (DB15 #3) | `Connector_PinHeader_2.54mm:PinHeader_2x08_P2.54mm_Vertical` | (56, 39) | F.Cu |
| J7 (DB9 #1) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (82, 10) | F.Cu |
| J8 (DB9 #2) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (82, 23) | F.Cu |
| J9 (DB9 #3) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (82, 39) | F.Cu |

After placing each, delete the corresponding silkscreen text guide.

### Step 4 — Add Arduino mating headers on BOTTOM side (10 min)

These plug DOWN into the Arduino Due's female headers. Use **Flip** (`F` shortcut) after placement to put them on B.Cu (bottom side).

| Reference | Footprint | Position (mm) | Layer | Pin length |
|---|---|---|---|---|
| H_PWR (POWER) | `Connector_PinHeader_2.54mm:PinHeader_1x08_P2.54mm_Vertical` | ~(7, 50) | B.Cu | 7 mm standard |
| H_ANA (ANALOG) | `Connector_PinHeader_2.54mm:PinHeader_1x08_P2.54mm_Vertical` | ~(36, 50) | B.Cu | 7 mm |
| H_COM (COMM) | `Connector_PinHeader_2.54mm:PinHeader_1x10_P2.54mm_Vertical` | ~(13, 3.5) | B.Cu | 7 mm |
| H_DIG (DIGITAL D0–D7) | `Connector_PinHeader_2.54mm:PinHeader_1x08_P2.54mm_Vertical` | ~(40, 3.5) | B.Cu | 7 mm |
| H_D22 (2×18 D22–D53) | `Connector_PinHeader_2.54mm:PinHeader_2x18_P2.54mm_Vertical` | ~(57, 30) | B.Cu (rotated 90°) | 7 mm |
| H_DAC (DAC/CAN 1×6) | `Connector_PinHeader_2.54mm:PinHeader_1x06_P2.54mm_Vertical` | ~(83, 30) | B.Cu (rotated 90°) | 7 mm |

> ⚠ **Verify Arduino header positions against the actual Due board** before final routing — these are approximate. Easiest verification: print the PCB at 1:1 from KiCad (`File → Plot`), overlay on a real Due, mark any offsets.
>
> Even better: import an actual Arduino Mega R3 footprint from a community KiCad library (SnapEDA, Ultra Librarian) for guaranteed-correct positions.

### Step 5 — Define nets (30–60 min)

Two options:

**(A) Schematic-driven**: Open Eeschema, create a `.kicad_sch` file, place all 15 connector symbols, draw net labels (DIN0..15, DOUT0..15, ADC0..7, DAC0/1, +3V3, +5V, GND, etc.) per the pinout in [`docs/technical/PCB_BACKPLANE_PINOUT.md`](../docs/technical/PCB_BACKPLANE_PINOUT.md). Then `Tools → Update PCB from Schematic` to push nets to the PCB.

**(B) PCB-direct**: In PCBnew, click each pad and assign nets manually via the properties dialog. Faster for this skeleton's ~50 net mapping.

Net list reference (from `PCB_BACKPLANE_PINOUT.md` §4):
- 16 DIN nets (DIN0..15) — connect to even pins of H_D22 and to specified D-Sub pins
- 16 DOUT nets (DOUT0..15) — connect to odd pins of H_D22 and to specified D-Sub pins
- 8 ADC nets (ADC0..7) — connect to H_ANA pins 1–8 and to specified D-Sub pins
- 2 DAC nets (DAC0, DAC1) — connect to H_DAC pins 1–2 and to specified D-Sub pins
- Power nets: +3V3, +5V, GND, AREF
- Comm nets (future): UART_TX/RX, SPI_MOSI/MISO/SCK, I2C_SDA/SCL

### Step 6 — Route traces (60–120 min)

Use the built-in router (`X` shortcut) with push-and-shove for tight spaces. Recommended layer assignment (per `PCB_BACKPLANE_PINOUT.md` §7):
- **Bottom layer (B.Cu)**: long horizontal traces from H_D22 westward to D-Sub columns
- **Top layer (F.Cu)**: short local connections at D-Sub headers + ground pour

Or click `Route → Auto-route Selected Tracks` for autoroute.

### Step 7 — Add ground pour (5 min)

`Place → Filled Zone` on B.Cu, draw a polygon covering the whole board outline. Set net to GND. KiCad fills the bottom layer with copper connected to GND.

### Step 8 — DRC check (5 min)

`Inspect → Design Rules Checker`. Fix any errors before plotting.

### Step 9 — Export print masters for UV exposure (5 min)

`File → Plot`:
- **Plot format**: PDF
- **Layers to plot**:
  - F.Cu (top copper) — for top master
  - B.Cu (bottom copper) — for bottom master, **enable "Mirror" option**
- **Plot border and title block**: NO (saves ink)
- **Plot footprint references and values**: NO
- **Plot pad on silkscreen**: NO
- **Drill marks**: small marks (helps with drilling later)
- **Output directory**: `gerbers/` (auto-created)

Result: 2 PDF files at 1:1 scale, ready to print on transparency film for UV bromograph.

### Step 10 — Drill template (3 min)

`File → Fabrication Outputs → Drill Files`. Generates a drill table (PDF) showing all hole sizes and positions. Use as guide when manually drilling the etched PCB.

## Footprint library availability

All footprints used are in the **default KiCad standard library** (installed with KiCad 8). No external libraries required:
- `Connector_PinHeader_2.54mm` — 2.54mm pin headers
- `MountingHole` — mounting hole footprints

If KiCad reports "footprint not found", check:
```
Preferences → Manage Footprint Libraries → Global Libraries
```
and ensure `Connector_PinHeader_2.54mm` is enabled (default install: `${KICAD8_FOOTPRINT_DIR}/Connector_PinHeader_2.54mm.pretty`).

## Reference docs in this repo

- [`docs/technical/PCB_BACKPLANE_PINOUT.md`](../docs/technical/PCB_BACKPLANE_PINOUT.md) — full pinout, BOM, signal routing strategy
- [`docs/technical/DEVICE_PINOUT.md`](../docs/technical/DEVICE_PINOUT.md) — front + rear panel context
- [`panel_sketch/pcb_backplane_top.svg`](../panel_sketch/pcb_backplane_top.svg) — visual placement reference (top side)
- [`panel_sketch/pcb_backplane_bottom.svg`](../panel_sketch/pcb_backplane_bottom.svg) — visual placement reference (bottom side)

## Why a skeleton (not a full project)

Generating a fully-routed KiCad PCB by hand in S-expression text is error-prone (~5000 lines of valid syntax with UUIDs, layer indexes, net assignments). The skeleton captures the **non-trivial geometric setup** (board outline + mounting holes at exact Due positions, layer stackup, sensible defaults) and lets KiCad's GUI handle what it does well (footprint placement, routing, plotting).

If the skeleton is missing something obvious you'd want pre-built, ask and it can be added.
