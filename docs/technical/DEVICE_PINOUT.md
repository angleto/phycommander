# Device Pinout Reference

Master pinout reference for the phycommander chassis. Covers all physical connectors on the front and rear panels. For the PCB-level detail of the front-panel distribution board, see `PCB_BACKPLANE_PINOUT.md`.

```
            ┌─ FRONT PANEL (user I/O) ──────────────────────────────┐
            │                                                        │
            │   9 x D-Sub in 3×3 grid (confirmed via photo):         │
            │     col1 (left)   = DB25 × 3  (top to bottom)          │
            │     col2 (center) = DB15 × 3                           │
            │     col3 (right)  = DB9  × 3                           │
            │   147 pins total, fully populated                      │
            │                                                        │
            │   + 3 illuminated pushbuttons (right):                 │
            │       Top: Arduino Reset    Mid: PC Reset              │
            │       Bot: Main Power ON/OFF                           │
            │                                                        │
            │   + 4 banana jacks (2 left + 2 right):                 │
            │       +12V/GND each side, parallel from host PSU       │
            │                                                        │
            └────────────────────────────────────────────────────────┘

            ┌─ REAR PANEL (service + industrial) ───────────────────┐
            │                                                        │
            │   CURRENT (from photo, to be redesigned):              │
            │     4 x DB9 (top row), motherboard I/O shield (USB,    │
            │     VGA, HDMI, Ethernet, audio), barrel DC jack,       │
            │     red power rocker switch. No rear fan.              │
            │                                                        │
            │   DESIRED (see §3 — design changes pending):           │
            │     4 x DB9 retained, IEC C14 mains inlet replacing    │
            │     barrel jack, fuse holder, service cover over       │
            │     motherboard I/O, chassis ground stud.              │
            │                                                        │
            │   4 x DB9 assignments:                                 │
            │     · COM1  (RS-232, motherboard UART1)                │
            │     · COM2  (RS-232, motherboard UART2)                │
            │     · DEBUG (SWD/JTAG, Arduino Due SAM3X)              │
            │     · CAN   (CAN bus, Arduino Due CAN0)                │
            │                                                        │
            └────────────────────────────────────────────────────────┘
```

---

## 1. Front panel

Nine D-Sub connectors distributed by function and voltage level. All pins are driven or sensed by the Arduino Due (which the host Intel/Linux board controls via USB CDC).

Full pin-by-pin tables are in [`PCB_BACKPLANE_PINOUT.md` §5](PCB_BACKPLANE_PINOUT.md#5-d-sub-connector-pinouts). Summary here:

### 1.1 Front connector map

All signals are native **3.3 V CMOS** — backplane PCB v2 is pure passive routing. External breakout boards (user-built, application-specific) handle level shifting, opto-isolation, or analog conditioning if needed.

| Label | Connector | Personality | Signals (per backplane v4) |
|---|---|---|---|
| **DB25-1** | DB25 female | SENSING | 16 DIN + 8 ADC (0–7) + AGND |
| **DB25-2** | DB25 female | CONTROL | 16 DOUT + 8 PWM + GND |
| **DB25-3** | DB25 female | **NOT Arduino-driven** | Wired to host motherboard's onboard parallel port (LPT). Backplane v4 deliberately does not route this connector — the bottom-left DB25 stays as a parallel-port breakout for the Linux host. Treat as `/dev/parport0` from userspace, not as phycommander I/O. |
| **DB15-1** | DB15 female | Bank Low (ch 0–3) | 4 DIN + 4 DOUT + ADC0/1 + DAC0/1 + 3.3V + AGND + GND |
| **DB15-2** | DB15 female | Bank Mid (ch 4–7) | 4 DIN + 4 DOUT + ADC4/5/6/7 + DAC0 + 3.3V + GND |
| **DB15-3** | DB15 female | Bank High (ch 8–11) | 4 DIN + 4 DOUT + ADC8/9/10/11 + AREF + 3.3V + AGND |
| **DB9-1** | DB9 female | Quick Analog | ADC0–3 + DAC0/1 + AREF + 3.3V + AGND |
| **DB9-2** | DB9 female | Quick Digital High (ch 12–15) | 4 DIN + 4 DOUT + GND |
| **DB9-3** | DB9 female | Quick COMM | UART1 + UART2 + I2C0 + 5V + 3.3V + GND |

**Physical arrangement** (confirmed from photo 2026-04-13): **3×3 grid, columns by connector size**. Left column = DB25 × 3 (top to bottom: DB25-1, DB25-2, DB25-3). Center column = DB15 × 3. Right column = DB9 × 3. This arrangement keeps similar-width connectors grouped which looks clean and makes cable labeling easier (each column has one type).

**Gender convention on front panel**: all **female** D-Sub (panel-mount, blue plastic insert visible in photo), so user-side cables carry male connectors.

### 1.2 Front panel accessories

Beyond the 9 D-Subs the front panel carries additional elements that are **not routed through the I/O backplane PCB** — they wire directly to the host motherboard's front-panel header, the PSU, or the Arduino Due.

#### 1.2.1 Three illuminated pushbuttons (right side)

Three momentary pushbuttons with integrated LEDs, stacked vertically on the right side of the front panel.

| Position | Function | Button wiring (backplane v4) | LED indicates | LED driver |
|---|---|---|---|---|
| **Top** | Arduino Due Reset | Button → backplane **J12 p3** (`RST_BTN`, routed to Due `D3`) ↔ **J12 p4** (`BTN_GND`). Active-low, Due has internal pull-up. Firmware debounces 50 ms and triggers a software reset via `RSTC_CR` | Firmware health (once `D4` driver lands); currently just confirms `D4` initialised | LED anode → backplane **J12 p1** (`LED_A`, current-limited by R2 = 220 Ω on backplane) ↔ LED cathode → **J12 p2** (`LED_K`, to Due `D4`, firmware sinks LOW = LED on) |
| **Mid** | PC Reset | Button → motherboard front-panel RESET# header (FPRST) | Host booted / PWR_OK | Motherboard PWR_OK output or +3.3V_SB rail via series resistor |
| **Bot** | Main Power ON/OFF | Button → motherboard front-panel PWRBTN# header (ATX soft-power, momentary pulse) | PSU +12 V live | +12 V rail via ~1.2 kΩ series resistor |

**Notes**:
- All three buttons are momentary (not latching). Power-on is ATX soft-start logic (pulse the PWRBTN# low briefly), the motherboard's firmware does the actual state transition.
- **The Arduino Reset top button + LED are routed through the backplane PCB (J12)** since v4. The legacy direct harness from Due POWER to panel (used in v3 and earlier) is no longer assembled. 4-conductor cable from backplane J12 to the front panel carries: LED_A, LED_K, RST_BTN, BTN_GND. See `PCB_BACKPLANE_PINOUT.md` §11.
- Reset behaviour: D3-driven soft reset (firmware-debounced 50 ms, then `RSTC_CR = 0xA500000D`). If a hung-firmware "panic" reset is needed, rewire J12 p3 to the `RESET` net on `H_PWR` p2 instead (one trace change in KiCad).
- The Arduino Reset LED is firmware-driven via `D4` (currently a reserved pin in PIN_MAP §3.1). Until firmware adds a `D4` output driver, the LED stays dark. Natural integration: mirror the on-board `D13` heartbeat on `D4` so the front-panel LED blinks at the same 1 Hz "healthy iso transport" rhythm.
- Consider a status summary silk-screen below each button, e.g. "DUE", "PC", "POWER".

#### 1.2.2 Four banana jacks (12 V power distribution)

Four banana jacks (4 mm standard, insulated shaft) for distributing the host PSU's +12 V rail to external accessories — useful for powering lamps, relay coils, small motors, scope probes, or anything else the user wants in a lab setting. Two pairs (one per side) for symmetric access.

**Column layout** (stacked vertically, 2 jacks each side):
| Column | Position | Signal | Function |
|---|---|---|---|
| **Left** (2 jacks) | Top | +12 V | PSU +12 V rail |
| | Bottom | GND | Return for L-top |
| **Right** (2 jacks) | Top | +12 V | Second +12 V tap (parallel with L-top) |
| | Bottom | GND | Return for R-top |

**Wiring notes**:
- All +12 V jacks are fed from the same PSU rail via a common bus wire (AWG 18, inside the chassis). No isolation between left and right.
- User mentioned the PSU output is "probably 12 V but needs measuring". **Action item**: confirm with multimeter before labeling the jacks. If the rail is actually something else (5 V? split ±12 V?), relabel accordingly.
- Add a 3 A fast-blow in-line fuse between the PSU and the jack bus to protect against shorts on the external probes.
- Insulated 4 mm banana jack convention: **red body = positive**, **black body = ground**. Match the jack insulator colors to signal polarity for user safety.

---

## 2. Rear panel

Four DB9 connectors for service, development and industrial connectivity, plus power entry and motherboard I/O access. **The rear panel is scheduled for redesign** — see [§3 Rear panel redesign (desiderata)](#3-rear-panel-redesign-desiderata) for the proposed new layout. This section documents the **current** state (what is physically built today, per photo taken 2026-04-13) and the **final** DB9 pinout assignments (which are stable and will survive the redesign unchanged).

### 2.1 Current state (as-built)

Observation from photo:

```
                  REAR PANEL (current — as-built)
   ┌────────────────────────────────────────────────────────────┐
   │                                                            │
   │   [COM1]     [COM2]     [DEBUG]    [CAN]                   │
   │    DB9m      DB9m        DB9f       DB9m                   │
   │                                                            │
   │   ┌──── motherboard I/O shield (mITX/mATX) ─────┐ [RED SW] │
   │   │ [audio] [HDMI] [USB3.0] [USB2.0] [VGA] [E]  │          │
   │   │ [barrel DC jack]                             │          │
   │   └──────────────────────────────────────────────┘          │
   │                                                            │
   └────────────────────────────────────────────────────────────┘
```

**Issues with the current layout**:
1. **Power inlet**: barrel DC jack is flimsy, easy to unplug accidentally, limited current carrying capability. Not industry-standard for benchtop equipment.
2. **No fuse**: if the PSU fails short, only the external adapter's protection kicks in. Local fuse protection is standard practice for lab gear.
3. **Motherboard I/O exposed**: USB/VGA/HDMI/audio all visible to the end user. Encourages using the phycommander as a general-purpose PC, which is not its role. Also creates confusion about which ports are for phycommander and which are for the underlying Linux host.
4. **No chassis ground stud**: proper earth bonding via a dedicated M4/M5 stud is missing. Relying on the mains PE conductor's path through the PSU is not best practice.
5. **No rear ventilation**: the chassis relies on passive / top-cover ventilation. Under full load (all ADCs + DACs + DOUTs + boost converter) some active airflow would be prudent.

### 2.2 Rear connector map (final — pinout stable across redesign)

| Label | Connector | Origin | Purpose | Pinout convention |
|---|---|---|---|---|
| **COM1** | DB9 male | Motherboard UART1 header | RS-232 serial | IBM PC DTE (standard) |
| **COM2** | DB9 male | Motherboard UART2 header | RS-232 serial | IBM PC DTE (standard) |
| **DEBUG** | DB9 female | Arduino Due JTAG header (10-pin 1.27 mm) via adapter cable | SWD/JTAG debug of ATSAM3X8E | ARM Cortex-M compatible (custom) |
| **CAN** | DB9 male | Arduino Due CAN0 (PA0/PA1) via MCP2562FD transceiver PCB | CAN 2.0 B bus, up to 1 Mbit/s | CiA 303-1 (industrial standard) |

**Gender convention on rear panel**:
- COM1, COM2: **male** (standard PC convention — DCE cables, e.g. modem cables, have female ends)
- DEBUG: **female** (unusual, chosen so a standard ARM debugger adapter cable with male end can plug in)
- CAN: **male** (CiA 303-1 convention — all CAN nodes expose male DB9)

### 2.3 COM1 / COM2 — RS-232 serial (motherboard UART)

Standard IBM PC DB9 RS-232 DTE pinout. Use a straight-through DB9 cable for null modem, or a crossover for DTE-to-DTE.

| Pin | Signal | Direction (DTE) | Wire color (typical) |
|---|---|---|---|
| 1 | DCD — Data Carrier Detect | IN | brown |
| 2 | RXD — Receive Data | IN | red |
| 3 | TXD — Transmit Data | OUT | orange |
| 4 | DTR — Data Terminal Ready | OUT | yellow |
| 5 | GND — Signal Ground | — | black/green |
| 6 | DSR — Data Set Ready | IN | blue |
| 7 | RTS — Request To Send | OUT | violet |
| 8 | CTS — Clear To Send | IN | grey |
| 9 | RI — Ring Indicator | IN | white |

**Wiring**: motherboard COM headers are 2×5 pin IDC box headers (standard IBM PC layout). Use an off-the-shelf "COM ribbon cable" (10-pin IDC to DB9 male) to route from the motherboard to the rear panel. Pin mapping is standardized:

| Motherboard header pin | DB9 pin |
|---|---|
| 1 | 1 (DCD) |
| 2 | 6 (DSR) |
| 3 | 2 (RXD) |
| 4 | 7 (RTS) |
| 5 | 3 (TXD) |
| 6 | 8 (CTS) |
| 7 | 4 (DTR) |
| 8 | 9 (RI) |
| 9 | 5 (GND) |
| 10 | N/C (key) |

**Linux device names**: `/dev/ttyS0` (COM1), `/dev/ttyS1` (COM2) on standard kernel. Verify with `dmesg | grep ttyS` after boot.

### 2.4 DEBUG — SWD/JTAG (Arduino Due SAM3X)

**Function**: live in-circuit debug and flash programming of the Arduino Due firmware via an external SWD/JTAG debugger (Segger J-Link, ST-Link V3, CMSIS-DAP, Black Magic Probe, etc.).

**Rationale**: the only other programming path is USB CDC via the SAM-BA bootloader, activated by the 1200-baud trick. That path has two problems: (1) it erases the entire flash before every write (see `memory/arduino_due_flash_flow.md`), and (2) it cannot breakpoint or step running code. A rear-panel SWD port bypasses both — you can flash, breakpoint, and read memory without opening the chassis.

**Pinout** (ARM Cortex-M compatible, fits both SWD-2-wire and JTAG-5-wire debuggers):

| Pin | Signal | SAM3X pin | Notes |
|---|---|---|---|
| 1 | VREF (3.3 V) | Due 3V3 rail | Debugger senses target voltage here — required for auto-level shifting |
| 2 | TMS / SWDIO | PB6 | Test-mode / SWD data I/O |
| 3 | TCK / SWCLK | PB7 | Test / SWD clock |
| 4 | TDO / SWO | PB5 | Test data out / SWV trace output |
| 5 | TDI | PB4 | Test data in (JTAG only, NC for SWD) |
| 6 | nRESET | SAM3X NRSTB (pin 68) | Active-low reset, open-drain |
| 7 | nTRST | (SAM3X JTAGSEL is bond option; tie to NRSTB via diode or leave NC) | JTAG test reset, optional |
| 8 | GND | Due GND | Signal return |
| 9 | GND | Due GND | Extra ground for shield / twisted-pair return |

The pinout preserves the canonical ARM 10-pin (2×5 1.27 mm) signal set minus one VREF duplicate — no information is lost, just re-routed to 9 pins instead of 10.

**Adapter cable needed** (DB9 male ↔ 2×5 1.27 mm ARM Cortex header):

```
   DB9 side (plug into rear panel DEBUG)       ARM Cortex 10-pin side (plug into debugger)
   ┌───────────┐                                   ┌────────────┐
   │ 1  VREF   ├──────────────────────────────────┤ 1  VREF    │
   │ 2  TMS    ├──────────────────────────────────┤ 2  SWDIO   │
   │ 3  TCK    ├──────────────────────────────────┤ 4  SWCLK   │
   │ 4  TDO    ├──────────────────────────────────┤ 6  SWO     │
   │ 5  TDI    ├──────────────────────────────────┤ 8  TDI     │
   │ 6  nRESET ├──────────────────────────────────┤ 10 nRESET  │
   │ 7  nTRST  ├──────────────────────────────────┤ (NC on 10-pin, connect only for legacy 20-pin)
   │ 8  GND    ├──────────────────────────────────┤ 3,5,7,9 GND│
   │ 9  GND    │                                   │            │
   └───────────┘                                   └────────────┘
```

Build this cable once (5 min with flat ribbon + IDC crimping) and leave it next to the debugger.

**Wiring inside chassis (backplane v4)**: the JTAG signals are routed through the backplane PCB. Build a custom **JTAG pigtail** (~80 mm, 1.27 mm IDC plug ↔ 2.54 mm IDC plug) that goes:

1. Due-J1 (10-pin 1.27 mm SMD header on the Due top side, near USB) ▸ pigtail enters
2. The pigtail exits through a rectangular cutout in the backplane PCB directly above J1
3. The pigtail's 2.54 mm end plugs into the **JTAG bridge header J11** on the backplane top side
4. Backplane traces route from J11 to the **rear DB9 #5 IDC header J10** (10-conductor IDC ribbon to the rear DB9 solder-cup)

See `PCB_BACKPLANE_PINOUT.md` §1.3 (cutout geometry), §5.10 (DB9 #5 pinout), §8 (full bridge schematic).

This routing is short and contained inside the chassis; total Due-J1-to-DB9 trace length is typically <100 mm including pigtail, so SWCLK at 10+ MHz is not at risk.

**Compatible debuggers**:

| Debugger | SWD | JTAG | Notes |
|---|---|---|---|
| Segger J-Link (any model) | ✓ | ✓ | Best-in-class, expensive |
| Segger J-Link EDU | ✓ | ✓ | Cheap ($60), non-commercial license |
| ST-Link V2 / V3 | ✓ | ✗ | Cheap ($5 clones on AliExpress), SWD only — use OpenOCD with `interface/stlink.cfg` and `target/at91sam3XXX.cfg` |
| CMSIS-DAP (DAPLink) | ✓ | ✓ | Open hardware, works with pyocd |
| Black Magic Probe | ✓ | ✓ | Built-in GDB server, no OpenOCD needed |

### 2.5 CAN — CAN 2.0 B bus (Arduino Due CAN0)

**Function**: CAN bus connectivity for instrument integration. The ATSAM3X8E has two CAN controllers; CAN0 (pins PA0=CANRX0, PA1=CANTX0) is available. CAN1 conflicts with DOUT15 (PB14) and DAC0 (PB15) and is not usable without a firmware pin-mux change.

**Pinout** (CiA 303-1, CANopen-compatible industrial standard):

| Pin | Signal | Direction | Notes |
|---|---|---|---|
| 1 | Reserved (optional CAN_V+ secondary) | — | Leave NC on most nodes |
| 2 | CAN_L | I/O | Differential low |
| 3 | CAN_GND | — | Ground reference |
| 4 | Reserved | — | NC |
| 5 | CAN_SHLD | — | Cable shield (optional) |
| 6 | GND (optional) | — | Tie to CAN_GND or leave NC |
| 7 | CAN_H | I/O | Differential high |
| 8 | Reserved (error line) | — | NC |
| 9 | CAN_V+ (optional, 7–36 V bus power) | — | Leave NC unless powering external nodes |

> Strictly use this pinout. Commercial CAN cables (DSUB9) expect pins 2 and 7 to be CAN_L/CAN_H — don't remap them.

**Required hardware inside chassis (backplane v4)**: the CAN transceiver subcircuit (MCP2562FD DIP-8 + 120 Ω termination + DPST term-enable switch + decoupling) lives **on the backplane PCB itself**, in the east margin (x=104..118 mm, y=10..50 mm). The Due's CANRX0 (PA1, D68) and CANTX0 (PA0, D69) are picked up from the `H_DAC` mating header (same header that brings out DAC0/DAC1) with traces shorter than 30 mm to the transceiver. The rear DB9 #4 receives the conditioned differential pair (CAN_H pin 7, CAN_L pin 2) via a standard 10-conductor IDC ribbon from backplane J9.

See `PCB_BACKPLANE_PINOUT.md` §7 for the full transceiver schematic and BOM. There is **no separate daughter-PCB** in v4 — everything fits on the backplane.

**Firmware status**: **not implemented** in the current firmware. The SAM3X CAN peripheral is supported by ASF (the driver is in `ATSAM3X8E_FW/ATSAM3X8E_FW/src/ASF/sam/drivers/can/`) but no phycommander-level code drives it. Enabling this feature requires:

1. Initialize CAN0 at startup (125 kbit/s or 250 kbit/s typical for lab use, 500 kbit/s or 1 Mbit/s for automotive)
2. Extend PhyCMD-64 protocol with CAN TX/RX commands or set up a CAN-to-USB bridge mode
3. On the host side, add `can0` / `vcan0` SocketCAN translation in `physerver` so `candump` / `cansend` tools work

Estimate: 1–2 days of firmware + host work. Hardware is ready once the transceiver PCB is installed.

---

## 3. Rear panel redesign (desiderata)

The current rear panel (documented in §2.1) is a working prototype that mixes consumer PC I/O with instrument-grade ports. This section captures the **desired future layout** so the next mechanical revision has a clear target. The 4 DB9 pinouts (§2.2–§2.6) are stable and will survive the redesign unchanged — only the surrounding hardware changes.

### 3.1 Goals of the redesign

1. **Professional instrument appearance**: hide consumer PC ports behind a service cover, show only ports the end user actually uses during operation.
2. **Standard mains entry**: replace the barrel DC jack with an IEC C14 inlet (universal, 10 A rated, replaceable cable). Benchtop instruments worldwide use this convention.
3. **Safety compliance**: add a mains fuse, proper chassis grounding (M4/M5 stud bonded to the inlet earth pin), and a safety-rated mains switch.
4. **Serviceability without disassembly**: the motherboard I/O shield stays behind the cover but must be reachable via a 2-screw plate for BIOS setup, monitor-attached recovery, USB keyboard for first boot, etc.
5. **Thermal headroom**: add a rear-exhaust 80 mm fan (12 V, temperature-controlled off the motherboard's FAN2 header or a dedicated thermistor). Quiet operation under typical load, ramp up under stress.
6. **Preserve DB9 positions**: COM1/COM2/DEBUG/CAN stay in the same X positions as today so existing cable harnesses don't need rework.

### 3.2 Proposed layout

```
                  REAR PANEL (desiderata — target design)
   ┌──────────────────────────────────────────────────────────────┐
   │                                                              │
   │   [COM1]     [COM2]     [DEBUG]    [CAN]                     │
   │    DB9m      DB9m        DB9f       DB9m                     │
   │                                                              │
   │   ┌──── SERVICE COVER (2 M3 screws, removable) ─────┐        │
   │   │ [audio jacks] [HDMI] [USB×4] [VGA] [Ethernet]    │        │
   │   │  (motherboard I/O — only visible when cover off) │        │
   │   └──────────────────────────────────────────────────┘        │
   │                                                              │
   │   [FAN 80mm]   [GND ⏚]   [FUSE]   [MAIN SW]   [IEC C14]      │
   │                                                              │
   └──────────────────────────────────────────────────────────────┘
```

Element-by-element:

| Zone | Element | Specification | Note |
|---|---|---|---|
| **Top row** | 4 × DB9 (COM1, COM2, DEBUG, CAN) | Unchanged from current | Pinouts in §2.2–§2.6 |
| **Middle** | Service cover | Aluminum plate, 2 × M3 screws, cutout matching motherboard I/O shield | Labelled "SERVICE — remove for host setup only" in silk-screen |
| **Bottom: fan** | 80 mm brushless 12 V fan | ~1500 RPM, <30 dBA, 3-pin tacho, temperature-controlled from MB FAN2 header | Finger guard on outside |
| **Bottom: ground** | Chassis ground stud | M4 brass stud, bonded to IEC earth pin with ≤10 mΩ impedance | Labelled with IEC 60417-5019 earth symbol |
| **Bottom: fuse** | Mains fuse holder | Panel-mount, 5×20 mm, T3.15 A slow-blow (or sized to PSU input rating) | Separate from PSU internal fuse — this is first-line protection |
| **Bottom: switch** | Mains switch | Rocker, DPST, 10 A / 250 V rated, illuminated | Disconnects both L and N for safety |
| **Bottom: inlet** | IEC C14 inlet | Panel-mount, integrated EMI filter optional | Replaces barrel DC jack |

### 3.3 Migration path

What changes inside the chassis:

1. **Replace barrel jack with IEC C14 + internal AC-DC PSU**. The current setup (barrel jack → external DC adapter) becomes (IEC C14 → internal PSU → +12 V / +5 V rails). This means adding a small form-factor PSU inside (pico-PSU, Mean Well IRM-60-12, or similar). Budget ~60 × 30 mm for PSU footprint.
2. **Add fuse + switch wiring**: L (line) from IEC passes through fuse → switch → PSU primary. N (neutral) passes through switch → PSU primary. PE (earth) goes directly to chassis ground stud (no switch).
3. **Machine the service cover**: cut the motherboard I/O shield cutout in a removable aluminum plate (the shield itself stays bolted to the motherboard). Add 2 × M3 threaded holes on the chassis back wall.
4. **Mount fan**: cut 80 mm round hole with 4 × M4 mounting holes. Add internal wire harness from fan to MB FAN2 header (3-pin) or dedicated thermistor controller if you want it PSU-independent.
5. **Re-terminate the DB9 harness**: the existing DB9 wiring stays but cable lengths may need adjustment because the DB9 horizontal positions shift slightly as the layout below them changes.

**Zero firmware impact** — this is a mechanical redesign only. The 4 DB9 pinouts, their electrical wiring, and all software remain unchanged.

### 3.4 Open questions before cutting metal

Before ordering a new rear plate or machining the service cover, confirm:

1. **PSU choice**: stay with external adapter (cheapest, but conflicts with "professional appearance" goal), or go internal (pico-PSU ~€40, Mean Well IRM ~€25, industrial 12 V DIN rail ~€60)? Recommend **internal Mean Well IRM-60-12** for benchtop instruments.
2. **Fan requirement**: is the thermal load actually high enough to need forced cooling, or is the passive top-cover vent sufficient? Measure steady-state CPU + ambient temps under full phycommander workload before committing to a fan cutout (which is irreversible once machined).
3. **Service cover aesthetics**: flush-mount (plate sits in a recessed pocket, requires CNC machining) or surface-mount (plate sits on top of the chassis back wall, simpler)?
4. **Fuse vs circuit breaker**: 5×20 mm fuse is cheaper; panel-mount thermal breaker (e.g. E-T-A 1110) is more user-friendly (resettable). Fuse is probably fine for this use case.
5. **Chassis material**: current chassis is aluminum (per photo); new rear plate should match gauge and finish. Budget a 2 mm 5052 aluminum plate with brushed + anodized finish.

### 3.5 Action items

Pending decisions, the following are "ready to do" now:

- [ ] Measure actual PSU output voltage (user said "probably 12 V" — confirm before labeling banana jacks)
- [ ] Measure steady-state thermals at full load (decides whether fan is needed)
- [ ] Decide on internal vs external PSU (decides whether IEC C14 is appropriate)
- [ ] Source 4 × DB9 panel-mount connectors (solder cup, metal body with flange — ~€3 each) if not already in stock
- [ ] Draft CAD for the new rear plate once decisions above are made
- [ ] Update `companion_board/panel_sketch/rear_panel.svg` to match the final decided layout

Once the user confirms the desiderata, the SVG in `companion_board/panel_sketch/rear_panel.svg` will be rewritten to match this proposed layout.

---

## 4. Connector orientation reference

When looking at the front/rear panel **from outside** (i.e., the user's perspective):

### 4.1 DB9 pin numbering (all rear DB9s and front DB9-1/2/3)

```
   Female (front panel)               Male (rear COM1/COM2/CAN)
   ┌─────────────────────┐            ┌─────────────────────┐
   │  1   2   3   4   5  │            │  5   4   3   2   1  │
   │    6   7   8   9    │            │    9   8   7   6    │
   └─────────────────────┘            └─────────────────────┘
      (viewed from outside)              (viewed from outside)
```

> Pin numbering on female vs male connectors is mirrored — pin 1 is always on the wide side. Double-check with a multimeter before cabling.

### 4.2 DB15 and DB25 pin numbering (front panel only)

DB15 (2-row gameport-style):
```
   ┌──────────────────────────────────┐
   │ 1   2   3   4   5   6   7   8    │
   │   9  10  11  12  13  14  15      │
   └──────────────────────────────────┘
```

DB25:
```
   ┌──────────────────────────────────────────────────────┐
   │ 1   2   3   4   5   6   7   8   9  10  11  12  13    │
   │  14  15  16  17  18  19  20  21  22  23  24  25      │
   └──────────────────────────────────────────────────────┘
```

---

## 5. Cable inventory

Cables needed to fully commission a unit (assuming backplane v4 is installed):

| # | Cable | From | To | Length | Notes |
|---|---|---|---|---|---|
| 1 | COM ribbon (10-pin IDC to DB9M) | Motherboard COM1 header | Rear COM1 DB9M | ~20 cm | Off-the-shelf "PC COM cable" |
| 2 | COM ribbon (10-pin IDC to DB9M) | Motherboard COM2 header | Rear COM2 DB9M | ~20 cm | Same |
| 3 | DB25 ribbon (parallel-port type) | Motherboard LPT header | Front-panel DB25-3 (bottom-left) | ~25 cm | Standard PC parallel-port cable. DB25-3 is wired to the host parallel port, NOT to the backplane (see §1.1) |
| 4 | JTAG pigtail (1.27 mm IDC ▸ flat ribbon ▸ 2.54 mm IDC) | Arduino Due-J1 (JTAG header on Due top side) | Backplane J11 (JTAG bridge header on backplane top side, accessed through PCB cutout above J1) | ~80 mm | Custom (see `PCB_BACKPLANE_PINOUT.md` §8); built once and stays inside the chassis |
| 5 | IDC ribbon (10-conductor) to DB9 solder-cup | Backplane J9 (rear-CAN IDC) | Rear-panel CAN DB9M | ~15 cm | Standard 10-conductor IDC cable + DB9 solder-cup. Carries CAN_H, CAN_L, CAN_GND, shield drain |
| 6 | IDC ribbon (10-conductor) to DB9 solder-cup | Backplane J10 (rear-JTAG IDC) | Rear-panel DEBUG DB9F | ~15 cm | Standard 10-conductor IDC cable + DB9 solder-cup. Carries SWD/JTAG signals + VREF + GND |
| 7 | DB9 male ▸ ARM Cortex 10-pin (1.27 mm) adapter | Rear DEBUG DB9F (outside chassis) | External debugger (J-Link, ST-Link, etc.) | ~10–20 cm | Custom user-side adapter (see §2.4 ASCII diagram). Built once, kept next to the debugger |
| 8 | — (no cable) | Backplane PCB v4 (Arduino Due shield) | Plugs directly onto Due via 8 male pin headers (POWER, ANALOG, COMM, DIGITAL, 2×18, DAC/CAN, UART, AEXT) — no wire harness | — | See `PCB_BACKPLANE_PINOUT.md` §1.4 + §4 |
| 9 | IDC ribbon (26 / 16 / 10-conductor) | Backplane top-side D-Sub IDC headers (8 connectors: J1, J2 for DB25 + J3..J5 for DB15 + J6..J8 for front DB9) | Front-panel DB9 / DB15 / DB25 solder cups | ~10 cm each | 8 cables total, IDC at PCB end, solder cup at panel end |
| 10 | 4-conductor flat cable (2.54 mm IDC plug ▸ crimp / solder ends) | Backplane J12 (top side, 1×4 panel-button/LED header) | Front-panel illuminated reset pushbutton (top button) — LED+/LED−/RST_BTN/BTN_GND | ~10–15 cm | Standard 4-wire cable, 2.54 mm IDC at PCB end. Carries LED supply (current-limited on backplane via R2) and the active-low button signal to Due `D3` |
| 11 | IEC C13 mains cable | Rear IEC C14 inlet (after redesign — see §3) | Wall | — | Standard |

Cables 1, 2, 3, 11 are commodity items. Cables 4, 5, 6, 7, 10 are one-time custom builds. Cable 9 is done as part of chassis assembly (8 ribbon cables, one per Due-driven front-panel D-Sub). Cable 8 is the elimination of the wire harness that v2 needed: backplane v3 / v4 plug directly onto the Due with no intermediate cabling.

---

## 6. Design rationale for rear free ports

This section captures the decision process so future maintainers know why JTAG and CAN were chosen over alternatives. Context: the chassis has 4 rear DB9 connectors; 2 are committed to motherboard serials, 2 were initially free.

### 6.1 Why JTAG/SWD

- Firmware development is a first-class activity on this project (the ATSAM3X8E FW lives in this repo, under `ATSAM3X8E_FW/`). Supporting in-circuit debug is worth a dedicated port
- The only alternative (USB CDC with SAM-BA bootloader) erases the flash on every programming cycle and doesn't support breakpoints
- Zero firmware work required — JTAG/SWD is wired directly to the SAM3X's dedicated debug pins
- Zero recurring cost — adapter cable built once

### 6.2 Why CAN (over RS-485, trigger I/O, or GPIO expansion)

- **CAN vs RS-485**: CAN is the de-facto industrial bus for instrumentation built post-2005; RS-485 (Modbus) dominates older gear. Given phycommander's target domains (modern lab instruments, control prototyping, vehicle/robotics applications), CAN has broader future-use potential. RS-485 can be retrofitted on one of the front DB9s if needed (DB9-3 has UART exposed).

- **CAN vs trigger I/O**: trigger sync is a nicher use case (scope/signal-generator synchronization in physics labs). Few users need it. If a specific application needs trigger I/O, a front-panel DOUT + DIN pair can do the job (3.3 V edge triggering is sufficient for most equipment).

- **CAN vs I2C/SPI expansion bus**: the front DB9-3 (Quick COMM) already exposes 2 UARTs + I2C0. A second expansion port on the rear would be redundant and would encourage internal-only accessories, which defeats the purpose of having a dedicated port. (SPI is not exposed on backplane v4; if needed, add a second cutout above the Due's ICSP header in a future revision.)

- **CAN's weakness**: firmware support is work, not zero-cost. Budget 1–2 days to implement. If this budget is a blocker, leave the rear DB9 wired but the MCP2562FD DIP-8 socket on the backplane empty — populate the chip later when the firmware lands.

### 6.3 Options explicitly rejected

| Option | Why rejected |
|---|---|
| RS-485 as primary | Less future-oriented than CAN; can be retrofitted via front DB9-3 |
| Ethernet / Modbus TCP | Requires MAC+PHY on SAM3X (not integrated) — disproportionate hardware cost |
| GPIO breakout for daughter-MCU | Outside scope of phycommander's one-Arduino architecture |
| Trigger I/O (BNC via DB9) | Niche use, covered by front-panel DIN/DOUT pair |
| Expose USB host | No standard DB9 USB pinout; would look cursed |
| Second JTAG (on second MCU if added) | No second MCU planned |
| Spare / blanking plate | Wastes a mounting hole; better to populate |

### 6.4 If requirements change

The rear DEBUG and CAN ports can be re-purposed if needed:
- Swapping CAN for RS-485 is a 30-minute job: pull the MCP2562FD out of its DIP-8 socket on the backplane, plug a MAX485 (same pinout family) into the same socket, change R1 from 120 Ω to 120 Ω still (RS-485 also uses 120 Ω term), rewire the rear DB9 to put A/B on the differential pin pair. Document the swap on the chassis label.
- DEBUG port can be repurposed as a generic "expansion UART" by rewiring the JTAG pigtail and the backplane J11 ▸ J10 traces, but you'd lose the ability to debug externally — not recommended.

---

## 7. Version history

| Version | Date | Changes |
|---|---|---|
| 1.0 | 2026-04-13 | Initial document. Front panel 9× D-Sub cross-reference; rear panel 4× DB9 with COM1/COM2/DEBUG/CAN assignments. |
| 1.1 | 2026-04-13 | Photo-confirmed physical layout: 3×3 D-Sub grid (DB25 left / DB15 center / DB9 right), 3 illuminated pushbuttons (Arduino Reset / PC Reset / Power), 5 banana jacks for +12V distribution. Rewrote rear panel section: current state vs proposed redesign (desiderata §3). Added §1.2 for front panel accessories. |
| 1.2 | 2026-05-09 | Aligned with backplane v4 (`PCB_BACKPLANE_PINOUT.md` v4): updated §1.1 connector personality table (DB25-2 now CONTROL with 16 DOUT + 8 PWM; DB15-1/2/3 use ADC0-1/4-7/8-11; DB9-3 is COMM not Serial; DB25-3 explicitly NOT Arduino-driven, stays as host LPT breakout). Rewrote §2.4 JTAG wiring (now via PCB cutout + pigtail through backplane J11 → J10 → rear DB9, no direct chassis harness). Rewrote §2.5 CAN hardware (MCP2562FD subcircuit lives on backplane east margin, no separate daughter-PCB). Cable inventory §5 expanded: pigtail, 2 rear-DB9 IDC ribbons, parallel-port DB25-3 cable, 8 (not 9) front-panel ribbons. |
| 1.3 | 2026-05-11 | Front-panel Arduino-Reset top button + LED are now routed through backplane J12 (1×4 header + R2 220 Ω LED limit). §1.2.1 updated: button connects to Due `D3` (firmware-debounced soft reset), LED cathode connects to Due `D4` (firmware-driven, future heartbeat). Direct 4-wire harness from Due POWER is replaced. Cable inventory §5: new cable 10 (J12 ▸ panel button+LED), old cable 10 (IEC mains) renumbered to 11. |
