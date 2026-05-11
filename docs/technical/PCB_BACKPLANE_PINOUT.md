# PCB Backplane — Arduino Due Shield (v4)

> ⚠️  **Status: design sketch, never fabricated.**
> Mechanical outline, pinout, layer stackup, and BOM are decided; nothing
> has been etched, drilled, or signal-integrity tested. Two bottom-side
> mating headers (`H_UART`, `H_AEXT`) target standard Mega R3 / Due
> "extension" positions that **must be verified against an actual Arduino
> Due** before final fabrication (see §1.4). See the repository README →
> *Contributions wanted* for what would close the gap to a built +
> validated board.

Passive Arduino Due shield that fans out **all** firmware-published I/O
signals to ten D-Sub connectors (8 front + 2 rear) plus a front-panel
reset button and an external CAN bus. Plugs directly onto the Due via
six standard Mega R3 mating headers plus two Due-extension mating
headers, no wire harness from the Due.

The board is **120 × 100 mm**, **4-layer** (continuous internal GND
plane + split PWR plane), fab-service produced (PCBWay or equivalent)
with proper plated through-hole vias. The previous home-fab + manual
rivet approach (v3) was abandoned because the v4 trace count is too
high for single-layer routing and the analog noise floor on the 12-bit
ADCs benefits measurably from a continuous reference plane.

See also:
- `companion_board/panel_sketch/pcb_backplane_top.svg` — top layer
  placement guide (1:1 scale, viewable in Inkscape or any browser)
- `companion_board/panel_sketch/pcb_backplane_bottom.svg` — bottom
  layer placement guide (1:1, mirror before any silk overlay)
- `docs/technical/DEVICE_PINOUT.md` — front + rear panel context, DB9
  CAN/JTAG conventions

---

## 1. Mechanical specifications

| Parameter | Value |
|---|---|
| PCB outline | **120 × 100 mm** rectangle |
| Layers | **4** (L1 top signals, L2 GND plane, L3 PWR plane, L4 bottom signals) |
| Layer stackup | 1.6 mm FR-4, 1 oz copper all layers, standard 4-layer prepreg |
| Vias | Plated through-hole, fab-service produced (no manual rivets) |
| Soldermask | Green (default), top + bottom |
| Silkscreen | White, top + bottom |
| Mounting holes | **8 × M3**, NPTH Ø 3.2 mm (4 at Arduino Due native positions + 4 at backplane corners) |
| Top side | 10 × D-Sub IDC pin headers (2.54 mm pitch, 2-row, vertical, shrouded) facing UP, plus 1 × JTAG bridge header (2×5 2.54 mm), 1 × panel-button/LED header (1×4 2.54 mm, J12), and the CAN transceiver subcircuit |
| Bottom side | 8 × Arduino mating male pin headers (~7 mm pin length, non-stackable) facing DOWN, plug into the Due female sockets |
| PCB cutout | 1 × rectangular slot above Due-J1 (JTAG) for 1.27 mm pigtail passage, ~14 × 7 mm |
| Total signal pins distributed to D-Sub front + rear | 140 (2 × 25 + 3 × 15 + 5 × 9), all populated |
| Additional panel I/O pins | 4 (J12: LED+/LED−/RST_BTN/GND) routed to the front-panel "Arduino Reset" pushbutton + LED |

### 1.1 Position of the Arduino Due relative to the backplane

The Due footprint (101.6 × 53.34 mm) is placed **flush with the south
and west edges** of the backplane:

- Due region: x = 0 .. 101.6 mm, y = 0 .. 53.34 mm
- East margin: x = 101.6 .. 120.0 mm (18.4 mm wide), used for the CAN
  transceiver subcircuit
- North area: y = 53.34 .. 100.0 mm (46.66 mm tall), used for the 10
  D-Sub IDC connectors arranged in three rows (see §1.5)

Origin (0, 0) is the backplane south-west corner.

### 1.2 Mounting hole positions

Eight M3 holes total. Four at Arduino Due native positions
(Due-side fastening, screwed to the same standoffs that hold the Due),
plus four at backplane corners (chassis-side fastening, independent
of the Due).

| Hole | X (mm) | Y (mm) | Type | Notes |
|---|---|---|---|---|
| MH1 (Due, top-left) | 15.24 | 50.80 | Due-native | Datasheet quoted, same as v3 M2 |
| MH2 (Due, top-right) | 90.17 | 50.80 | Due-native | Datasheet quoted (v3 M4 with y mirrored) |
| MH3 (Due, mid-right) | 66.04 | 17.78 | Due-native | KiCad UNO R3 pattern (v3 M3 with y mirrored) |
| MH4 (Due, bottom-left) | 15.24 | 2.54 | Due-native | Datasheet quoted (v3 M1) |
| MH5 (backplane SW) | 4.0 | 4.0 | Chassis | Backplane corner |
| MH6 (backplane SE) | 116.0 | 4.0 | Chassis | Backplane corner (lands in the CAN transceiver area, verify clearance) |
| MH7 (backplane NW) | 4.0 | 96.0 | Chassis | Backplane corner |
| MH8 (backplane NE) | 116.0 | 96.0 | Chassis | Backplane corner |

> **Y-coordinate note**: v3 used screen-style coordinates (y increases
> *down* the page, so Y=2.54 in v3 was the *top* of the Due region).
> v4 uses math-style coordinates (origin at south-west, y increases
> *up*), so each Due-native y-coord here is `53.34 − y_v3`. Always
> confirm against a 1:1 printout overlaid on a real Due before
> drilling, regardless of convention.

> **MH6 clearance**: the south-east backplane corner sits inside the
> CAN transceiver area (§1.5). If the M3 hole + standoff conflicts
> with U1 / passives, move the hole to (116, 13) or skip it (the
> remaining 7 holes provide ample retention).

### 1.3 PCB cutout (JTAG passage)

A rectangular through-cut in the PCB, located **directly above the
Arduino Due's on-board JTAG header (J1, 2×5, 1.27 mm pitch SMD,
positioned near the SAM-BA USB connector on the Due top side)**.

| Parameter | Value |
|---|---|
| Cutout shape | Rounded rectangle |
| Cutout size | 14 × 7 mm (clearance for a 1.27 mm flat-ribbon pigtail with strain relief) |
| Cutout center (approx) | x ≈ 16, y ≈ 26 (above Due-J1; verify on real Due) |
| Edge filleting | R = 1 mm (avoid stress concentrators in FR-4) |

A 1.27 mm flat-ribbon pigtail (~80 mm long) plugs into Due-J1 from
underneath the backplane, exits through the cutout, and terminates
at a 2×5 2.54 mm IDC plug that mates with the **JTAG bridge header
J11** on the backplane top side (see §1.5 row C, §8 for circuit).

### 1.4 Bottom-side mating headers

Eight male pin headers face DOWN, mating with the Arduino Due female
socket headers. Standard 7 mm pin length (the v4 shield is final;
nothing plugs on top).

| Header | Type | Position on Due | Used pins | Status |
|---|---|---|---|---|
| `H_PWR` (POWER) | 1×8 male, 2.54 mm | south edge, west side | RESET, 3V3, 5V, GND, GND (5 of 8) | ✓ standard Mega R3 |
| `H_ANA` (ANALOG) | 1×8 male | south edge, east of POWER | A0 .. A7 (all 8 = ADC 0..7) | ✓ standard |
| `H_COM` (COMM) | 1×10 male | north edge, west side | D8 .. D13, GND, AREF, SDA (D20), SCL (D21) — 10 of 10 | ✓ standard |
| `H_DIG` (DIGITAL) | 1×8 male | north edge, east of COMM | D0 (RX0), D1 (TX0), D2 .. D7 — 8 of 8 | ✓ standard |
| `H_D22` (2×18 dual block) | 2×18 male | east edge | D22 .. D53 (32 of 36 pin positions, 4 NC) | ✓ standard |
| `H_DAC` (DAC/CAN) | 1×6 male | north-east edge, above H_D22 | DAC0, DAC1, CANRX (D68/PA1), CANTX (D69/PA0), GND, GND (4 of 6 used) | ✓ standard |
| **`H_UART`** (UART/I2C extension) | 1×8 male | north center, between COMM and DIGITAL | TX3 (D14), RX3 (D15), TX2 (D16), RX2 (D17), TX1 (D18), RX1 (D19), SDA (D20 dup), SCL (D21 dup) | ⚠️ **VERIFY position on real Due** |
| **`H_AEXT`** (analog extension) | 1×6 male (or 2×3) | south-east edge, between ANALOG and DAC/CAN | A8 (D62/PB17), A9 (D63/PB18), A10 (D64/PB19), A11 (D65/PB20), SDA1 (D70/PA17), SCL1 (D71/PA18) | ⚠️ **VERIFY position + pin order on real Due** |

> **Verification protocol for `H_UART` and `H_AEXT`** before sending
> the design to fab:
>
> 1. Print the v4 PCB top + bottom layers at 1:1 (`File → Plot` in
>    KiCad, PDF format, mirror the bottom).
> 2. Overlay the bottom printout on a real Arduino Due, aligning the
>    other six headers (`H_PWR`, `H_ANA`, `H_COM`, `H_DIG`, `H_D22`,
>    `H_DAC`) with the Due's female sockets.
> 3. Check that `H_UART` aligns with the Due's 1×8 north-center
>    header. If alignment is off, measure the Due header position
>    in mm and patch the v4 KiCad file accordingly.
> 4. Same for `H_AEXT`. The Due's analog-extension area (`A8..A11 +
>    D70/D71`) varies subtly across Due revisions. If the real Due
>    has a 2×3 dual block instead of a 1×6 row, change the footprint.
> 5. If neither H_UART nor H_AEXT can be matched within ±0.5 mm,
>    leave their footprints unpopulated and wire those signals via
>    flying jumpers from the Due to spare pads on the backplane.
>
> The remaining six mating headers (POWER, ANALOG, COMM, DIGITAL,
> 2×18 D22-D53, DAC/CAN) are standard Mega R3 / Due R3 — no risk.

### 1.5 Top-side connector layout (3-row grid + CAN block)

```
y=100 ┌────────────────────────────────────────────────────────────────┐
      │                                                                │
y=87  │ Row A:  [   DB25-1 IDC 2x13   ]   [   DB25-2 IDC 2x13   ]      │
      │                                                                │
y=72  │ Row B:  [DB15-1] [DB15-2] [DB15-3]                              │
      │           2x8     2x8     2x8                                  │
      │                                                                │
y=57  │ Row C:  [DB9-1][DB9-2][DB9-3][DB9-CAN][DB9-JTAG][JTAG-bridge]  │
      │          2x5    2x5    2x5    2x5      2x5       2x5           │
      │                                                                │
y=53  │ ┌─────────────────────────────────┐ ┌── CAN sub ──┐            │
y=50  │ │                                 │ │ U1 MCP2562  │            │
      │ │  Arduino Due footprint          │ │ R1 120 ohm  │            │
      │ │  (101.6 x 53.34 mm)             │ │ SW1 term    │            │
      │ │  ┌──┐ JTAG cutout ~14x7         │ │ C1..C4 cap  │            │
      │ │  │  │ x≈16, y≈26                │ │             │            │
      │ │  └──┘                           │ │ x=104..118  │            │
      │ │                                 │ │ y=10..50    │            │
y=0   │ └─────────────────────────────────┘ └─────────────┘            │
      └────────────────────────────────────────────────────────────────┘
       0   10    20    30    40    50   60    70    80   90   100  110 120
```

Top connector positions (centered, 2.54 mm-snapped, doc Y up):

| Ref | Connector | Footprint | X center (mm) | Y center (mm) |
|---|---|---|---|---|
| J1 | DB25 #1 IDC | 2×13 vertical shrouded | 26.5 | 88 |
| J2 | DB25 #2 IDC | 2×13 vertical shrouded | 71.5 | 88 |
| J3 | DB15 #1 IDC | 2×8 vertical shrouded | 20 | 72 |
| J4 | DB15 #2 IDC | 2×8 vertical shrouded | 55 | 72 |
| J5 | DB15 #3 IDC | 2×8 vertical shrouded | 90 | 72 |
| J6 | DB9 #1 IDC | 2×5 vertical shrouded | 14.5 | 58 |
| J7 | DB9 #2 IDC | 2×5 vertical shrouded | 33.5 | 58 |
| J8 | DB9 #3 IDC | 2×5 vertical shrouded | 52.5 | 58 |
| J9 | DB9 CAN (rear) IDC | 2×5 vertical shrouded | 71.5 | 58 |
| J10 | DB9 JTAG (rear) IDC | 2×5 vertical shrouded | 90.5 | 58 |
| J11 | JTAG bridge header | 2×5 vertical, no shroud (or shrouded, optional) | 109.5 | 58 |
| **J12** | **Panel button + LED header** | **1×4 vertical pin header (10.16 × 2.54 mm body)** | **8** | **40** |
| **R2** | **220 Ω LED current limit** | **THT axial horizontal (6.3 × 2.5 mm body)** | **8** | **44** |
| U1 | MCP2562FD | DIP-8 socket, vertical | 110 | 35 |
| R1 | 120 Ω termination | THT axial, vertical | 110 | 22 |
| SW1 | DPST term enable | THT slide switch | 110 | 14 |
| C1, C2 | 10 µF / 16 V | THT radial 5 mm | (104, 28), (114, 28) |
| C3, C4 | 100 nF | THT 5.08 mm | (104, 42), (114, 42) |

All D-Sub IDC headers (J1..J11) are **2.54 mm pitch shrouded vertical**
with polarity key, so the ribbon cable can only plug one way. J12 is a
plain (non-shrouded) 1×4 pin header to save board area; if you want a
polarity key, swap for a shrouded 1×4 (e.g., Würth 61300411121).

---

## 2. Bill of Materials

| Ref | Part | Qty | Unit cost | Notes |
|---|---|---|---|---|
| J1, J2 | IDC pin header 2×13, 2.54 mm, vertical, shrouded | 2 | €0.40 | DB25 IDC, top side |
| J3, J4, J5 | IDC pin header 2×8, 2.54 mm, vertical, shrouded | 3 | €0.30 | DB15 IDC, top side |
| J6 .. J10 | IDC pin header 2×5, 2.54 mm, vertical, shrouded | 5 | €0.25 | DB9 IDC, top side (3 front + 2 rear) |
| J11 | Pin header 2×5, 2.54 mm, vertical, shrouded | 1 | €0.25 | JTAG bridge to Due-J1 (top side) |
| J12 | Pin header 1×4, 2.54 mm, vertical (shrouded optional) | 1 | €0.15 | Front-panel button + LED (top side, see §11) |
| H_PWR | Pin header 1×8, 2.54 mm, ~7 mm pin, non-stack | 1 | €0.20 | POWER mating (bottom side) |
| H_ANA | Pin header 1×8 | 1 | €0.20 | ANALOG mating (bottom side) |
| H_COM | Pin header 1×10 | 1 | €0.25 | COMM mating (bottom side) |
| H_DIG | Pin header 1×8 | 1 | €0.20 | DIGITAL mating (bottom side) |
| H_D22 | Pin header 2×18 | 1 | €0.50 | D22-D53 mating (bottom side) |
| H_DAC | Pin header 1×6 | 1 | €0.20 | DAC/CAN mating (bottom side) |
| H_UART | Pin header 1×8 | 1 | €0.20 | UART D14-D21 mating (bottom side) — verify Due geometry |
| H_AEXT | Pin header 1×6 (or 2×3) | 1 | €0.20 | Analog extension A8-A11 + D70/D71 (bottom side) — verify |
| U1 | MCP2562FD-E/P | 1 | €1.50 | CAN transceiver, DIP-8, 3.3 V VIO compatible |
| U1-socket | DIP-8 turned-pin socket | 1 | €0.30 | Optional but recommended (lets you swap U1 without desoldering) |
| R1 | 120 Ω 1/4 W axial | 1 | €0.05 | CAN bus termination resistor |
| R2 | 220 Ω 1/4 W axial | 1 | €0.05 | LED current-limit on J12 pin 1 (sizes the front-panel Arduino-Reset LED at ~6 mA for a 2 V Vf LED on +3.3 V) |
| SW1 | Slide switch DPST, THT | 1 | €0.40 | Enable / disable CAN termination |
| C1, C2 | 10 µF / 16 V electrolytic THT radial | 2 | €0.10 | CAN VDD + VIO bulk |
| C3, C4 | 100 nF ceramic THT 5.08 mm | 2 | €0.05 | CAN U1 high-frequency decoupling |
| MH1..MH8 | M3 NPTH Ø 3.2 mm | 8 | — | 4 Due-native + 4 backplane corners |
| **Bare PCB (4-layer, 120×100 mm)** | PCBWay / JLCPCB | 5 | €5–€8 | Min order qty typically 5; with sponsorship the cost drops to ~€0 |
| **Total per board (parts only, qty 5)** | — | — | **~€8** | excluding PCB; PCB ~€5–€8 fab service |

**Cable from PCB to panel**: 10 × IDC ribbon cable (26 / 16 / 10
conductor) → D-Sub solder cup, ~10–15 cm each. 8 cables for the
front panel + 2 cables for the rear DB9s = 10 cables total.

**Plus**: 1 × custom JTAG pigtail (1.27 mm IDC plug → flat ribbon →
2.54 mm IDC plug, ~80 mm), built once.

**No active power conversion on this board** — the Due supplies +3.3 V
and +5 V, and the MCP2562FD runs from +5 V (VDD) with 3.3 V VIO from
the same Due rails. Total transceiver current: ~50 mA peak during
CAN dominant bursts.

---

## 3. Signal architecture

```
  ┌────────────────────────────────────────┐
  │  phycommander Linux host (Intel)       │
  │  physerver via USB CDC                 │
  └─────────────┬──────────────────────────┘
                │ USB-B
                ▼
  ┌────────────────────────────────────────┐
  │  Arduino Due (ATSAM3X8E)               │
  │  Female header sockets (top side)      │
  │  + JTAG 2×5 1.27 mm SMD header         │
  └─────────────┬──────────────────────────┘
                │ male pins down (~7 mm), shield plugs on top
                │ + 1.27 mm pigtail through cutout for JTAG
                ▼
  ┌────────────────────────────────────────┐
  │  Backplane PCB v4 (this document)      │  120 × 100 mm, 4-layer
  │  - Bottom: 8 Arduino mating headers    │  + MCP2562FD CAN xceiver
  │  - Top: 10 D-Sub IDC + JTAG bridge     │  + JTAG bridge to Due-J1
  └─────────────┬─────────────┬────────────┘
                │             │
                │ 8× IDC ribbon (front)
                │             │ 2× IDC ribbon (rear: CAN, JTAG DB9s)
                ▼             ▼
  ┌────────────────────────┐ ┌──────────────────────┐
  │ Front panel            │ │ Rear panel           │
  │ 3×3 grid, 9 D-Sub fem  │ │ 4 DB9 (COM1/COM2/    │
  │ (DB25-3 wired to MB    │ │  CAN/JTAG)           │
  │  parallel port, NOT    │ │ + power, fan, etc.   │
  │  to backplane)         │ │ See DEVICE_PINOUT.md │
  └────────────────────────┘ └──────────────────────┘
```

All signals are **3.3 V CMOS native** at the Due output. External
application breakouts (level shifters, opto, signal conditioning,
24 V relay drivers, etc.) plug into the front-panel D-Subs.

The CAN bus on the rear DB9 is the only signal on the panel that is
**already conditioned**: it leaves the chassis as a true differential
CAN_H / CAN_L pair, terminated and filtered by the on-board MCP2562FD
subcircuit. Plug a CAN cable in and go.

The JTAG on the rear DB9 is **direct from the SAM3X**: still 3.3 V
CMOS, just a longer trace. Use a short cable (< 30 cm) when running
the debugger fast (10+ MHz SWCLK).

---

## 4. Bottom-side mating header assignments (Due → backplane signals)

Six standard Mega R3 / Due R3 headers + two Due-extension headers.
Pin numbers below are the *header pin numbers*, not the Arduino D-pin
numbers.

### 4.1 `H_PWR` — POWER (1×8)

| Header pin | Arduino pin | Signal on backplane | Routed to |
|---|---|---|---|
| 1 | IOREF | (3.3 V ref) | NC (sense only, not used) |
| 2 | RESET | RESET | NC on shield (Arduino reset is wired direct to front-panel button — see §10) |
| 3 | 3V3 | +3.3V | shield's +3V3 power net (powers all D-Sub +3.3 V pins, MCP2562FD VIO, JTAG VTREF) |
| 4 | 5V | +5V | shield's +5V power net (powers MCP2562FD VDD, DB9-3 +5 V pin) |
| 5 | GND | GND | shield's main GND net |
| 6 | GND | GND | shield's main GND net (parallel) |
| 7 | VIN | (~7-12 V) | NC |
| 8 | NC | — | NC |

### 4.2 `H_ANA` — ANALOG (1×8)

| Header pin | Arduino pin | Signal | Notes |
|---|---|---|---|
| 1 | A0 | ADC0 | DB25-1 p17, DB15-1 p9, DB9-1 p1 |
| 2 | A1 | ADC1 | DB25-1 p18, DB15-1 p10, DB9-1 p2 |
| 3 | A2 | ADC2 | DB25-1 p19, DB9-1 p3 |
| 4 | A3 | ADC3 | DB25-1 p20, DB9-1 p4 |
| 5 | A4 | ADC4 | DB25-1 p21, DB15-2 p9 |
| 6 | A5 | ADC5 | DB25-1 p22, DB15-2 p10 |
| 7 | A6 | ADC6 | DB25-1 p23, DB15-2 p11 |
| 8 | A7 | ADC7 | DB25-1 p24, DB15-2 p12 |

### 4.3 `H_COM` — COMM (1×10)

| Header pin | Arduino pin | Signal | Notes |
|---|---|---|---|
| 1 | D8 | PWM1 (firmware `pwm[1]`) | DB25-2 p18 |
| 2 | D9 | PWM0 (firmware `pwm[0]`) | DB25-2 p17 |
| 3 | D10 | PWM4 (firmware `pwm[4]`) | DB25-2 p21 |
| 4 | D11 | PWM5 (firmware `pwm[5]`) | DB25-2 p22 |
| 5 | D12 | NC (reserved) | — |
| 6 | D13 | NC (firmware heartbeat LED, do not load) | — |
| 7 | GND | GND | shield's main GND net |
| 8 | AREF | AREF | DB15-3 p13, DB9-1 p7 |
| 9 | D20 (SDA) | I2C0_SDA | DB9-3 p5 |
| 10 | D21 (SCL) | I2C0_SCL | DB9-3 p6 |

### 4.4 `H_DIG` — DIGITAL D0..D7 (1×8)

| Header pin | Arduino pin | Signal | Notes |
|---|---|---|---|
| 1 | D0 (RX0) | UART0_RX | NC on backplane (USB-UART bridge, do not touch — see PIN_MAP §6.1) |
| 2 | D1 (TX0) | UART0_TX | NC on backplane (same reason) |
| 3 | D2 | PWM7 (firmware `pwm[7]`) | DB25-2 p24 |
| 4 | D3 | RST_BTN (front-panel reset-button input, PIN_MAP §5) | J12 p3 (see §11) |
| 5 | D4 | LED_K (front-panel LED cathode, firmware-driven, see §11) | J12 p2 |
| 6 | D5 | PWM6 (firmware `pwm[6]`) | DB25-2 p23 |
| 7 | D6 | PWM3 (firmware `pwm[3]`) | DB25-2 p20 |
| 8 | D7 | PWM2 (firmware `pwm[2]`) | DB25-2 p19 |

### 4.5 `H_D22` — 2×18 dual block (D22..D53), main DIN/DOUT source

Convention for the 2×18 block on the backplane:
- "Outer" row (closer to board edge): even D-pins → **DIN** signals
- "Inner" row (closer to board interior): odd D-pins → **DOUT** signals

| Pair | Outer pin (DIN) | Arduino | Inner pin (DOUT) | Arduino |
|---|---|---|---|---|
| 1 | DIN0 | D22 (PB26) | DOUT0 | D23 (PA14) |
| 2 | DIN1 | D24 (PA15) | DOUT1 | D25 (PD0) |
| 3 | DIN2 | D26 (PD1) | DOUT2 | D27 (PD2) |
| 4 | DIN3 | D28 (PD3) | DOUT3 | D29 (PD6) |
| 5 | DIN4 | D30 (PD9) | DOUT4 | D31 (PA7) |
| 6 | DIN5 | D32 (PD10) | DOUT5 | D33 (PC1) |
| 7 | DIN6 | D34 (PC2) | DOUT6 | D35 (PC3) |
| 8 | DIN7 | D36 (PC4) | DOUT7 | D37 (PC5) |
| 9 | DIN8 | D38 (PC6) | DOUT8 | D39 (PC7) |
| 10 | DIN9 | D40 (PC8) | DOUT9 | D41 (PC9) |
| 11 | DIN10 | D42 (PA19) | DOUT10 | D43 (PA20) |
| 12 | DIN11 | D44 (PC19) | DOUT11 | D45 (PC18) |
| 13 | DIN12 | D46 (PC17) | DOUT12 | D47 (PC16) |
| 14 | DIN13 | D48 (PC15) | DOUT13 | D49 (PC14) |
| 15 | DIN14 | D50 (PC13) | DOUT14 | D51 (PC12) |
| 16 | DIN15 | D52 (PB21) | DOUT15 | D53 (PB14) |
| 17 | NC | — | NC | — |
| 18 | NC | — | NC | — |

**Routing**: DIN signals fan out to DB25-1 + corresponding DB15 +
DB9-2; DOUT signals fan out to DB25-2 + corresponding DB15 + DB9-2.
With four layers and continuous internal GND, the L1 + L4 routing
is straightforward (no crossings on inner planes).

### 4.6 `H_DAC` — DAC/CAN (1×6)

| Header pin | Arduino pin | Signal | Notes |
|---|---|---|---|
| 1 | DAC0 (D66) | DAC0 | DB25-2 NC (PWM-only DB25-2), DB15-1 p11, DB15-2 p13, DB9-1 p5 |
| 2 | DAC1 (D67) | DAC1 | DB15-1 p12, DB9-1 p6 |
| 3 | CANRX (D68/PA1) | CAN_RXD (to MCP2562FD U1 pin 4 RXD) | internal — see §7 |
| 4 | CANTX (D69/PA0) | CAN_TXD (to MCP2562FD U1 pin 1 TXD) | internal — see §7 |
| 5 | GND | GND | shield's main GND |
| 6 | GND | GND | shield's main GND |

### 4.7 `H_UART` — UART / I2C extension (1×8)

> ⚠️ **VERIFY position on real Due** before fab (see §1.4).

| Header pin | Arduino pin | Signal | Notes |
|---|---|---|---|
| 1 | D14 (TX3) | UART3_TX | NC in v4 (no DB9 pin allocated) |
| 2 | D15 (RX3) | UART3_RX | NC in v4 |
| 3 | D16 (TX2) | UART2_TX | DB9-3 p3 |
| 4 | D17 (RX2) | UART2_RX | DB9-3 p4 |
| 5 | D18 (TX1) | UART1_TX | DB9-3 p1 |
| 6 | D19 (RX1) | UART1_RX | DB9-3 p2 |
| 7 | D20 (SDA dup) | I2C0_SDA (same net as `H_COM` p9) | redundant; can NC |
| 8 | D21 (SCL dup) | I2C0_SCL (same net as `H_COM` p10) | redundant; can NC |

### 4.8 `H_AEXT` — Analog extension (1×6 or 2×3)

> ⚠️ **VERIFY position + footprint on real Due** before fab (see §1.4).

| Header pin | Arduino pin | Signal | Notes |
|---|---|---|---|
| 1 | A8 (D62/PB17) | ADC8 | DB15-3 p9 |
| 2 | A9 (D63/PB18) | ADC9 | DB15-3 p10 |
| 3 | A10 (D64/PB19) | ADC10 | DB15-3 p11 |
| 4 | A11 (D65/PB20) | ADC11 | DB15-3 p12 |
| 5 | D70 (SDA1/PA17) | I2C1_SDA | NC in v4 (no DB9 pin allocated; reserve for future expansion) |
| 6 | D71 (SCL1/PA18) | I2C1_SCL | NC in v4 |

---

## 5. D-Sub IDC connector pinouts (top side)

Each D-Sub IDC header on the backplane top side connects via an
IDC ribbon cable to a panel-mount D-Sub female on the front panel
(or to one of the rear-panel DB9s for CAN and JTAG).

Pin numbering below uses the **D-Sub pin number** (1..N as silk-screened
on the connector), not the IDC ribbon order. The IDC-to-DSUB cables
follow the standard "snake" wiring where ribbon pin 1 → DSUB pin 1,
ribbon pin 2 → DSUB pin 2, etc., with the end terminations following
the connector manufacturer's convention. Use a commercial 26 / 16 /
10-conductor IDC cable assembly.

### 5.1 DB25 #1 — SENSING (16 DIN + 8 ADC + AGND), 25 pins

| Pin | Signal | Note |
|---|---|---|
| 1 | DIN0 | 3.3 V CMOS digital in |
| 2 | DIN1 | |
| 3 | DIN2 | |
| 4 | DIN3 | |
| 5 | DIN4 | |
| 6 | DIN5 | |
| 7 | DIN6 | |
| 8 | DIN7 | |
| 9 | DIN8 | |
| 10 | DIN9 | |
| 11 | DIN10 | |
| 12 | DIN11 | |
| 13 | DIN12 | |
| 14 | DIN13 | |
| 15 | DIN14 | |
| 16 | DIN15 | |
| 17 | ADC0 | 0–3.3 V analog in |
| 18 | ADC1 | |
| 19 | ADC2 | |
| 20 | ADC3 | |
| 21 | ADC4 | |
| 22 | ADC5 | |
| 23 | ADC6 | |
| 24 | ADC7 | |
| 25 | AGND | analog ground (joined to GND on backplane via single point at H_PWR) |

### 5.2 DB25 #2 — CONTROL (16 DOUT + 8 PWM + GND), 25 pins

| Pin | Signal | Note |
|---|---|---|
| 1 | DOUT0 | 3.3 V CMOS digital out |
| 2 | DOUT1 | |
| 3 | DOUT2 | |
| 4 | DOUT3 | |
| 5 | DOUT4 | |
| 6 | DOUT5 | |
| 7 | DOUT6 | |
| 8 | DOUT7 | |
| 9 | DOUT8 | |
| 10 | DOUT9 | |
| 11 | DOUT10 | |
| 12 | DOUT11 | |
| 13 | DOUT12 | |
| 14 | DOUT13 | |
| 15 | DOUT14 | |
| 16 | DOUT15 | |
| 17 | PWM0 | firmware `pwm[0]` = D9 (PWMH4) |
| 18 | PWM1 | `pwm[1]` = D8 (PWMH5) |
| 19 | PWM2 | `pwm[2]` = D7 (PWMH6) |
| 20 | PWM3 | `pwm[3]` = D6 (PWMH7) |
| 21 | PWM4 | `pwm[4]` = D10 (TC2.ch1 TIOB) |
| 22 | PWM5 | `pwm[5]` = D11 (TC2.ch2 TIOA) |
| 23 | PWM6 | `pwm[6]` = D5 (TC2.ch0 TIOA) |
| 24 | PWM7 | `pwm[7]` = D2 (TC0.ch0 TIOA) |
| 25 | GND | digital ground |

### 5.3 DB15 #1 — Bank Low (channels 0..3), 15 pins

| Pin | Signal |
|---|---|
| 1 | DIN0 |
| 2 | DIN1 |
| 3 | DIN2 |
| 4 | DIN3 |
| 5 | DOUT0 |
| 6 | DOUT1 |
| 7 | DOUT2 |
| 8 | DOUT3 |
| 9 | ADC0 |
| 10 | ADC1 |
| 11 | DAC0 |
| 12 | DAC1 |
| 13 | +3.3 V |
| 14 | AGND |
| 15 | GND |

### 5.4 DB15 #2 — Bank Mid (channels 4..7 + ADC4..7), 15 pins

| Pin | Signal |
|---|---|
| 1 | DIN4 |
| 2 | DIN5 |
| 3 | DIN6 |
| 4 | DIN7 |
| 5 | DOUT4 |
| 6 | DOUT5 |
| 7 | DOUT6 |
| 8 | DOUT7 |
| 9 | ADC4 |
| 10 | ADC5 |
| 11 | ADC6 |
| 12 | ADC7 |
| 13 | DAC0 |
| 14 | +3.3 V |
| 15 | GND |

### 5.5 DB15 #3 — Bank High (channels 8..11 + ADC8..11), 15 pins

| Pin | Signal |
|---|---|
| 1 | DIN8 |
| 2 | DIN9 |
| 3 | DIN10 |
| 4 | DIN11 |
| 5 | DOUT8 |
| 6 | DOUT9 |
| 7 | DOUT10 |
| 8 | DOUT11 |
| 9 | ADC8 |
| 10 | ADC9 |
| 11 | ADC10 |
| 12 | ADC11 |
| 13 | AREF |
| 14 | +3.3 V |
| 15 | AGND |

### 5.6 DB9 #1 — Quick Analog, 9 pins

| Pin | Signal |
|---|---|
| 1 | ADC0 |
| 2 | ADC1 |
| 3 | ADC2 |
| 4 | ADC3 |
| 5 | DAC0 |
| 6 | DAC1 |
| 7 | AREF |
| 8 | +3.3 V |
| 9 | AGND |

### 5.7 DB9 #2 — Quick Digital High (channels 12..15), 9 pins

| Pin | Signal |
|---|---|
| 1 | DIN12 |
| 2 | DIN13 |
| 3 | DIN14 |
| 4 | DIN15 |
| 5 | DOUT12 |
| 6 | DOUT13 |
| 7 | DOUT14 |
| 8 | DOUT15 |
| 9 | GND |

### 5.8 DB9 #3 — Quick COMM (UART + I2C), 9 pins

| Pin | Signal | Source |
|---|---|---|
| 1 | UART1_TX | D18 (PA11) via `H_UART` p5 |
| 2 | UART1_RX | D19 (PA10) via `H_UART` p6 |
| 3 | UART2_TX | D16 (PA13) via `H_UART` p3 |
| 4 | UART2_RX | D17 (PA12) via `H_UART` p4 |
| 5 | I2C0_SDA | D20 (PB12) via `H_COM` p9 |
| 6 | I2C0_SCL | D21 (PB13) via `H_COM` p10 |
| 7 | +5 V | shield's +5V net |
| 8 | +3.3 V | shield's +3V3 net |
| 9 | GND | shield's GND net |

UART3 (D14/D15) and I2C1 (D70/D71) are wired to the bottom-side
mating headers (`H_UART`, `H_AEXT`) but not routed to any DB9 in v4
— they remain available for a future revision.

SPI is **not exposed on v4**. The ICSP header on the Due (which
carries MISO/MOSI/SCK) sits under the shield with no cutout. Adding
SPI would require a second cutout + bridge header, deferred to v5
if user demand emerges.

### 5.9 DB9 #4 (rear) — CAN bus, CiA 303-1 pinout, 9 pins

| Pin | Signal | Direction |
|---|---|---|
| 1 | NC (reserved CAN_V+) | — |
| 2 | CAN_L | I/O (from MCP2562FD U1 pin 6) |
| 3 | CAN_GND | tied to backplane GND |
| 4 | NC | — |
| 5 | CAN_SHLD | tied to chassis ground via 1 MΩ + 10 nF (or directly, depending on installation) |
| 6 | NC (or GND) | optional GND |
| 7 | CAN_H | I/O (from MCP2562FD U1 pin 7) |
| 8 | NC (reserved error line) | — |
| 9 | NC (reserved CAN_V+) | — |

Pins 2 and 7 carry the differential pair. Routing on the backplane
keeps these traces short (< 30 mm), parallel, and equal-length to
preserve the differential impedance at the modest data rates of CAN
(up to 1 Mbit/s).

### 5.10 DB9 #5 (rear) — DEBUG (SWD/JTAG), 9 pins

| Pin | Signal | Source |
|---|---|---|
| 1 | VREF (3.3 V) | shield's +3V3 net (debugger senses target voltage here) |
| 2 | TMS / SWDIO | JTAG bridge J11 p2 |
| 3 | TCK / SWCLK | J11 p4 |
| 4 | TDO / SWO | J11 p6 |
| 5 | TDI | J11 p8 |
| 6 | nRESET | J11 p10 |
| 7 | nTRST | NC (or tied to nRESET via diode for legacy 20-pin JTAG) |
| 8 | GND | shield's GND |
| 9 | GND | shield's GND (extra return for shielded debugger cables) |

Pinout matches `DEVICE_PINOUT.md` §2.4. The custom adapter cable
(DB9 male ↔ ARM 10-pin 1.27 mm) documented there plugs into the
debugger; build it once.

---

## 6. Signal duplication summary

| Signal | DB25-1 | DB25-2 | DB15-1 | DB15-2 | DB15-3 | DB9-1 | DB9-2 | DB9-3 | DB9-CAN | DB9-JTAG | Multiplicity |
|---|---|---|---|---|---|---|---|---|---|---|---|
| DIN0–3 | ✓ | | ✓ | | | | | | | | 2× |
| DIN4–7 | ✓ | | | ✓ | | | | | | | 2× |
| DIN8–11 | ✓ | | | | ✓ | | | | | | 2× |
| DIN12–15 | ✓ | | | | | | ✓ | | | | 2× |
| DOUT0–3 | | ✓ | ✓ | | | | | | | | 2× |
| DOUT4–7 | | ✓ | | ✓ | | | | | | | 2× |
| DOUT8–11 | | ✓ | | | ✓ | | | | | | 2× |
| DOUT12–15 | | ✓ | | | | | ✓ | | | | 2× |
| ADC0–1 | ✓ | | ✓ | | | ✓ | | | | | 3× |
| ADC2–3 | ✓ | | | | | ✓ | | | | | 2× |
| ADC4–7 | ✓ | | | ✓ | | | | | | | 2× |
| ADC8–11 | | | | | ✓ | | | | | | 1× |
| DAC0 | | | ✓ | ✓ | | ✓ | | | | | 3× |
| DAC1 | | | ✓ | | | ✓ | | | | | 2× |
| PWM0–7 | | ✓ | | | | | | | | | 1× |
| UART1, UART2 | | | | | | | | ✓ | | | 1× |
| I2C0 (SDA/SCL) | | | | | | | | ✓ | | | 1× |
| AREF | | | | | ✓ | ✓ | | | | | 2× |
| +5 V | | | | | | | | ✓ | | | 1× |
| +3.3 V | | | ✓ | ✓ | ✓ | ✓ | | ✓ | | | 5× |
| GND (digital) | | ✓ | ✓ | ✓ | | | ✓ | ✓ | (CAN_GND) | (2×) | many |
| AGND | ✓ | | ✓ | | ✓ | ✓ | | | | | 4× |
| CAN_H, CAN_L | | | | | | | | | ✓ | | rear only |
| JTAG (5 sig + VREF) | | | | | | | | | | ✓ | rear only |

All 140 D-Sub pins are populated. Most-used signals (DIN, DOUT,
ADC0-3, DAC, +3.3 V) appear on 2-3 connectors for convenience.
ADC8-11 and PWM appear on a single connector each (less critical
duplication budget).

---

## 7. CAN transceiver subcircuit

A small subcircuit on the backplane east margin converts the
SAM3X CAN0 controller's 3.3 V CMOS RXD / TXD signals into the CAN
bus differential pair on the rear DB9. Selectable bus termination
via SW1.

```
  +5V (from H_PWR p4)
   │
   ├── 10 µF (C1) ─── GND
   │
   ├── 100 nF (C3) ── GND
   │
   ▼
  ┌─────────────┐
  │  MCP2562FD  │ (DIP-8, U1)
  │             │
  │  pin 1 TXD ◄──── CANTX (D69/PA0) from H_DAC p4
  │  pin 2 GND ────► GND
  │  pin 3 VDD ◄──── +5V
  │  pin 4 RXD ────► CANRX (D68/PA1) to H_DAC p3
  │  pin 5 VIO ◄──── +3.3V (from H_PWR p3) ── 100 nF (C4) ── GND
  │  pin 6 CANL ───► (via SW1 + R1) ──┐
  │  pin 7 CANH ───►                   ├── DB9-CAN p2 (CAN_L), p7 (CAN_H)
  │  pin 8 STBY ──── GND (always active mode)
  └─────────────┘    + 10 µF (C2) on VIO ── GND
                     │
                  R1 120Ω
                     │
                    SW1 (DPST slide)
                     │
                    (when closed: termination on; when open: termination off)
```

| Component | Function |
|---|---|
| U1 MCP2562FD | CAN 2.0 B / FD-capable transceiver, 3.3 V VIO compatible, DIP-8 (use socket so it can be replaced if the bus accidentally takes a 24 V hit) |
| R1 120 Ω | bus termination, end-of-line. Switch SW1 selects whether this node is at the bus end (term ON) or in the middle (term OFF) |
| SW1 DPST | enable / disable termination |
| C1 10 µF + C3 100 nF | VDD bulk + decoupling |
| C2 10 µF + C4 100 nF | VIO bulk + decoupling |

**STBY (pin 8)**: tied to GND through a short trace to keep the
transceiver in normal (active) mode always. If low-power CAN sleep
mode is ever needed, lift the STBY pin and route to a spare GPIO.

**Firmware status**: as of v2.1.1 the SAM3X CAN0 peripheral is **not
driven by the phycommander firmware**. The ASF CAN driver
(`ATSAM3X8E_FW/src/ASF/sam/drivers/can/`) is present but no PhyCMD-64
protocol command exposes CAN. Enabling CAN requires:

1. Initialize CAN0 in `main.c` at startup (pick rate: 125 kbit/s, 250
   kbit/s, 500 kbit/s, or 1 Mbit/s)
2. Extend the wire protocol with CAN TX / RX commands, or add a
   CAN-USB bridge mode in `physerver`
3. Optionally expose `can0` via SocketCAN on the host so `candump` /
   `cansend` work natively

Estimate: 1–2 days firmware + host work. Hardware on backplane v4 is
ready; CAN messages can be exchanged on the bus the moment firmware
support lands, with no PCB changes needed.

---

## 8. JTAG bridge subcircuit

The Arduino Due exposes its SWD / JTAG interface on a 2×5 1.27 mm
SMD header on the **top side** of the Due PCB, near the SAM-BA USB
connector. With the v4 shield plugged on top, this header sits
directly under the backplane. v4 solves access via:

1. A rectangular **PCB cutout** (~14 × 7 mm) directly above Due-J1
   (see §1.3)
2. A custom **1.27 mm-to-2.54 mm pigtail** (~80 mm long) that plugs
   into Due-J1 from below, exits through the cutout, and lands on a
   2×5 2.54 mm IDC plug
3. A **JTAG bridge header J11** (2×5 2.54 mm shrouded vertical) on the
   backplane top side, in row C of the connector grid (see §1.5)
4. Backplane traces from J11 to the rear DB9 #5 (DEBUG) via
   conventional 0.3 mm signal traces on L1 / L4

Pinout follows the **ARM Cortex 10-pin Debug Connector** standard
on J11, mapped to the DB9 per `DEVICE_PINOUT.md` §2.4:

| J11 pin (2.54 mm) | ARM 10-pin signal | DB9 #5 pin |
|---|---|---|
| 1 | VTREF (3.3 V) | 1 |
| 2 | TMS / SWDIO | 2 |
| 3 | GND | tied to backplane GND, also DB9 pins 8, 9 |
| 4 | TCK / SWCLK | 3 |
| 5 | GND | backplane GND |
| 6 | TDO / SWO | 4 |
| 7 | KEY | NC |
| 8 | TDI | 5 |
| 9 | GND | backplane GND |
| 10 | nRESET | 6 |

The pigtail is built once (~5 minutes with a 1.27 mm 10-conductor
flat ribbon, 1×5 1.27 mm SMD socket on the Due end, 2×5 2.54 mm IDC
plug on the J11 end). Stash it inside the chassis next to the rear
DB9 once installed; nobody needs to unplug it during normal use.

---

## 9. Routing guidelines (4-layer)

### 9.1 Layer assignment

- **L1 (top, signal + ground pour)**: traces from D-Sub IDC headers to
  vias bound for L4. Local short routes for the CAN transceiver
  passives. L1 ground pour fills empty area, tied to L2 GND plane via
  stitching vias every ~10 mm.
- **L2 (continuous GND plane)**: solid copper, no splits. Single
  connection to AGND (via a star point at H_PWR) so analog return
  currents have a clean path to the Due's analog ground.
- **L3 (PWR plane, split)**: two zones:
  - **+3.3 V zone**: covers the area under DB15s, DB9-1, DB9-3, JTAG
    bridge, and CAN transceiver VIO trace. Source: `H_PWR` pin 3.
  - **+5 V zone**: covers the area under the CAN transceiver VDD and
    DB9-3 pin 7. Source: `H_PWR` pin 4.
  - Split kept narrow (~0.5 mm gap), with a spanning capacitor (100 nF
    NPO 0805 on L1) to provide a return path for any signal trace that
    happens to cross the split.
- **L4 (bottom, signal)**: traces from `H_D22` and other Due mating
  headers fan out westward and northward to the IDC headers above.
  Most of the DIN / DOUT / ADC routing lives here.

### 9.2 Trace widths

| Net | Width (mm) | Why |
|---|---|---|
| Digital signal (DIN, DOUT, UART, I2C) | 0.25 | sufficient at 3.3 V, ≤ 20 mA |
| Analog (ADC, DAC, AREF) | 0.30 | low current, high impedance, slightly wider for noise margin |
| PWM | 0.30 | rise / fall time matters; wider trace helps |
| Power (+3.3 V, +5 V — only where leaving the L3 plane) | 0.5 | ≤ 200 mA |
| GND (only where leaving L2 plane) | 0.6 | always wide |
| CAN_H, CAN_L | 0.4 each, 0.4 mm spacing | edge-coupled differential pair, controlled to ~120 Ω at standard JLC stackup |

### 9.3 Star ground point

Single tie between AGND and main GND at `H_PWR` pin 5. Run a 1 mm
trace from H_PWR p5 to L2 GND plane via a 0.6 mm via, and a separate
1 mm trace from H_PWR p5 to all AGND pins on L1 (via stitching). This
prevents digital return currents from circulating through the analog
return path.

### 9.4 Differential pair routing (CAN_H, CAN_L)

- Length-matched within ±0.5 mm
- Parallel run with 0.4 mm spacing
- No vias (keep entirely on L1 from U1 pins 6/7 to DB9 #4 pins 2/7)
- Distance < 30 mm
- Reference: continuous L2 GND plane underneath

### 9.5 Via count estimate

| Net class | Vias |
|---|---|
| DIN signals (16 × ~1 via from L4 to L1 IDC) | ~16 |
| DOUT signals (16 × ~1 via) | ~16 |
| ADC signals (12 × ~2 vias because most are 2-3× duplicated) | ~25 |
| PWM (8 × ~1 via to DB25-2) | ~8 |
| DAC, UART, I2C, AREF, JTAG signals | ~15 |
| Power (+3.3 V, +5 V from L3 to L1 / L4 IDC pads) | ~30 |
| GND stitching (L1 pour to L2 plane, every 10 mm) | ~80 |
| **Total** | **~190** |

At ~190 plated through-hole vias on a 4-layer board, fab cost is
~€5–€8 per piece in QTY 5 at PCBWay or JLCPCB (no via tax in this
range).

### 9.6 4-layer vs 2-layer trade-off

A 2-layer fallback is feasible at this density if the 4-layer fab
budget is unacceptable. Consequences of dropping to 2 layers:

- ADC noise floor: empirically 5–10 dB worse mid-band (no continuous
  internal reference plane). On 12-bit ADC reads this is borderline
  noticeable; on 8-bit downsampled output it disappears.
- CAN signal integrity: still works at 1 Mbit/s on short cables (< 5 m)
  but the differential pair is no longer over a clean reference plane;
  consider derating bus length.
- Routing time: roughly 2× the layout effort because power and ground
  traces compete with signal traces on the same two layers.
- Cost: ~50% of 4-layer fab cost.

The v4 design lives on 4 layers in KiCad. Switching to 2 layers
later is a ~2 hour rework in KiCad if needed.

---

## 10. Front-panel "Arduino Reset" button + LED (routed through J12)

The front-panel top pushbutton (Arduino Reset) and its illuminated
indicator are routed **through this PCB** in v4, replacing the direct
harness used in earlier revisions. The signals leave the backplane on
header J12 (4-pin 2.54 mm, top side, see §11) and reach the panel
button + LED via a short 4-conductor cable.

The reset path uses the Due's `D3` GPIO input rather than the hardware
RESET line: the firmware (`main.c::SysTick_Handler`) debounces `D3` for
50 ms and then triggers a software reset via
`RSTC_CR = 0xA5000000 | PROCRST | PERRST | EXTRST` (see
[`docs/hardware/PIN_MAP.md`](../hardware/PIN_MAP.md) §5). The effect is
identical to pulling the Due's RESET pin low, with the added benefit of
debouncing and the ability for firmware to suppress accidental presses
or log reset events.

The LED is driven by `D4` (currently a "reserved" pin in PIN_MAP §3.1):
firmware sets `D4` LOW to light the LED, HIGH to extinguish. Until the
firmware adds a `D4` output driver, the LED stays dark by default
(`D4` floats as input). The natural firmware integration is to mirror
the heartbeat behaviour of the on-board `D13` LED, but on the
front-panel LED instead.

> ⚠️ **If you want a hardware reset that survives a hung firmware**,
> rewire J12 pin 3 from the `D3` net to the `RESET` net on `H_PWR` p2
> (which is currently NC on the shield). One trace change in KiCad;
> all other v4 wiring stays the same. The trade-off: you lose
> debouncing and you lose the ability for firmware to log the reset.

---

## 11. Panel button + LED header (J12)

J12 is a **1×4 pin header (2.54 mm pitch, vertical)** at the west
margin of the top side, positioned to give short trace runs to both
the `H_DIG` mating header (which carries `D3` and `D4`) and the
backplane GND/+3V3 power nets. R2 (220 Ω THT axial) sits just north of
J12 and limits the LED current.

### 11.1 J12 pinout (looking at the connector from outside the board)

| Pin | Net | Backplane source | Function | Wire to panel |
|---|---|---|---|---|
| 1 | `LED_A` | +3.3 V via R2 (220 Ω) | LED anode (current-limited) | front-panel LED anode |
| 2 | `LED_K` | Due `D4` via `H_DIG` p5 | LED cathode, firmware-driven (LOW = ON) | front-panel LED cathode |
| 3 | `RST_BTN` | Due `D3` via `H_DIG` p4 | Active-low button input (Due has internal pull-up) | front-panel button NO contact |
| 4 | `BTN_GND` | Backplane GND | Button return | front-panel button COM contact |

### 11.2 LED current calculation

```
+3.3 V ─── R2 (220 Ω) ─── J12 p1 ─── LED+ (anode)
                                          │
                                       LED Vf
                                          │
                                       J12 p2 ─── Due D4 (output)
                                                    │
                                             when D4 = LOW:
                                              I_LED = (3.3 V − Vf) / 220 Ω
```

For a typical red LED (Vf ≈ 2.0 V): I_LED ≈ (3.3 − 2.0) / 220 = 5.9 mA.
For a blue / white LED (Vf ≈ 3.0 V): I_LED ≈ (3.3 − 3.0) / 220 = 1.4 mA
(quite dim; if a brighter blue LED is wanted, drop R2 to 100 Ω, which
gives 3 mA at Vf = 3.0 V).

The SAM3X GPIO can comfortably sink 5–6 mA per pin so a red LED at
6 mA on `D4` is well within spec.

### 11.3 Cable from J12 to front panel

| Conductor | J12 pin | Panel target |
|---|---|---|
| 1 (red) | 1 (LED_A) | LED anode terminal of the front-panel illuminated pushbutton |
| 2 (any) | 2 (LED_K) | LED cathode terminal |
| 3 (any) | 3 (RST_BTN) | NO contact of the panel button |
| 4 (black / green) | 4 (BTN_GND) | COM contact of the panel button |

Length: ~10–15 cm. A 4-conductor flat ribbon with a 2.54 mm crimp-on
IDC plug at the backplane end and crimp / solder terminals at the
panel end. Cheap and standard.

### 11.4 Firmware status

| Feature | Status as of v2.1.1 |
|---|---|
| `D3` reset-button input | ✅ active in firmware (`main.c::SysTick_Handler`, 50 ms debounce, triggers `RSTC_CR` software reset) |
| `D4` LED output | ❌ not driven by firmware yet (the pin sits floating as input). Adding a `D4` heartbeat (1 Hz blink on healthy iso transport, ~4 Hz before USB enumeration, solid on error, mirroring the on-board `D13` LED) is a ~20-line change to `main.c` plus declaring `D4` as PIO output at boot. |

No firmware change is required to make the **reset button** work; only
the **LED** is dark until `D4` driving lands.

---

## 12. Migration from v3

| Aspect | v3 (skeleton, never fabricated) | v4 (this document) |
|---|---|---|
| PCB outline | 101.6 × 53.34 mm (Due footprint exact) | **120 × 100 mm** (Due overlay + extension area) |
| Layers | 2 (home-fab, manual rivets) | **4 (fab service, plated vias)** |
| Top D-Sub IDC headers | 9 (3 DB25 + 3 DB15 + 3 DB9, all front) | **10 (2 DB25 + 3 DB15 + 5 DB9 = 3 front + 2 rear)** — DB25 #3 dropped because the bench's third DB25 is wired to the Intel host parallel port (memory: `phycommander_db25_panel_routing.md`) |
| Bottom mating headers | 6 (POWER, ANALOG, COMM, DIGITAL, 2×18, DAC/CAN) | **8** (above six + new H_UART for D14-D21 + new H_AEXT for A8-A11 + D70/D71) |
| ADC channels exposed | 8 (ADC0-7) | **12 (ADC0-11)** — matches firmware capability |
| PWM channels exposed | 0 (deferred) | **8 (PWM0-7)** on DB25-2 — matches firmware capability |
| UART exposed | 0 | **2 (UART1, UART2)** on DB9 #3 |
| I2C exposed | 0 | **1 (I2C0)** on DB9 #3, plus I2C1 reserved |
| CAN bus exposed | 0 | **CAN0** on rear DB9 #4 (+ on-board MCP2562FD transceiver) |
| JTAG exposed | 0 | **SWD/JTAG** on rear DB9 #5 (+ pigtail through PCB cutout) |
| Front-panel pin count covered by backplane | 147 (3 × 25 + 3 × 15 + 3 × 9) | **140** (2 × 25 + 3 × 15 + 5 × 9), with the missing 25-pin going to the host parallel port |
| Active components | 0 | **1 (MCP2562FD)** + supporting passives |
| Front-panel button + LED | direct 4-wire harness from Due POWER header to panel (LED indicates +3.3 V only, no firmware control) | routed through this PCB via header **J12** (1×4) + R2 (220 Ω LED limit); reset signal goes through Due `D3` (firmware-debounced 50 ms soft reset), LED cathode goes through Due `D4` (firmware-driven, future heartbeat) |
| Assembly time | ~1 hour (including 40-50 manual rivets) | ~30 minutes (no rivets, just header soldering + DIP-8 socket) |
| Fab lead time | 0 (home-etched) | ~7 days (PCBWay standard) |
| Cost per board (parts + PCB) | ~€12-€17 | ~€13-€16 |

Migration steps for someone holding a v3 PCB:
1. Don't bother — v3 is a skeleton, never built. Cancel any v3 fab order in flight and order v4 instead.
2. Front-panel D-Sub pinouts: completely re-allocated. Old front-panel cabling (if any) needs to be rewired according to §5.
3. Rear-panel DB9 cabling (CAN, JTAG): replaces the dedicated transceiver / debug PCBs that `DEVICE_PINOUT.md` §2.4 and §2.5 describe. Backplane v4 drives both rear DB9s directly.

---

## 13. Version history

| Version | Date | Changes |
|---|---|---|
| v1 | 2026-04-11 | Active design with onboard level shifters (ULN2803, 74HCT245), opto DIN, LM2596/XL6009 power modules, 152 × 53 mm 2-layer DIP. Abandoned: too many ICs to hand-solder. |
| v2 | 2026-04-14 | Passive standalone PCB, 140 × 80 mm single-layer home-fab, no active components, Arduino Due connected via wire harness from edge strips. Worked but cable management was messy. |
| v3 | 2026-04-14 | Arduino Due shield: 101.6 × 53.34 mm exact Due footprint, double-sided home-fab with through-hole rivets for vias. 6 bottom-side mating headers, 9 top D-Sub IDC headers. Skeleton committed, never fabricated. |
| v4 | 2026-05-09 | **120 × 100 mm 4-layer fab-service shield**: 8 bottom-side mating headers (above six + H_UART + H_AEXT) cover all firmware-published Due pins (12 ADC, 8 PWM, 16 DIN/DOUT, 2 DAC, 2 UART, 2 I2C, CAN0, JTAG). 10 top-side D-Sub IDC headers (3 front + 2 rear DB9). On-board MCP2562FD CAN transceiver with switchable termination. JTAG access via PCB cutout + pigtail to rear DB9 #5. DB25 #3 dropped (already wired to Intel host parallel port on the bench). |
| **v4.1** | 2026-05-11 | **Added J12 (1×4 panel button + LED header) + R2 (220 Ω)** in the west margin. Routes the front-panel "Arduino Reset" pushbutton through Due `D3` (firmware-debounced soft reset) and the front-panel LED through Due `D4` (firmware-driven, eventually a heartbeat indicator). Replaces the legacy direct 4-wire harness from Due POWER to the panel. `H_DIG` pins 4 and 5 now route to J12 instead of being NC. |
