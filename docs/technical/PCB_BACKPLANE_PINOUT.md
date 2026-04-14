# PCB Backplane — Arduino Due Shield (v3)

Passive Arduino Due shield that fans out all I/O signals to nine D-Sub front-panel connectors. Plugs directly onto the Due — no wire harness. Double-sided home-fab compatible (UV bromograph + photosensitized board + through-hole rivets for vias).

See also:
- `companion_board/panel_sketch/pcb_backplane_top.svg` — top layer print master (1:1 scale)
- `companion_board/panel_sketch/pcb_backplane_bottom.svg` — bottom layer print master (1:1 scale, MIRROR before printing on film)

---

## 1. Mechanical specifications

| Parameter | Value |
|---|---|
| PCB outline | **101.60 × 53.34 mm** (Arduino Due footprint exact, per official datasheet A000062 §8.1) |
| Layers | **2** (top + bottom), home-fab via UV bromograph + photosensitized board |
| Vias | Manual through-hole **rivets** (drill + insert + solder both sides) — no plated holes |
| Board thickness | 1.6 mm FR-4 standard |
| Mounting holes | 4 × M3, Ø 3.2 mm, **at Arduino Due's exact 4 hole positions** (no shift, no calculation) |
| Top side | 9 × D-Sub male pin headers (2.54 mm pitch, 2-row, vertical) facing UP — for ribbon cables to front panel |
| Bottom side | Arduino mating male pin headers (~7 mm standard pin length, **not stackable**) facing DOWN — plug into Due's female headers |
| Total signal pins distributed | 147 (3×9 + 3×15 + 3×25) — all populated |

### 1.1 Mounting hole positions

Arduino Due native pattern. Coordinates derived from two sources cross-referenced:
1. Official Arduino Due datasheet A000062 §8.1 mechanical drawing (PDF dimensions)
2. KiCad official `Arduino_UNO_R3_WithMountingHoles.kicad_mod` footprint (UNO R3 = shield-compatible reference)

| Hole | X (mm) | Y (mm) | Confidence | Source |
|---|---|---|---|---|
| **M1** (top-left) | **15.24** | **2.54** | ✓ verified | Datasheet quoted dimensions |
| **M4** (top-right) | **90.17** | **2.54** | ✓ verified | Datasheet (M1 + 74.93 mm horizontal span, quoted) |
| **M2** (bottom-left) | 15.24 | 50.80 | ★ best estimate | Symmetric to M1; matches UNO R3 KiCad pattern (`y = pin-row + 48.26 mm`) |
| **M3** (middle-right) | 66.04 | 35.56 | ★ best estimate | KiCad UNO R3 pattern (`y = pin-row + 33.02 mm`); x extrapolated for Due length |

Hole drill diameter: **Ø 3.18 mm** per datasheet (Ø 3.20 mm clearance also acceptable for M3).

> **Verification recommended**: M1 and M4 are datasheet-verified. M2 and M3 are best-estimate based on the standard UNO R3 shield-compatible pattern (extrapolated for the Due's longer body). Before final fab, verify in KiCad by importing either:
> - The KiCad official `Arduino_UNO_R3_WithMountingHoles` footprint (gives 4 shield-compatible holes — but UNO is shorter than Due, so x-coord for top-right and middle-right needs scaling)
> - Or a community-maintained Arduino Due footprint (e.g., from SnapEDA, Ultra Librarian, or the original Eagle reference design from Arduino's website)
>
> The KiCad official library does NOT include a dedicated `Arduino_Mega_R3` or `Arduino_Due` footprint — only UNO R3. This is a known gap.

The shield plugs onto the Due via the male-pin headers; the 4 M3 mounting holes provide additional mechanical fixation (M3 standoffs through both shield and Due into the chassis).

### 1.2 Top side — D-Sub layout (3×3 grid)

```
  ┌──────────────────────────────────────────────────────────────┐
  │  ●M1                                                  ●M4    │ y=2.54
  │  [    DB25 #1    ] [  DB15 #1  ] [ DB9#1 ]                   │ row1, y=18..26
  │  [    DB25 #2    ] [  DB15 #2  ] [ DB9#2 ]                   │ row2, y=32..40
  │                                            ●M3                │ y=35.56
  │  [    DB25 #3    ] [  DB15 #3  ] [ DB9#3 ]                   │ row3, y=49..57
  │  ●M2                                                          │ y=50.80
  └──────────────────────────────────────────────────────────────┘
  x=0  15            48 52        74  80    94                 101.52
```

Connector columns positioned to clear the mounting holes. Row 3 ends at y=57 mm, leaving 4.3 mm bottom margin (acceptable for an M3 hole at y=50.80).

### 1.3 Bottom side — Arduino Due mating headers

Male pin headers facing DOWN, mating with Arduino Due's female socket headers. Standard 7 mm pin length (non-stackable shield — nothing plugs on top).

| Header | Type | Position on Due | Used pins |
|---|---|---|---|
| POWER | 1×8 male | south edge, west side | RESET, 3V3, 5V, GND, GND (5 of 8 used) |
| ANALOG | 1×8 male | south edge, east of POWER | A0–A7 (all 8 = ADC 0–7) |
| COMM | 1×10 male | north edge, west side | D8 (future), AREF, SDA, SCL (4 of 10 used) |
| DIGITAL | 1×8 male | north edge, east of COMM | D0=UART_RX, D1=UART_TX (2 of 8 used) |
| 2×18 dual block | 2×18 male | east side of board | D22–D53 = DIN 0–15 + DOUT 0–15 (32 of 36 pin positions) |
| DAC/CAN | 1×6 male | east edge of board | DAC0, DAC1, CANRX, CANTX (4 of 6 used) |

Total bottom-side pins: ~63 pin positions (8+8+10+8+36+6 = 76 holes drilled, ~63 actually carrying signals).

Position coordinates are approximate in `companion_board/panel_sketch/pcb_backplane_bottom.svg`. **Verify against Arduino Due mechanical drawing before final fabrication** — see the official Due reference for exact header positions.

---

## 2. Bill of Materials

| Ref | Part | Qty | Notes |
|---|---|---|---|
| J1–J3 | Male pin header 2×13, 2.54 mm pitch, vertical, ~7 mm pin | 3 | DB25 #1/#2/#3 (top side) |
| J4–J6 | Male pin header 2×8, 2.54 mm pitch, ~7 mm pin | 3 | DB15 #1/#2/#3 (top side) |
| J7–J9 | Male pin header 2×5, 2.54 mm pitch, ~7 mm pin | 3 | DB9 #1/#2/#3 (top side) |
| H-PWR | Male pin header 1×8, 2.54 mm, **standard 7 mm pin** (non-stackable) | 1 | POWER mate (bottom side) |
| H-ANA | Male pin header 1×8, 7 mm pin | 1 | ANALOG mate (bottom side) |
| H-COM | Male pin header 1×10, 7 mm pin | 1 | COMM mate (bottom side) — included for future UART/I2C/SDA/SCL/AREF |
| H-DIG | Male pin header 1×8, 7 mm pin | 1 | DIGITAL D0–D7 mate (bottom side) |
| H-D22 | Male pin header 2×18, 7 mm pin | 1 | 2×18 D22–D53 block mate (bottom side) — main DIN/DOUT routing source |
| H-DAC | Male pin header 1×6, 7 mm pin | 1 | DAC/CAN mate (bottom side) |
| Rivets | 0.6–1.0 mm copper / brass mini rivets | 20–40 | Manual through-hole vias, soldered both sides |

**Total cost**: ~€10–15 per board (connectors only). Home-fab PCB substrate adds ~€2 per board. **No active components** (no ICs, no level shifters, no power converters).

**Cable from PCB to panel**: 9× IDC ribbon cable, 26/16/10 conductor → D-Sub solder-cup, ~10–15 cm each. 9 cables total (one per D-Sub).

**Pin length note**: standard 7 mm pin lengths chosen because **the shield is final** — nothing plugs on top. If you ever want to stack other shields above this one, swap to "stackable" 12 mm pin headers (requires longer mating sockets on the upper shield).

---

## 3. Signal architecture

```
  ┌────────────────────────────────────────┐
  │     phycommander Linux host (Intel)    │
  │     physerver via USB CDC              │
  └─────────────┬──────────────────────────┘
                │ USB-B
                ▼
  ┌────────────────────────────────────────┐
  │  Arduino Due (ATSAM3X8E)               │
  │  Female header sockets ↑↑↑             │
  └─────────────┬──────────────────────────┘
                │ male pins down (~7 mm), shield plugs on top
                ▼
  ┌────────────────────────────────────────┐
  │  Backplane PCB v3 (this document)      │  101.52 × 53.30 mm
  │  - Bottom: Arduino mating headers      │  2 layers + rivet vias
  │  - Top: 9 × D-Sub male pin headers     │  Pure passive routing
  └─────────────┬──────────────────────────┘
                │ 9 × IDC ribbon cables ↑
                ▼
  ┌────────────────────────────────────────┐
  │  Front panel (3×3 D-Sub female grid)   │
  │  user-facing                           │
  └────────────────────────────────────────┘
```

All signals are 3.3 V CMOS native (Arduino Due output level). External application-specific add-ons (level shifters, opto-isolation, signal conditioning) plug into the front-panel D-Subs as needed.

---

## 4. Arduino Due pin → backplane signal mapping

For each Arduino mating header on the bottom side, the table shows which Arduino pin (Due naming) carries which phycommander signal.

### 4.1 POWER header (1×8, bottom-left)

| Header pin | Arduino pin | Signal | Routed to |
|---|---|---|---|
| 1 | IOREF | (3.3 V ref) | not used (NC on shield) |
| 2 | RESET | RESET | NC on shield (Arduino reset is wired direct to front button — see §8) |
| 3 | 3V3 | +3.3 V | shield's +3.3 V net (powers all D-Sub +3.3V pins) |
| 4 | 5V | +5 V | shield's +5 V net (powers DB25#3, DB9#3 +5V pins) |
| 5 | GND | GND | shield's main GND net |
| 6 | GND | GND | shield's main GND net (parallel) |
| 7 | VIN | (~7-12V) | not used |
| 8 | NC | — | not used |

### 4.2 ANALOG header (1×8, bottom-east of POWER)

| Header pin | Arduino pin | Signal | Routed to |
|---|---|---|---|
| 1 | A0 | ADC0 | DB25#1 pin 17, DB15#1 pin 9, DB9#1 pin 1 |
| 2 | A1 | ADC1 | DB25#1 pin 18, DB15#1 pin 10, DB9#1 pin 2 |
| 3 | A2 | ADC2 | DB25#1 pin 19, DB15#2 pin 9, DB9#1 pin 3 |
| 4 | A3 | ADC3 | DB25#1 pin 20, DB15#2 pin 10, DB9#1 pin 4 |
| 5 | A4 | ADC4 | DB25#1 pin 21, DB15#3 pin 1 |
| 6 | A5 | ADC5 | DB25#1 pin 22, DB15#3 pin 2 |
| 7 | A6 | ADC6 | DB25#1 pin 23, DB15#3 pin 3 |
| 8 | A7 | ADC7 | DB25#1 pin 24, DB15#3 pin 4 |

### 4.3 COMM header (1×10, top-west)

| Header pin | Arduino pin | Signal | Routed to |
|---|---|---|---|
| 1 | D8 | (PWM future) | NC for now (firmware doesn't drive PWM yet) |
| 2 | D9 | (PWM future) | NC |
| 3 | D10 | (PWM future / SS) | NC |
| 4 | D11 | (PWM future) | NC |
| 5 | D12 | (digital, future) | NC |
| 6 | D13 | (LED, future) | NC |
| 7 | GND | GND | shield's main GND net |
| 8 | AREF | AREF | DB25#2 pin 24, DB25#3 pin —, DB15#3 pin 13, DB9#1 pin 7 |
| 9 | D20 (SDA1) | I2C_SDA | DB25#3 pin 22, DB9#3 pin 6 |
| 10 | D21 (SCL1) | I2C_SCL | DB25#3 pin 23, DB9#3 pin 7 |

### 4.4 DIGITAL D0–D7 header (1×8, top-east of COMM)

| Header pin | Arduino pin | Signal | Routed to |
|---|---|---|---|
| 1 | D0 (RX0) | UART_RX | DB25#3 pin 18, DB9#3 pin 2 |
| 2 | D1 (TX0) | UART_TX | DB25#3 pin 17, DB9#3 pin 1 |
| 3 | D2 | NC | |
| 4 | D3 | NC | |
| 5 | D4 | NC | |
| 6 | D5 | NC | |
| 7 | D6 | NC | |
| 8 | D7 | NC | |

### 4.5 2×18 dual block (D22–D53, east edge) — main DIN/DOUT source

This block carries all 16 DIN and 16 DOUT signals. Pin numbering convention for the 2×18 block:
- "Front" row (closer to board edge): even D-pins → DIN signals
- "Back" row (closer to board interior): odd D-pins → DOUT signals

| Pair # | Front pin (DIN) | Arduino | Back pin (DOUT) | Arduino |
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

> The pair 17/18 (corresponding to D54/D55 if they existed — they don't on Due) are unused. The shield can still drill those holes but leave them floating.

**Routing strategy**: from this block, the 16 DIN signals need to fan out to DB25#1 + the mixed DB15s + DB9#2; the 16 DOUT signals fan out to DB25#2 + DB25#3 + the mixed DB15s + DB9#2. Many traces; double-sided routing recommended.

### 4.6 DAC/CAN header (1×6, east edge)

| Header pin | Arduino pin | Signal | Routed to |
|---|---|---|---|
| 1 | DAC0 (D66) | DAC0 | DB25#2 pin 17, DB15#1 pin 11, DB15#3 pin 5, DB9#1 pin 5 |
| 2 | DAC1 (D67) | DAC1 | DB25#2 pin 18, DB15#1 pin 12, DB15#3 pin 6, DB9#1 pin 6 |
| 3 | CANRX (D68) | NC | (firmware doesn't use CAN yet) |
| 4 | CANTX (D69) | NC | |
| 5 | (varies) | NC or GND | |
| 6 | (varies) | NC or GND | |

> Header pinout for the DAC/CAN small block varies slightly by Due revision. Verify with the mechanical drawing.

---

## 5. D-Sub connector pinouts

Each D-Sub connector connects to its corresponding 2-row male pin header on the top side via an IDC ribbon cable, then to a panel-mount female D-Sub on the front panel. Pin numbers below are **D-Sub pin numbers** (1–N), not IDC pin numbers.

### 5.1 DB25 #1 — FULL SENSING (pure Strip-A signals: 16 DIN + 8 ADC + AGND)

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
| 25 | AGND | analog ground |

### 5.2 DB25 #2 — FULL CONTROL (16 DOUT + 2 DAC + 3 PWM + power)

| Pin | Signal |
|---|---|
| 1–16 | DOUT 0..15 |
| 17 | DAC0 |
| 18 | DAC1 |
| 19 | PWM0 (future) |
| 20 | PWM1 (future) |
| 21 | PWM2 (future) |
| 22 | +5 V |
| 23 | +3.3 V |
| 24 | AREF |
| 25 | GND |

### 5.3 DB25 #3 — CONTROL-DUP + COMMS

| Pin | Signal |
|---|---|
| 1–16 | DOUT 0..15 (duplicate of DB25 #2) |
| 17 | UART_TX |
| 18 | UART_RX |
| 19 | SPI_MOSI (firmware TBD) |
| 20 | SPI_MISO (firmware TBD) |
| 21 | SPI_SCK (firmware TBD) |
| 22 | I2C_SDA |
| 23 | I2C_SCL |
| 24 | +5 V |
| 25 | GND |

### 5.4 DB15 #1 — Mixed ch 0–3

| Pin | Signal |
|---|---|
| 1–4 | DIN 0..3 |
| 5–8 | DOUT 0..3 |
| 9 | ADC0 |
| 10 | ADC1 |
| 11 | DAC0 |
| 12 | DAC1 |
| 13 | +3.3 V |
| 14 | AGND |
| 15 | GND |

### 5.5 DB15 #2 — Mixed ch 4–7 + PWM

| Pin | Signal |
|---|---|
| 1–4 | DIN 4..7 |
| 5–8 | DOUT 4..7 |
| 9 | ADC2 |
| 10 | ADC3 |
| 11 | PWM0 (future) |
| 12 | PWM1 (future) |
| 13 | PWM2 (future) |
| 14 | +3.3 V |
| 15 | GND |

### 5.6 DB15 #3 — Analog focus + DIN 8–11

| Pin | Signal |
|---|---|
| 1–4 | ADC 4..7 |
| 5 | DAC0 |
| 6 | DAC1 |
| 7 | DIN8 |
| 8 | DIN9 |
| 9 | DIN10 |
| 10 | DIN11 |
| 11 | DOUT8 |
| 12 | DOUT9 |
| 13 | AREF |
| 14 | +3.3 V |
| 15 | AGND |

### 5.7 DB9 #1 — Quick Analog

| Pin | Signal |
|---|---|
| 1–4 | ADC 0..3 |
| 5 | DAC0 |
| 6 | DAC1 |
| 7 | AREF |
| 8 | +3.3 V |
| 9 | AGND |

### 5.8 DB9 #2 — Quick Digital high-channel

| Pin | Signal |
|---|---|
| 1–4 | DIN 12..15 |
| 5–8 | DOUT 12..15 |
| 9 | GND |

### 5.9 DB9 #3 — Quick Serial

| Pin | Signal |
|---|---|
| 1 | UART_TX |
| 2 | UART_RX |
| 3 | SPI_MOSI |
| 4 | SPI_MISO |
| 5 | SPI_SCK |
| 6 | I2C_SDA |
| 7 | I2C_SCL |
| 8 | +5 V |
| 9 | GND |

---

## 6. Signal duplication summary

| Signal | DB25-1 | DB25-2 | DB25-3 | DB15-1 | DB15-2 | DB15-3 | DB9-1 | DB9-2 | DB9-3 | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| DIN 0–3 | ✓ | | | ✓ | | | | | | 2× |
| DIN 4–7 | ✓ | | | | ✓ | | | | | 2× |
| DIN 8–11 | ✓ | | | | | ✓ | | | | 2× |
| DIN 12–15 | ✓ | | | | | | | ✓ | | 2× |
| DOUT 0–3 | | ✓ | ✓ | ✓ | | | | | | 3× |
| DOUT 4–7 | | ✓ | ✓ | | ✓ | | | | | 3× |
| DOUT 8–9 | | ✓ | ✓ | | | ✓ | | | | 3× |
| DOUT 10–11 | | ✓ | ✓ | | | | | | | 2× |
| DOUT 12–15 | | ✓ | ✓ | | | | | ✓ | | 3× |
| ADC 0–1 | ✓ | | | ✓ | | | ✓ | | | 3× |
| ADC 2–3 | ✓ | | | | ✓ | | ✓ | | | 3× |
| ADC 4–7 | ✓ | | | | | ✓ | | | | 2× |
| DAC 0–1 | | ✓ | | ✓ | | ✓ | ✓ | | | 4× |
| PWM 0–2 | | ✓ | | | ✓ | | | | | 2× (future) |
| UART/SPI/I2C | | | ✓ | | | | | | ✓ | 2× |
| +5 V | | ✓ | ✓ | | | | | | ✓ | 3× |
| +3.3 V | | ✓ | | ✓ | ✓ | ✓ | ✓ | | | 5× |
| AREF | | ✓ | | | | ✓ | ✓ | | | 3× |
| GND (any) | ✓ | ✓ | ✓ | ✓ (both) | ✓ | ✓ (both) | ✓ | ✓ | ✓ | all |

All 147 front-panel pins populated. All DIN and DOUT appear on at least 2 ports.

---

## 7. Routing guidelines (double-sided home-fab)

Two layers (top + bottom copper), connected via manual rivets at via locations. Vastly more routing flexibility than a single-layer design.

### 7.1 Layer assignment

- **Top layer (copper side facing user, away from Due)**: signal traces from D-Sub male headers, plus power/ground distribution where needed
- **Bottom layer (copper side facing Due)**: signal traces from Arduino mating headers, plus the other half of the routing
- **Vias (rivets)**: any signal that needs to cross from top to bottom (or vice versa) goes through a small drilled hole with a copper rivet soldered on both sides

### 7.2 Component side assignment

- **Top side (above PCB)**: D-Sub male pin headers, mounted with pins facing up. Pads soldered on bottom layer (so the trace can leave from the bottom layer side).
- **Bottom side (below PCB, facing Due)**: Arduino mating male pin headers, mounted with pins facing down toward the Due. Pads soldered on top layer (so the trace leaves on the top layer side).

This means signals from Arduino mating headers naturally enter on the TOP layer, and signals to D-Sub headers naturally enter on the BOTTOM layer. They pass each other through rivets.

### 7.3 Trace widths

| Net | Width (mm) | Why |
|---|---|---|
| Digital signal (DIN, DOUT, UART, SPI, I2C) | 0.3 | sufficient at 3.3 V, ≤ 20 mA |
| Analog (ADC, DAC, AREF) | 0.3 | low current, high impedance |
| Power (+3.3 V, +5 V) | 0.5 | ~200 mA max |
| GND | 0.6 or wider | always use wide / poured |

### 7.4 Ground strategy

- Top layer: mostly signal traces, no large GND pour needed
- Bottom layer: GND pour in unused areas (acts as quiet reference plane)
- AGND merged with main GND (no separate analog ground plane on this small board — accept slight noise impact)
- For applications needing better analog isolation, build a daughter board with proper AGND/DGND split

### 7.5 Rivet (via) count estimate

| Signal class | Rivets needed |
|---|---|
| Each DIN/DOUT trace from 2×18 to D-Sub (16+16 = 32 signals × ~1 via each) | ~30 |
| ADC traces from ANALOG header | ~5 |
| DAC traces | ~3 |
| Power rails (+3V3, +5V, GND from POWER header) | ~5–10 |
| **Total rivets** | **~40–50** |

Drill bit: 0.6–0.8 mm. Rivet: 1 mm OD copper, ~1.6 mm long, flared on both sides. Soldered at each end. Plan for ~1 minute per rivet during assembly = 45–60 minutes total rivet work. Tedious but routine for home-fab.

### 7.6 Single trace strategy: D22–D53 fan-out

The 2×18 block at the east edge of the bottom side is the source of all 32 DIN/DOUT signals. Best routing strategy:
1. Each D22–D53 signal exits the bottom-side header pad
2. Routes on the **bottom layer** westward across the PCB
3. At the destination D-Sub column, transitions to **top layer** via rivet
4. Top layer trace reaches the D-Sub male pin
5. For multi-destination signals (e.g., DOUT0 on DB25#2 + DB25#3 + DB15#1), use T-branches on the bottom layer before transitioning up

This keeps most signal routing on one layer (bottom), simplifying the top layer to mostly local D-Sub-to-rivet connections.

---

## 8. Arduino reset button (not on this PCB)

Same as v2: the front-panel top pushbutton (Arduino Reset + 3.3V powered LED) is wired **directly** from the Due's POWER header to the button, bypassing this PCB. 4-wire harness:

| Wire | From (Due POWER header) | To (front button top) |
|---|---|---|
| RESET | RESET pin | Button NO contact |
| GND | GND | Button COM contact |
| +3.3 V | +3.3 V (via 220 Ω series) | LED anode |
| GND | GND | LED cathode |

LED lit = Arduino has +3.3 V power. Does not indicate firmware health (could add heartbeat in firmware later — wire LED to spare GPIO instead).

---

## 9. Migration from v2

| Aspect | v2 | v3 |
|---|---|---|
| Form factor | 140 × 80 mm standalone PCB | 101.52 × 53.30 mm Arduino Due shield |
| Layers | 1 (single-layer home-fab) | 2 (double-sided home-fab + rivets) |
| Connection to Due | wire harness (50 wires from Due to backplane Strips) | direct plug (male pins down into Due female sockets) |
| Mounting holes | 4 × M3 at "Due virtual overlay shifted up 20 mm" pattern | 4 × M3 at Arduino Due exact native positions |
| Strips A/B | 25 solder-through holes per strip on right edge | replaced with Arduino Due mating headers on bottom side |
| BOM | 9 box headers + 50 holes | 9 box headers (top) + 6 mating headers (bottom, ~63 pins total) + ~40 rivets |
| Assembly time | longer (50 wires to solder) | shorter (just plug-in + ribbon cables) |

Migration steps for someone holding a v2 PCB:
1. Don't bother — v2 is obsolete. Order/fab the v3 board fresh
2. Front-panel D-Sub pinouts unchanged → existing front panel cables stay
3. Arduino Due wiring becomes plug-in (remove Strip A/B wire harness)

---

## 10. Version history

| Version | Date | Changes |
|---|---|---|
| v1 | 2026-04-11 | Active design with onboard level shifters (ULN2803, 74HCT245), opto DIN, LM2596/XL6009 power modules, 152×53 mm 2-layer DIP. |
| v2 | 2026-04-14 | Passive standalone PCB, 140×80 mm single-layer home-fab, no active components, Arduino Due connected via wire harness from edge strips. |
| v3 | 2026-04-14 | **Arduino Due shield**: 101.52 × 53.30 mm exact Due footprint, double-sided home-fab with through-hole rivets for vias. Bottom side has Arduino mating male pin headers (POWER 1×8, ANALOG 1×8, COMM 1×10, DIGITAL 1×8, 2×18 D22–D53, DAC 1×6) at standard 7 mm pin length (non-stackable). Top side keeps 9 D-Sub male pin headers in 3×3 grid. Mounting at Due native M3 positions. Plugs directly onto Due — no wire harness. |
