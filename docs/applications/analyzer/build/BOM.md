# Bill of Materials — Multi-Function Analyzer

> Complete component list for every tier of the multi-function analytical
> bench instrument, with part numbers, suppliers, quantities, and costs.
> All prices are indicative (April 2026) in EUR, excluding shipping and
> VAT. Source: Mouser, TME, RS Components, Digi-Key, Sigma-Aldrich,
> Megazyme, AliExpress, and specialty suppliers.

---

## How to read this BOM

The instrument is modular (see [`../../MULTIFUNCTION_ANALYZER.md`](../../MULTIFUNCTION_ANALYZER.md) §5).
Each module has its own BOM section. Mix and match depending on your
target configuration. The tier totals at the bottom of the document
summarize the most common configurations.

**Part number conventions:**
- `M:` Mouser
- `TME:` TME (Europe)
- `RS:` RS Components
- `DK:` Digi-Key
- `SIG:` Sigma-Aldrich
- `MEG:` Megazyme
- `AMZ:` Amazon
- `ALI:` AliExpress (when nothing else is reasonably priced)

**Tier color code** used below:
- 🟢 **required** for this module
- 🟡 **recommended** (nice to have)
- 🔵 **optional** (specific use cases only)
- ⚪ **alternative** (substitute for a required item)

---

## 1. Base station

The base station is always present and shared by every configuration. It
hosts phycommander (Arduino Due), the universal photodiode + TIA, the
safety interlock, and the power distribution to modules.

### 1.1 Phycommander core

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Arduino Due (ATSAM3X8E) | `A000062` | M / RS / TME | 1 | 40.00 | 40.00 | 🟢 |
| USB A-to-Micro-B cable, 1.5 m shielded | generic | AMZ | 1 | 3.00 | 3.00 | 🟢 |

### 1.2 Universal detector front-end (TIA)

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Photodiode UV-enhanced, 10×10 mm, 200–1100 nm | Hamamatsu `S1227-1010BR` | M / Hamamatsu | 1 | 30.00 | 30.00 | 🟢 |
| ⚪ *alternative* Photodiode basic, visible-only | Vishay `BPW34` | TME / RS | 1 | 1.00 | 1.00 | ⚪ |
| Transimpedance op-amp | TI `OPA381AIDBVR` (SOT-23-5) | M / DK | 1 | 5.50 | 5.50 | 🟢 |
| ⚪ *alternative* lower-cost low-noise op-amp | TI `LMP7721MA` | M | 1 | 7.00 | 7.00 | ⚪ |
| Feedback resistor, 10 MΩ, 1 %, 0.1 W, thin film | Vishay `MRS25` 10M0F | M / TME | 1 | 0.30 | 0.30 | 🟢 |
| Feedback capacitor, 1 pF, C0G/NP0, 0805 | Murata `GRM2195C1H1R0BA01` | M | 1 | 0.10 | 0.10 | 🟢 |
| Bias divider 100 kΩ ×2, 1 % | any metal film | TME | 2 | 0.05 | 0.10 | 🟢 |
| Bypass cap 10 µF tantalum or ceramic | any | TME | 2 | 0.15 | 0.30 | 🟢 |
| Bypass cap 100 nF X7R | any | TME | 4 | 0.05 | 0.20 | 🟢 |
| IC socket DIP-8 (for op-amp breakout) | any | TME | 1 | 0.20 | 0.20 | 🟡 |

### 1.3 Power distribution

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| DC-DC isolated ±12 V, 1 W (from 5 V USB rail) | Meanwell `SIM1-0512S` (or Traco `TMR 1-0511`) | M / RS | 1 | 8.00 | 8.00 | 🟢 |
| Ferrite bead on 5 V rail | any 600 Ω @ 100 MHz | TME | 1 | 0.10 | 0.10 | 🟡 |
| Power LEDs (3.3 V, 5 V, ±12 V status) | any 5 mm | TME | 4 | 0.10 | 0.40 | 🟡 |
| Barrel jack input (optional 5 V external) | any 5.5×2.1 mm | TME | 1 | 0.50 | 0.50 | 🔵 |

### 1.4 Safety interlock

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Cover microswitch, SPDT, normally-closed | Omron `D2F-01L` | M / TME / RS | 1 | 1.50 | 1.50 | 🟢 |
| Safety relay, SPST, 5 V coil | Omron `G5V-1-DC5` | M / DK | 1 | 2.00 | 2.00 | 🟢 |
| Flyback diode (1N4148) | any | TME | 1 | 0.02 | 0.02 | 🟢 |
| NPN transistor for relay drive (BC547) | any | TME | 1 | 0.05 | 0.05 | 🟢 |
| Base resistor 1 kΩ | any | TME | 1 | 0.02 | 0.02 | 🟢 |
| Pull-up 10 kΩ on DIN5 | any | TME | 1 | 0.02 | 0.02 | 🟢 |

### 1.5 Status LEDs & UX

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| 5 mm green LED (ready) | Kingbright `L-7113GD` | TME | 1 | 0.10 | 0.10 | 🟢 |
| 5 mm amber LED (measuring) | Kingbright `L-7113YD` | TME | 1 | 0.10 | 0.10 | 🟢 |
| 5 mm red LED (fault/laser-on) | Kingbright `L-7113SRD-D` | TME | 1 | 0.10 | 0.10 | 🟢 |
| Current-limit resistors 470 Ω | any | TME | 3 | 0.02 | 0.06 | 🟢 |
| Tactile push button (start) | any 6×6 mm SPST | TME | 1 | 0.30 | 0.30 | 🟡 |
| Tactile push button (tare) | any 6×6 mm SPST | TME | 1 | 0.30 | 0.30 | 🟡 |

### 1.6 Expansion headers

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Box header 2×10 pin IDC, 2.54 mm | any | TME | 2 | 0.50 | 1.00 | 🟢 |
| IDC ribbon cable, 20-wire, 30 cm | any | TME | 2 | 2.00 | 4.00 | 🟢 |
| IDC socket 2×10 (for cable ends) | any | TME | 4 | 0.40 | 1.60 | 🟢 |
| I²C identification EEPROM 24AA02 (per module) | Microchip `24AA02T-I/OT` | M | 2 | 0.30 | 0.60 | 🟢 |

### 1.7 Enclosure (base station)

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| 3D-printed enclosure, ~180×130×80 mm | STL files TBD | own printer | 1 | 8.00 | 8.00 | 🟢 |
| ⚪ *alternative* off-the-shelf ABS enclosure | Hammond `1591GSBK` | RS / TME | 1 | 18.00 | 18.00 | ⚪ |
| M3 brass inserts for PCB mounting | any | AMZ | 8 | 0.10 | 0.80 | 🟡 |
| M3 × 8 mm screws | any | AMZ | 8 | 0.05 | 0.40 | 🟡 |
| Rubber feet (adhesive) | any | AMZ | 4 | 0.10 | 0.40 | 🟡 |

### 1.8 PCB / perfboard

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Stripboard / perfboard, 100×80 mm (Phase 1) | any | TME | 1 | 3.00 | 3.00 | 🟢 |
| Custom PCB (Phase 2, after validation) | JLCPCB / OSH Park | JLCPCB | 5 | 2.00 | 10.00 | 🔵 |
| Hookup wire, AWG 24, multicolor set | any | TME | 1 set | 5.00 | 5.00 | 🟢 |
| Solder, 0.8 mm, lead-free, 100 g | any | TME | 1 | 8.00 | 8.00 | 🟢 |

### Base station subtotal

| Category | Cost (€) |
|---|---|
| Phycommander core | 43.00 |
| TIA front-end (with S1227) | 36.70 |
| Power distribution | 8.50 |
| Safety interlock | 3.61 |
| Status LEDs & UX | 0.96 |
| Expansion headers | 7.20 |
| Enclosure | 9.60 |
| PCB / perfboard + consumables | 16.00 |
| **Base station total** | **~€126** |

(~€97 if you use a BPW34 instead of the S1227, but you lose UV capability.)

---

## 2. Absorbance + nephelometry optical head

The default optical head, usable for 13 of the 16 measurement modes.

### 2.1 Light sources

| Item | λ (nm) | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|---|
| UV LED 340 nm | 340 | Bivar `UV5TZ-340-15` | M / DK | 1 | 8.00 | 8.00 | 🟢 default |
| ⚪ UV LED 365 nm (cheaper alternative) | 365 | Nichia `NSPU510CS` | M | 1 | 3.00 | 3.00 | ⚪ |
| Violet LED 430 nm | 430 | Kingbright `L-7113PBC-B-U` | TME | 1 | 1.00 | 1.00 | 🟢 |
| Blue LED 470 nm | 470 | Kingbright `L-7113PBC-J` | TME | 1 | 0.50 | 0.50 | 🟢 |
| Green LED 525 nm | 525 | Kingbright `L-7113GC` | TME | 1 | 0.50 | 0.50 | 🟢 |
| Amber LED 590 nm | 590 | Kingbright `L-7113SYC` | TME | 1 | 0.50 | 0.50 | 🟢 |
| Red laser 5 V, 650 nm (user-supplied) | 650 | KY-008 style module | — | 1 | 0.00 | 0.00 | 🟢 |
| Deep red LED 740 nm | 740 | Vishay `VSLY5940` | M / TME | 1 | 0.80 | 0.80 | 🟢 |
| NIR LED 940 nm | 940 | Vishay `TSAL6100` | M / TME | 1 | 0.50 | 0.50 | 🟢 |

### 2.2 Driver components (per source)

For the 7 LEDs + 1 laser, replicated ×8 except as noted:

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Logic-level N-MOSFET | ON Semi `2N7000TA` (TO-92) | TME / RS | 8 | 0.30 | 2.40 | 🟢 |
| ⚪ SMD alternative | Infineon `IRLML2502TRPBF` (SOT-23) | M | 8 | 0.35 | 2.80 | ⚪ |
| Gate resistor 100 Ω | any | TME | 8 | 0.02 | 0.16 | 🟢 |
| Gate pull-down 10 kΩ | any | TME | 8 | 0.02 | 0.16 | 🟢 |
| LED current limit resistor | see §4.4 of [`BEER_ANALYZER.md`](../../BEER_ANALYZER.md) | TME | 7 | 0.05 | 0.35 | 🟢 |
| Comparator (for DAC0/DAC1 → square-wave carriers) | TI `TLV3201AIDBVR` | M / DK | 2 | 2.00 | 4.00 | 🟢 |

### 2.3 Secondary TIA (90° haze port)

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Secondary photodiode | Vishay `BPW34` (cheaper is fine for scattering) | TME | 1 | 1.00 | 1.00 | 🟡 |
| Secondary op-amp | TI `OPA381AIDBVR` (same as primary) | M | 1 | 5.50 | 5.50 | 🟡 |
| Feedback R 1 MΩ + C 10 pF | any | TME | 1 set | 0.40 | 0.40 | 🟡 |

### 2.4 Optics / mechanics

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Cuvettes, plastic PS, 1 cm, 100 pack (visible) | BrandTech `759015` or AMZ equivalent | AMZ / DK | 1 pack | 5.00 | 5.00 | 🟢 |
| Cuvettes, UV-transparent (for 340 nm CH0) | BrandTech `759200` (10 pack) | DK | 1 pack | 15.00 | 15.00 | 🟢 |
| PTFE optical mixer rod, 3 mm × 30 mm | RS `762-2881` or ALI | RS / ALI | 1 | 3.00 | 3.00 | 🟢 |
| LED holder, 5 mm panel-mount | any | TME | 8 | 0.20 | 1.60 | 🟡 |
| Photodiode holder | any | TME | 2 | 0.30 | 0.60 | 🟡 |
| Matte black filament for enclosure | any black PETG or ABS | AMZ | 1 kg | 20.00 | 20.00 | 🟢 |

### 2.5 Head enclosure

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| 3D-printed head body (STL TBD) | own printer | — | 1 | 5.00 | 5.00 | 🟢 |
| Spring-loaded cuvette retention | AMZ | AMZ | 1 | 1.50 | 1.50 | 🟡 |

### Absorbance head subtotal

| Category | Cost (€) |
|---|---|
| Light sources (default with 340 nm UV) | 11.80 |
| Driver components | 7.07 |
| Secondary TIA (90° port) | 6.90 |
| Optics / mechanics | 45.20 |
| Head enclosure | 6.50 |
| **Absorbance head total** | **~€77** |

---

## 3. Fluorescence optical head (optional)

Alternative head for modes 3.5 (fluorescence). Swaps in place of the
absorbance head.

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Excitation LED (application-specific, e.g. 430 nm for chlorophyll) | Kingbright or Cree | TME | 1 | 3.00 | 3.00 | 🟢 |
| Plano-convex lens, f = 20 mm, 10 mm Ø | Thorlabs `LA1074` | Thorlabs | 1 | 12.00 | 12.00 | 🟢 |
| ⚪ cheap alternative lens | ALI generic | ALI | 1 | 2.00 | 2.00 | ⚪ |
| Long-pass emission filter (example: 600 nm cut-on) | Thorlabs `FEL0600` | Thorlabs | 1 | 65.00 | 65.00 | 🟢 |
| Reference photodiode (for source intensity monitoring) | Vishay `BPW34` | TME | 1 | 1.00 | 1.00 | 🟡 |
| Second op-amp for reference TIA | TI `OPA381` | M | 1 | 5.50 | 5.50 | 🟡 |
| MOSFET driver (reuse from absorbance head) | `2N7000` | TME | 1 | 0.30 | 0.30 | 🟢 |
| Baffle set + optical cage | 3D-printed | own | 1 | 2.00 | 2.00 | 🟢 |
| Head enclosure (3D print) | own | — | 1 | 5.00 | 5.00 | 🟢 |

**Fluorescence head total: ~€96** (dominated by the emission filter).

---

## 4. Multi-angle nephelometer head (optional, for particle sizing)

Uses **six additional 650 nm lasers** (user-supplied). The single
photodiode is the primary one on the base station; angular separation
is achieved by lock-in multiplexing.

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| 650 nm lasers (user-supplied, KY-008 style) | — | — | 6 | 0.00 | 0.00 | 🟢 |
| MOSFETs for each laser | `2N7000` | TME | 6 | 0.30 | 1.80 | 🟢 |
| Current-limit resistors | any | TME | 6 | 0.05 | 0.30 | 🟢 |
| Angular mounting jig (3D-printed) | own | — | 1 | 10.00 | 10.00 | 🟢 |
| Polystyrene latex size standards (calibration) | Thermo `4205A` series set (50, 100, 200, 500 nm) | Fisher | 1 set | 120.00 | 120.00 | 🟢 |
| Head enclosure | own | — | 1 | 5.00 | 5.00 | 🟢 |

**Multi-angle head total (with calibration standards): ~€137**

---

## 5. Laser Doppler velocimetry head (optional)

Two crossed 650 nm lasers for flow / velocity measurements.

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| 650 nm lasers (user-supplied) | — | — | 2 | 0.00 | 0.00 | 🟢 |
| Precision adjustable laser mounts | Thorlabs `KM100` (2 pieces) | Thorlabs | 2 | 35.00 | 70.00 | 🟢 |
| ⚪ cheaper DIY mount (3D-printed + ball joints) | own | — | 2 | 5.00 | 10.00 | ⚪ |
| Focusing lenses (short f, ~10 mm) | ALI or Thorlabs `LA1050` | Thorlabs | 2 | 12.00 | 24.00 | 🟢 |
| Beam dump (to absorb transmitted laser light) | any matt black metal plate | own | 1 | 0.50 | 0.50 | 🟢 |
| Detection photodiode (reuse primary on base) | — | — | 0 | 0.00 | 0.00 | 🟢 |
| Head enclosure | own | — | 1 | 5.00 | 5.00 | 🟢 |

**Doppler head total: ~€100** (or ~€40 with DIY mounts).

---

## 6. Electrochemical module

Adds pH, ORP, ISE (potentiometry), AC conductivity, and amperometric
sensor support.

### 6.1 Amplifier stage

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Instrumentation amp (high-Z, for pH/ORP) | TI `INA116PA` (DIP-16, or `INA116IBDR` SMD) | M / DK | 1 | 15.00 | 15.00 | 🟢 |
| Dual op-amp (potentiostat) | TI `OPA2192IDR` | M | 1 | 6.00 | 6.00 | 🟢 |
| Precision voltage reference 3.0 V | TI `REF3030AIDBZR` | M | 1 | 3.00 | 3.00 | 🟢 |
| Precision sense resistor, 10 kΩ, 0.1 % | Vishay `MRS16-T0-10K-0.1%` | M | 1 | 1.00 | 1.00 | 🟢 |
| Various resistors, caps | any | TME | 1 lot | 2.00 | 2.00 | 🟢 |

### 6.2 Electrode connectors

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| BNC panel-mount connector (pH/ORP) | any | TME / RS | 3 | 2.00 | 6.00 | 🟢 |
| 3.5 mm audio jack (temp probe input) | any | TME | 1 | 1.00 | 1.00 | 🟢 |

### 6.3 Electrodes & standards

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Combination pH electrode (glass + Ag/AgCl ref) | Hamilton `Flatrode` or Sentek `P11` | Hamilton / Sentek / AMZ | 1 | 40.00 | 40.00 | 🟢 |
| ⚪ Budget pH electrode | generic AMZ BNC pH | AMZ | 1 | 12.00 | 12.00 | ⚪ |
| ORP electrode | Sensorex `ORP-1400` | Sensorex / AMZ | 1 | 35.00 | 35.00 | 🟡 |
| Conductivity 2-electrode cell | Sensorex `CS150TC` (K=1.0) | Sensorex / AMZ | 1 | 30.00 | 30.00 | 🟡 |
| Clark DO electrode (dissolved O₂) | YSI `5739` or generic | YSI / AMZ | 1 | 80.00 | 80.00 | 🔵 |
| pH calibration buffers 4.01 / 7.00 / 10.01 (500 mL × 3) | Hanna `HI70007` / `HI70004` / `HI70010` | AMZ | 1 set | 15.00 | 15.00 | 🟢 |
| KCl conductivity standards (84 µS / 1413 µS / 12.88 mS) | Hanna `HI7030` / `HI7031` / `HI7039` | AMZ | 1 set | 15.00 | 15.00 | 🟡 |

### 6.4 EEPROM & PCB

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Module identification EEPROM | Microchip `24AA02T-I/OT` | M | 1 | 0.30 | 0.30 | 🟢 |
| PCB or perfboard | any | TME | 1 | 5.00 | 5.00 | 🟢 |
| IDC cable connector to base | any 2×10 | TME | 1 | 0.80 | 0.80 | 🟢 |

### Electrochemical module subtotal

| Variant | Cost (€) |
|---|---|
| **Minimum** (amps + BNC + pH electrode + buffers) | ~€78 |
| **Full** (+ ORP + conductivity cell + standards) | ~€158 |
| **+ DO** | ~€238 |

---

## 7. Actuator module (motors, pumps, Peltier)

Adds temperature control, stirring, titration, and multi-cuvette
handling.

### 7.1 Motor drivers

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Stepper driver breakout (for cuvette turret) | Pololu `A4988` or `DRV8825` | Pololu / M | 1 | 5.00 | 5.00 | 🟢 |
| DC motor H-bridge (for stirrer + peristaltic pump) | TI `DRV8871DDAR` | M | 2 | 3.00 | 6.00 | 🟢 |
| Peltier H-bridge (high current) | Infineon `BTS7960` breakout | AMZ / ALI | 1 | 8.00 | 8.00 | 🟢 |

### 7.2 Motors, pumps, Peltier

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| NEMA 17 stepper, 1.8°, 0.4 N·m | generic | AMZ / ALI | 1 | 12.00 | 12.00 | 🟢 |
| 5 V DC stirrer motor (small, <20 mA) | generic | ALI | 1 | 4.00 | 4.00 | 🟡 |
| Magnetic stirrer bar 10 mm | any | AMZ | 1 set | 5.00 | 5.00 | 🟡 |
| Peristaltic pump, 12 V, 0.1 – 10 mL/min | Kamoer `NKP-DC-S06` or generic | ALI / AMZ | 1 | 25.00 | 25.00 | 🟢 |
| Silicon tubing for pump | 3×5 mm, 1 m | AMZ | 1 | 3.00 | 3.00 | 🟢 |
| Peltier TEC, 40 W | Adafruit `1335` or `TEC1-12706` | Adafruit / ALI | 1 | 8.00 | 8.00 | 🟢 |
| Heatsink for Peltier hot side, 40×40 mm | any CPU heatsink | AMZ | 1 | 4.00 | 4.00 | 🟢 |
| Fan 40 mm for Peltier heatsink | any 12 V | AMZ | 1 | 3.00 | 3.00 | 🟢 |
| Thermal grease / thermal paste | any | AMZ | 1 | 2.00 | 2.00 | 🟢 |
| NTC for Peltier overheating | 10 kΩ NTC | TME | 1 | 0.30 | 0.30 | 🟢 |

### 7.3 Mechanical

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| 6-position cuvette turret (3D-printed + bearing) | own | — | 1 | 12.00 | 12.00 | 🟢 |
| Flexible coupler (stepper → turret) | 5 mm × 8 mm | AMZ | 1 | 3.00 | 3.00 | 🟢 |
| Bolts, bearings, brackets | — | AMZ | 1 lot | 10.00 | 10.00 | 🟢 |

### 7.4 EEPROM & PCB

| Item | Part number | Supplier | Qty | Unit € | Total € | Status |
|---|---|---|---|---|---|---|
| Module EEPROM | Microchip `24AA02T-I/OT` | M | 1 | 0.30 | 0.30 | 🟢 |
| PCB / perfboard | any | TME | 1 | 5.00 | 5.00 | 🟢 |
| IDC cable | any | TME | 1 | 0.80 | 0.80 | 🟢 |

### Actuator module subtotal

| Variant | Cost (€) |
|---|---|
| **Minimum** (stepper + turret + thermal control) | ~€83 |
| **Full** (+ pump + stirrer + DO electrode) | ~€108 |

---

## 8. Reagents (by application)

These are consumables, not hardware. Budgets given per 100 measurements
unless otherwise noted.

### 8.1 Beer QC panel

| Reagent | Purpose | Part number | Supplier | Cost € | # assays |
|---|---|---|---|---|---|
| **Megazyme K-ETOH Ethanol Kit** (default) | Ethanol by ADH enzymatic | `K-ETOH` | Megazyme | 120 | ~100 |
| ⚪ DIY from Sigma: ADH 100 mg | Ethanol alt | `A3263` | Sigma | 50 | scalable |
| ⚪ DIY from Sigma: NAD⁺ 500 mg | Ethanol alt | `N7004` | Sigma | 35 | scalable |
| Bradford Coomassie reagent 500 mL | Protein | Bio-Rad `5000006` | Bio-Rad / AMZ | 40 | ~300 |
| BSA protein standard, 10 mg/mL | Protein calibration | Sigma `A7906` | Sigma | 20 | ongoing |
| Folin-Ciocalteu reagent 2 N, 500 mL | Polyphenols | Sigma `F9252` | Sigma | 35 | ~500 |
| Gallic acid monohydrate, 10 g | Polyphenol calibration | Sigma `G7384` | Sigma | 15 | ongoing |
| Sodium carbonate anhydrous, 500 g | Folin-C buffer | any chem | AMZ / chem | 5 | ∞ |
| 3,5-Dinitrosalicylic acid (DNS), 100 g | Reducing sugars | Sigma `D0550` | Sigma | 25 | ~500 |
| D-Glucose anhydrous, 100 g | Sugar calibration | any chem | AMZ | 5 | ongoing |
| Formazin 4000 NTU standard | Haze calibration | Hach `2461100` | Hach | 25 | ongoing |
| o-Phenanthroline monohydrate, 10 g | Iron | Sigma `131377` | Sigma | 15 | ~500 |
| Ethanol absolute, 500 mL | Ethanol calibration standards | Sigma `32221` | Sigma | 15 | ongoing |

**Beer panel total (Megazyme): ~€320**
**Beer panel total (DIY enzymes): ~€285**

### 8.2 Water quality panel

| Reagent | Purpose | Supplier | Cost € |
|---|---|---|---|
| DPD free chlorine test reagent | Chlorine colorimetric | Hach `14070-32` | 20 |
| Griess reagent (NED + sulfanilamide) | Nitrite / nitrate | Sigma | 35 |
| Nitrate cadmium reduction column | Nitrate → nitrite | Hach | 25 |
| Ammonia (salicylate method) reagent kit | Ammonia | Hach `2604545` | 20 |
| Phosphate (molybdenum blue) reagent | Phosphate | Hach | 20 |
| Iron (phenanthroline) — same as §8.1 | Iron | — | 0 |
| Manganese (formaldoxime) reagent | Mn | Sigma | 15 |
| Copper (bicinchoninate) reagent | Cu | Hach | 15 |
| EDTA disodium salt, 100 g | Hardness titration | any chem | 8 |
| Eriochrome Black T indicator, 5 g | Hardness | Sigma | 10 |

**Water panel total: ~€168**

### 8.3 Calibration standards (common to all)

| Reagent | Purpose | Supplier | Cost € |
|---|---|---|---|
| Methylene blue, 25 g | Beer-Lambert linearity check | Sigma `M9140` | 12 |
| pH buffers (4/7/10) set | pH electrode calibration | Hanna | 15 |
| KCl conductivity standards set | Conductivity calibration | Hanna | 15 |
| Polystyrene latex standards (multi-size) | Nephelometer calibration | Fisher | 120 |
| Distilled water, 5 L | Blanks | supermarket | 3 |

**Common standards total: ~€165**

---

## 9. Tier totals

| Tier | Contents | Hardware (€) | Reagents (€) | **Total (€)** |
|---|---|---|---|---|
| **Starter** | Base station + absorbance head (no UV cuvettes, no laser-specific parts) | ~145 | ~15 (methylene blue + ethanol for calib) | **~160** |
| **Beer QC** | Base + absorbance + UV-capable cuvettes | ~205 | ~320 | **~525** |
| **Beer QC (DIY enzymes)** | Same, DIY reagents | ~205 | ~285 | **~490** |
| **Water quality** | Base + absorbance + electrochemical (pH+cond) | ~285 | ~200 | **~485** |
| **Full modular bench** | Base + absorbance + electrochem + actuator | ~380 | ~350 | **~730** |
| **Full + fluorescence** | + Fluorescence head | ~476 | ~350 | **~826** |
| **Full + nephelometer** | + Multi-angle head + standards | ~517 | ~350 | **~867** |
| **Everything** | All modules, all heads, all major reagent kits | ~580 | ~500 | **~1080** |

---

## 10. Shopping checklist for the recommended first build

If you are building this instrument for the first time and want the
shortest path to a working beer ethanol measurement, order these items
together. This is **Tier: Beer QC (default)**, total **~€525**.

```
# BASE STATION
[ ] Arduino Due                                  M:A000062             €40
[ ] Hamamatsu S1227-1010BR photodiode            M:S1227-1010BR        €30
[ ] OPA381 op-amp                                 M:OPA381AIDBVR       €5.50
[ ] Passive components kit (Rs, Cs, regulators)   — assorted          ~€10
[ ] SIM1-0512 DC-DC ±12 V                        M:SIM1-0512S          €8
[ ] Omron D2F-01L microswitch                    M:D2F-01L             €1.50
[ ] Omron G5V-1-DC5 relay                        M:G5V-1-DC5           €2
[ ] IDC headers, ribbon, sockets                  — assorted          ~€10
[ ] Perfboard + hookup wire + solder              — assorted          ~€16
[ ] 3D-printed enclosure                          own                  €8

# ABSORBANCE HEAD
[ ] Bivar UV5TZ-340 (340 nm UV LED)              M:UV5TZ-340-15        €8
[ ] 6× LEDs (430, 470, 525, 590, 740, 940 nm)    TME                   €4.80
[ ] 8× 2N7000 MOSFETs + passives                  TME                  €3
[ ] 2× TLV3201 comparators                        M:TLV3201AIDBVR      €4
[ ] BPW34 + OPA381 for 90° haze port (optional)  TME + M              €7
[ ] PMMA-UV cuvettes, 10-pack                     DK:759200            €15
[ ] Standard PS cuvettes, 100-pack                AMZ                  €5
[ ] PTFE rod optical mixer                        RS:762-2881          €3
[ ] 3D-printed head body                          own                  €5

# REAGENTS
[ ] Megazyme K-ETOH kit                          Megazyme:K-ETOH      €120
[ ] Bradford reagent                              Bio-Rad:5000006      €40
[ ] BSA standard                                  Sigma:A7906          €20
[ ] Folin-Ciocalteu                               Sigma:F9252          €35
[ ] Gallic acid                                   Sigma:G7384          €15
[ ] DNS reagent + D-glucose                       Sigma                €30
[ ] Formazin 4000 NTU                             Hach:2461100         €25
[ ] Methylene blue (for Beer-Lambert calib)       Sigma:M9140          €12
[ ] Ethanol absolute                              Sigma:32221          €15

# SAFETY
[ ] Laser safety glasses, OD 2+ @ 650 nm          Thorlabs / Amazon    €15

TOTAL: ~€524
```

---

## 11. Suppliers and lead times (indicative)

| Supplier | Region | Typical lead time | Notes |
|---|---|---|---|
| **Mouser** | EU / Global | 3–7 days | Best for TI / Microchip / Vishay ICs |
| **Digi-Key** | EU / Global | 5–10 days | Good for Hamamatsu and specialty |
| **TME** | EU | 2–5 days | Good prices for passives, LEDs, connectors |
| **RS Components** | EU / UK | 3–7 days | Good for mechanical parts, enclosures |
| **Sigma-Aldrich** | Global | 7–14 days | Lab chemicals — requires institutional account in many countries |
| **Megazyme** | EU / Ireland | 7–10 days | Beer analysis kits |
| **Bio-Rad** | EU / Global | 7–14 days | Bradford and protein kits |
| **Thorlabs** | EU / Global | 5–10 days | Optics, filters, mounts |
| **Hamamatsu** | Japan / EU | 10–14 days | Direct for photodetectors, or via Mouser |
| **Amazon** | local | 1–3 days | Common components, low-cost alternatives |
| **AliExpress** | CN | 15–30 days | Cheap alternatives where quality is non-critical |

**Tip**: place the order for Sigma/Megazyme/Bio-Rad reagents *first* —
those typically have the longest lead time. You can start building the
electronics and mechanics in parallel while waiting for chemicals.
