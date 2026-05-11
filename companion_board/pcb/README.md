# PCB — phycommander I/O Backplane (Arduino Due Shield)

KiCad project skeleton. **Current revision: v4.** Open `pcb_backplane_v4.kicad_pro` with KiCad 8.x to start. The older `pcb_backplane_v3.*` files are kept for history (smaller 101.6 × 53.34 mm board, 2-layer home-fab target, never fabricated).

For the design rationale and full pinout see `docs/technical/PCB_BACKPLANE_PINOUT.md` (v4).

## What's in this skeleton (v4)

- **`pcb_backplane_v4.kicad_pro`** — KiCad project file (defaults inherited from v3; layer count to be set to 4 in KiCad once you open it: `File → Board Setup → Board Stackup → Layers`).
- **`pcb_backplane_v4.kicad_pcb`** — PCB layout containing:
  - Board outline **120 × 100 mm** rectangle (Arduino Due footprint flushed to the SW corner; the +18.4 mm east margin and +46.7 mm north area carry the 10 D-Sub IDC connectors and the CAN transceiver subcircuit)
  - **8 × M3 mounting holes** (NPTH, Ø 3.2 mm): 4 at Arduino Due native positions + 4 at backplane corners
  - **JTAG cutout** in `Edge.Cuts`: rectangular through-cut 14 × 7 mm above the Due-J1 1.27 mm SMD JTAG header (allows a custom pigtail to come up from the Due to the backplane top side)
  - Arduino Due footprint outline on `F.Fab` (visual reference, not for fab)
  - Silkscreen text guides on `F.SilkS` indicating where to place each of the 10 D-Sub IDC connectors (J1..J10), the JTAG bridge header (J11), the panel button + LED header (J12 + R2), and the CAN transceiver area (U1, R1, SW1, C1..C4)
  - Bottom-side `B.SilkS` reminders for the 8 Arduino mating headers (H_PWR, H_ANA, H_AEXT*, H_COM, H_UART*, H_DIG, H_D22, H_DAC). The two starred (*) headers target Due-extension positions and **must be verified against an actual Arduino Due** before fab (see `PCB_BACKPLANE_PINOUT.md` §1.4)
  - Title block + board ID silkscreen
  - **No** routed traces, no nets, no connector footprints yet — that's your job (see workflow below)

## What's NOT here yet (v4)

The skeleton is INTENTIONALLY minimal so you can drive the rest in KiCad's GUI:
- Schematic file (`.kicad_sch`) — create from scratch in Eeschema (no auto-link to PCB needed if you work directly in PCB editor)
- Layer count: skeleton declares 4 signal layers (`F.Cu`, `In1.Cu` for GND, `In2.Cu` for PWR, `B.Cu`), but reopen `Board Setup → Board Stackup` after first load to confirm dielectric constants and copper weights match the fab vendor's standard 4-layer stackup.
- Connector footprints (D-Sub IDC headers + Arduino mating headers) — drag from KiCad standard library
- MCP2562FD DIP-8 footprint — `Package_DIP:DIP-8_W7.62mm` from standard library
- Traces — route in PCB editor with push-and-shove router or autoroute
- GND plane (`In1.Cu`) and PWR plane (`In2.Cu`, split for +3V3 / +5V) — add via `Place → Filled Zone`
- Netlist — define via schematic, or add nets directly in PCB editor

## Workflow (estimated 4–6 hours for v4 first iteration)

### Step 1 — Open project (1 min)
```
$ cd companion_board/pcb/
$ open pcb_backplane_v4.kicad_pro
```
Or double-click the `.kicad_pro` file in Finder. KiCad will show the project manager; open the PCB by clicking the PCB icon.

### Step 2 — Verify board outline + holes + cutout (2 min)
You should see:
- Yellow rectangle = board outline (Edge.Cuts layer), 120 × 100 mm
- A small inner rectangle in Edge.Cuts at upper-left = JTAG cutout (14 × 7 mm)
- 8 small circles labelled MH1–MH8 = mounting holes (4 Due-native + 4 backplane corners)
- Silkscreen text on `F.SilkS` indicating top-side connector positions (J1..J12, R2) and the CAN xceiver area (U1, R1, SW1, C1..C4)
- Silkscreen text on `B.SilkS` indicating bottom-side mating header positions (H_PWR, H_ANA, H_AEXT*, H_COM, H_UART*, H_DIG, H_D22, H_DAC)

### Step 3 — Add D-Sub IDC + JTAG bridge headers on TOP side (15 min)

For each of 11 connectors, use **Add Footprint** (`A` shortcut). Positions below are in KiCad coords (Y-down) and match the silkscreen guides.

| Reference | Footprint (KiCad library) | Position (mm) | Layer |
|---|---|---|---|
| J1 (DB25 #1) | `Connector_PinHeader_2.54mm:PinHeader_2x13_P2.54mm_Vertical` | (26.5, 12) | F.Cu |
| J2 (DB25 #2) | `Connector_PinHeader_2.54mm:PinHeader_2x13_P2.54mm_Vertical` | (71.5, 12) | F.Cu |
| J3 (DB15 #1) | `Connector_PinHeader_2.54mm:PinHeader_2x08_P2.54mm_Vertical` | (20, 28) | F.Cu |
| J4 (DB15 #2) | `Connector_PinHeader_2.54mm:PinHeader_2x08_P2.54mm_Vertical` | (55, 28) | F.Cu |
| J5 (DB15 #3) | `Connector_PinHeader_2.54mm:PinHeader_2x08_P2.54mm_Vertical` | (90, 28) | F.Cu |
| J6 (DB9 #1) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (14.5, 42) | F.Cu |
| J7 (DB9 #2) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (33.5, 42) | F.Cu |
| J8 (DB9 #3) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (52.5, 42) | F.Cu |
| J9 (DB9 CAN, rear) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (71.5, 42) | F.Cu |
| J10 (DB9 JTAG, rear) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (90.5, 42) | F.Cu |
| J11 (JTAG bridge) | `Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical` | (109.5, 42) | F.Cu |
| J12 (panel button + LED, 1×4) | `Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical` | (8, 60) | F.Cu |
| R2 (220 Ω LED current limit) | `Resistor_THT:R_Axial_DIN0207_L6.3mm_D2.5mm_P10.16mm_Horizontal` | (8, 56) | F.Cu |
| R3 (10 kΩ nTRST pull-up) | `Resistor_THT:R_Axial_DIN0207_L6.3mm_D2.5mm_P10.16mm_Horizontal` | (108, 49) | F.Cu |

After placing each, delete the corresponding silkscreen text guide.

### Step 4 — Add Arduino mating headers on BOTTOM side (10 min)

These plug DOWN into the Arduino Due's female headers. Use **Flip** (`F` shortcut) after placement to put them on B.Cu.

| Reference | Footprint | Position (mm, KiCad Y-down) | Layer | Notes |
|---|---|---|---|---|
| H_PWR | `Connector_PinHeader_2.54mm:PinHeader_1x08_P2.54mm_Vertical` | (14, 98.5) | B.Cu | Standard Mega R3 |
| H_ANA | `Connector_PinHeader_2.54mm:PinHeader_1x08_P2.54mm_Vertical` | (42, 98.5) | B.Cu | Standard Mega R3 (A0..A7) |
| H_AEXT | `Connector_PinHeader_2.54mm:PinHeader_1x06_P2.54mm_Vertical` | (62, 98.5) | B.Cu | **Verify against real Due** |
| H_COM | `Connector_PinHeader_2.54mm:PinHeader_1x10_P2.54mm_Vertical` | (20, 49) | B.Cu | Standard |
| H_UART | `Connector_PinHeader_2.54mm:PinHeader_1x08_P2.54mm_Vertical` | (50, 49) | B.Cu | **Verify against real Due** |
| H_DIG | `Connector_PinHeader_2.54mm:PinHeader_1x08_P2.54mm_Vertical` | (75, 49) | B.Cu | Standard (D0..D7) |
| H_D22 | `Connector_PinHeader_2.54mm:PinHeader_2x18_P2.54mm_Vertical` | (97, 75), rotation 90° | B.Cu | Standard |
| H_DAC | `Connector_PinHeader_2.54mm:PinHeader_1x06_P2.54mm_Vertical` | (97, 50), rotation 90° | B.Cu | Due-extension |

> ⚠ **Verify Arduino header positions against the actual Due board** before final routing. The starred headers (`H_AEXT`, `H_UART`) target standard Mega R3 / Due R3 extension positions but Due revisions vary by 1–2 mm. Easiest verification: print `pcb_backplane_v4.kicad_pcb` at 1:1 from KiCad (`File → Plot → PDF`), overlay on a real Due, mark any offsets, then nudge the v4 footprints.
>
> Even better: import a community-maintained Arduino Due footprint from SnapEDA, Ultra Librarian, or the Eagle reference design from arduino.cc. The KiCad official library still doesn't include a dedicated `Arduino_Due` footprint, only `Arduino_UNO_R3` (which is shorter than the Due).

### Step 5 — Add CAN transceiver subcircuit (10 min)

In the east margin (KiCad x≈102.5..117.5, y≈51..95), place:

| Reference | Footprint | Position (mm) | Layer |
|---|---|---|---|
| U1 (MCP2562FD-E/P) | `Package_DIP:DIP-8_W7.62mm` (use socket; e.g. `Package_DIP:DIP-8_W7.62mm_Socket`) | (110, 65) | F.Cu |
| R1 (120 Ω) | `Resistor_THT:R_Axial_DIN0207_L6.3mm_D2.5mm_P10.16mm_Horizontal` | (110, 78) | F.Cu |
| SW1 (DPST slide) | e.g. `Switch_THT:SW_DIP_SPSTx02_Slide_9.78x6.7mm_W7.62mm_P2.54mm` | (110, 86) | F.Cu |
| C1, C2 (10 µF / 16 V) | `Capacitor_THT:CP_Radial_D5.0mm_P2.50mm` | (104, 72), (114, 72) | F.Cu |
| C3, C4 (100 nF) | `Capacitor_THT:C_Disc_D3.0mm_W2.0mm_P2.54mm` | (104, 58), (114, 58) | F.Cu |

### Step 6 — Define nets (30–60 min)

Two options:

**(A) Schematic-driven** (recommended for v4 because of the active component): create a `.kicad_sch` in Eeschema, place all 11 D-Sub IDC + 8 mating headers + MCP2562FD + passives + JTAG bridge, draw net labels per `PCB_BACKPLANE_PINOUT.md` §4 and §5, then `Tools → Update PCB from Schematic`.

**(B) PCB-direct**: in PCBnew, click each pad and assign nets manually via the properties dialog. Workable but tedious for ~80 nets.

Net list reference (`PCB_BACKPLANE_PINOUT.md` §4–§5):
- 16 DIN, 16 DOUT (each goes to 2 D-Sub IDCs)
- 12 ADC, 2 DAC, 8 PWM
- UART1, UART2 (TX/RX each), I2C0 (SDA/SCL)
- CAN_RXD / CAN_TXD (TTL, internal between H_DAC and U1)
- CAN_H, CAN_L (differential, U1 to J9)
- JTAG: VTREF, TMS, TCK, TDO, TDI, nRESET, GND (J11 to J10)
- Power: +3V3, +5V, GND, AREF
- Total: ~80 nets

### Step 7 — Add power planes (5 min)

`Place → Filled Zone`:
- On `In1.Cu` (GND plane): polygon covering the entire board outline (minus the JTAG cutout). Set net to GND.
- On `In2.Cu` (PWR plane): two zones, split: one polygon covering most of the board with net `+3V3`, one polygon (smaller, around U1 and DB9-3 area) with net `+5V`. Add a 0.5 mm gap between the two zones.

### Step 8 — Route signals on F.Cu and B.Cu (60–120 min)

Use the built-in router (`X` shortcut) with push-and-shove. Recommended assignment (per `PCB_BACKPLANE_PINOUT.md` §9):
- **F.Cu (top)**: short routes from D-Sub IDC headers to vias bound for B.Cu, plus local CAN xceiver routing (CAN_H/CAN_L differential pair stays on F.Cu)
- **B.Cu (bottom)**: long signal routes from H_D22 westward and northward to the D-Sub IDC vias

CAN_H / CAN_L routing rules: use Diff Pair tool (`6` then `Route → Differential Pair`), 0.4 mm width, 0.4 mm gap, length-matched within 0.5 mm.

### Step 9 — DRC check (5 min)

`Inspect → Design Rules Checker`. Fix any errors before exporting Gerbers.

### Step 10 — Export Gerbers + drill files for fab (5 min)

`File → Fabrication Outputs → Gerbers`:
- **Layers**: F.Cu, In1.Cu, In2.Cu, B.Cu, F.Mask, B.Mask, F.SilkS, B.SilkS, F.Paste, B.Paste, Edge.Cuts
- **Output directory**: `gerbers/`
- **Use Protel filename extensions**: yes (PCBWay prefers it)
- Click **Plot**

`File → Fabrication Outputs → Drill Files`:
- **Drill file format**: Excellon
- **Drill origin**: Absolute
- Click **Generate Drill File**

Zip the entire `gerbers/` directory and upload to PCBWay (or your fab service of choice). Standard 4-layer FR-4, 1.6 mm thick, 1 oz copper, HASL or ENIG finish, green soldermask, white silkscreen.

## Footprint library availability

All footprints used are in the **default KiCad standard library** (installed with KiCad 8). No external libraries required for the skeleton:
- `Connector_PinHeader_2.54mm` — 2.54mm pin headers (D-Sub IDC + Arduino mating headers)
- `MountingHole` — mounting hole footprints (M3 NPTH)
- `Package_DIP` — DIP-8 package for MCP2562FD
- `Resistor_THT`, `Capacitor_THT` — through-hole passives for the CAN subcircuit
- `Switch_THT` — through-hole DPST slide switch for the CAN termination

If KiCad reports "footprint not found", check:
```
Preferences → Manage Footprint Libraries → Global Libraries
```
and ensure the libraries above are enabled (default install: `${KICAD8_FOOTPRINT_DIR}/<libname>.pretty`).

## Reference docs in this repo

- [`docs/technical/PCB_BACKPLANE_PINOUT.md`](../../docs/technical/PCB_BACKPLANE_PINOUT.md) — full pinout, BOM, signal routing strategy
- [`docs/technical/DEVICE_PINOUT.md`](../../docs/technical/DEVICE_PINOUT.md) — front + rear panel context
- [`panel_sketch/pcb_backplane_top.svg`](../panel_sketch/pcb_backplane_top.svg) — visual placement reference (top side)
- [`panel_sketch/pcb_backplane_bottom.svg`](../panel_sketch/pcb_backplane_bottom.svg) — visual placement reference (bottom side)

## Why a skeleton (not a full project)

Generating a fully-routed KiCad PCB by hand in S-expression text is error-prone (~5000 lines of valid syntax with UUIDs, layer indexes, net assignments). The skeleton captures the **non-trivial geometric setup** (board outline + mounting holes at exact Due positions, layer stackup, sensible defaults) and lets KiCad's GUI handle what it does well (footprint placement, routing, plotting).

If the skeleton is missing something obvious you'd want pre-built, ask and it can be added.
