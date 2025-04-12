# Hardware schematics — Multi-function analyzer

> Textual (ASCII-art + netlist) schematics of every analog circuit
> required by the multi-function analyzer. Usable as the source for
> transcription into KiCad / Eagle / EasyEDA, or for direct perfboard
> assembly.
>
> **Conventions**:
> - Component designators: R1, C1, U1, Q1, D1, J1, LED1, ...
> - Nets are in `UPPER_CASE` (e.g. `VCC_3V3`, `ADC0`, `DOUT3`)
> - All `R` are 1 % metal film thin-film unless otherwise stated
> - All capacitors ≤ 1 µF are 0805 X7R ceramic unless otherwise stated
> - All capacitors > 1 µF are tantalum or low-ESR electrolytic
> - `TP` = test point

---

## Table of Contents

1. [Power distribution](#1-power-distribution)
2. [Transimpedance amplifier (primary TIA)](#2-transimpedance-amplifier-primary-tia)
3. [Secondary TIA (90° haze port)](#3-secondary-tia-90-haze-port)
4. [Light source driver (per LED / laser)](#4-light-source-driver-per-led--laser)
5. [DAC-to-square-wave comparator](#5-dac-to-square-wave-comparator)
6. [Safety interlock](#6-safety-interlock)
7. [Laser master-enable relay](#7-laser-master-enable-relay)
8. [Phycommander pin-to-net assignment](#8-phycommander-pin-to-net-assignment)
9. [Expansion headers](#9-expansion-headers)
10. [Electrochemical module](#10-electrochemical-module)
11. [Actuator module](#11-actuator-module)
12. [Netlist (base station, minimal build)](#12-netlist-base-station-minimal-build)
13. [Test points](#13-test-points)
14. [Assembly notes](#14-assembly-notes)

---

## 1. Power distribution

Phycommander's Arduino Due provides 5 V (from USB) and 3.3 V (from its
onboard LDO). The analyzer adds a DC-DC converter to generate ±12 V for
the electrochemical and actuator modules.

```
                ┌─────────────┐                 VCC_5V
    USB  ───────┤ Arduino Due ├─────────────────────●─────────────────────→
                │             │                     │
                │  onboard    │                     │
                │  LDO 3.3V   │                     ▼
                │             │             ┌─────────────┐
                │             │             │  SIM1-0512  │ VCC_12V
                │             │     VCC_5V ─┤+Vin     +Vo ├──●───────────→
                │             │             │             │
                │             │     GND    ─┤-Vin     0V  ├──●──── GND ──→
                │             │             │             │
                │             │             │         -Vo ├──●──── VCC_N12V
                │             │             └─────────────┘    (−12 V)
                │             │
                │  VCC_3V3    │
                └──────┬──────┘
                       │
                       ●─────────────────────────────── VCC_3V3 ──→
                       │
                       │
                      GND ──●─────────────────────────── GND ──→
```

**Bill of components (power)**:

| Ref | Value / P/N | Function |
|---|---|---|
| U1 | Meanwell `SIM1-0512S` | 5 V → ±12 V DC-DC |
| C1 | 10 µF / 25 V | VCC_5V input bulk cap |
| C2 | 100 nF | VCC_5V decoupling near U1 |
| C3 | 10 µF / 25 V | VCC_12V output cap |
| C4 | 10 µF / 25 V | VCC_N12V output cap |
| C5 | 100 nF | VCC_12V decoupling |
| C6 | 100 nF | VCC_N12V decoupling |
| FB1 | 600 Ω @ 100 MHz ferrite | optional EMI suppression on VCC_5V |
| LED1 | 5 mm green | power indicator on VCC_5V (via 1 kΩ to GND) |
| R_pwr | 1 kΩ | current limit for LED1 |

---

## 2. Transimpedance amplifier (primary TIA)

The heart of the instrument. A photocurrent from the Hamamatsu S1227
photodiode flows into a transimpedance op-amp whose output voltage is
proportional to the photocurrent. The output sits at 1.65 V
(mid-rail of the 0 – 3.3 V ADC range) in the dark and swings above
(for positive photocurrent, i.e. light present) during measurement.

```
                 ┌──────── VCC_3V3
                 │
                R3 (100 kΩ)
                 │
                 ●───────────● TP_VBIAS (1.65 V)
                 │           │
                R4 (100 kΩ)  C10 (10 µF) + C11 (100 nF)
                 │           │
                 ●─────────── GND
                 │
                 │  non-inverting
                 │
                 │                  R_f (10 MΩ)
                 │           ┌─────/\/\/\─────┐
                 │           │                │
                 │           │   C_f (1 pF)   │
                 │           ├──────||────────┤
                 │           │                │
                 │           │  inverting     │
                 │           │                │
                 └───────────┤+  U2           │
                             │    OPA381     ├──── VBIAS_TIA_OUT ──● ADC0
                          ┌──┤-               │                    │
                          │  │                │                    C12 (100 pF)
                          │  │ VCC_3V3 ── VCC │                    │
                          │  │ GND     ── V-  │                    GND
                          │  └────────────────┘
                          │
                          │ to photodiode cathode
                          │
                     ┌────┴────┐
                     │ PD1     │  Hamamatsu S1227-1010BR
                     │         │
                     │ anode   ├────────── GND
                     └─────────┘
```

**Circuit rationale**:

- The photodiode's cathode connects to the op-amp's inverting input.
  Its anode connects to ground. In this "photovoltaic" (zero-bias) mode
  the photodiode capacitance is maximum (~50 pF for S1227) but the dark
  current is minimum.
- R_f = 10 MΩ sets the transimpedance: 1 nA of photocurrent → 10 mV of
  output.
- C_f = 1 pF compensates the noise-gain peaking introduced by the
  photodiode capacitance plus the op-amp input capacitance. The
  stability condition is
  `C_f ≥ √((C_d + C_in) / (2π R_f · GBW))`. For S1227 (C_d = 50 pF) +
  OPA381 (C_in = 7 pF, GBW = 18 MHz), this gives `C_f ≥ 0.7 pF`.
- R3, R4 form a mid-rail bias of 1.65 V. C10, C11 decouple.
- C12 (100 pF) is a small anti-aliasing cap on the output node to
  suppress any out-of-band noise before the ADC. Optional.
- TP_VBIAS gives you a test point for verifying the mid-rail bias.

**Bill of components (TIA)**:

| Ref | Value / P/N | Function |
|---|---|---|
| U2 | TI `OPA381AIDBVR` (SOT-23-5) | Transimpedance op-amp |
| PD1 | Hamamatsu `S1227-1010BR` | Primary photodiode |
| R_f | 10 MΩ 1 % thin-film | TIA feedback |
| C_f | 1 pF C0G 0805 | TIA compensation |
| R3, R4 | 100 kΩ 1 % | Mid-rail bias divider |
| C10 | 10 µF tantalum | Bias cap bulk |
| C11 | 100 nF X7R | Bias cap HF |
| C12 | 100 pF C0G | Anti-aliasing on output |
| TP_VBIAS | test point header | Debug |

**Tuning R_f**:

If your signal is too small (< 10 % of full scale at the brightest
blank), increase R_f to 47 MΩ or 100 MΩ. If it saturates (close to
3.3 V at blank), decrease to 1 MΩ. Each tenfold change in R_f changes
the optimum C_f by √10; re-tune C_f accordingly (at 100 MΩ, C_f ≈ 0.2
pF, which is impractical — use a short trace and trust the parasitic).

---

## 3. Secondary TIA (90° haze port)

A second TIA is mounted at 90° to the main optical axis, behind a
second photodiode, for nephelometric haze measurement. The circuit is
identical to the primary TIA except:

- PD2 is a cheaper **BPW34** (visible-only, ~4 mm² active area),
  because the haze channel only needs the 650 nm laser signal and does
  not need UV response.
- R_f2 = 1 MΩ (not 10 MΩ). Scattered light is much weaker than
  transmitted light, but the photodiode area is 20× smaller, so a
  lower R_f compensates.

```
         (Copy of §2 circuit, but:)
         U3 = OPA381 (second instance)
         PD2 = BPW34
         R_f2 = 1 MΩ
         C_f2 = 4.7 pF    (stability condition for smaller C_d)
         output node → VBIAS_TIA2_OUT → ADC1
```

---

## 4. Light source driver (per LED / laser)

One copy per channel. 8 channels in the base absorbance head.

```
                VCC_5V  (for LEDs)
                   │
                   │
                   R_led  (current limit, see table)
                   │
                   │
              ┌────┴────┐
              │         │
              │  LEDx   │ (λ-specific)
              │         │
              │  (anode │
              │   to    │
              │  R_led) │
              │         │
              │ cathode │
              └────┬────┘
                   │
                   │
                   ● Drain
                   │
                   │
              ┌────┴────┐
              │         │
              │ Q_x     │ 2N7000 (or IRLML2502)
              │  N-ch   │
              │ MOSFET  │
              │         │
              └────┬────┘
                   │
                   ● Source ── GND
                   │
                  Gate
                   │
                R_gate (100 Ω)
                   │
                   ●─────── DRV_CHx  (from phycommander
                   │         PWM / DAC-compare / DOUT)
                  R_gp (10 kΩ)
                   │
                  GND
```

- **Q_x**: 2N7000 (TO-92) or IRLML2502 (SOT-23). Logic-level gate, V_GS(th) ≈ 1.5 V, I_D ≥ 200 mA.
- **R_gate = 100 Ω**: damps gate ringing during fast switching.
- **R_gp = 10 kΩ**: gate pull-down. Ensures the MOSFET is OFF if the
  drive signal is high-impedance at startup.
- **R_led**: current limit, sized per LED (see table below).

### R_led per channel (for 5 V rail, I_f = 20 mA typical):

| Channel | λ (nm) | V_f (V) | I_f (mA) | R_led (Ω) | E12 value |
|---|---|---|---|---|---|
| CH0 | 340 | 3.4 | 20 | 75 | 82 |
| CH1 | 430 | 3.2 | 20 | 85 | 100 |
| CH2 | 470 | 3.2 | 20 | 85 | 100 |
| CH3 | 525 | 3.0 | 20 | 95 | 100 |
| CH4 | 590 | 2.0 | 20 | 145 | 150 |
| CH5 | 650 (laser) | 5.0 (module) | 30 | 0 (direct)* | — |
| CH6 | 740 | 1.9 | 30 | 100 | 100 |
| CH7 | 940 | 1.3 | 40 | 90 | 100 |

*The 650 nm laser module is a complete 5 V unit with its own internal
current limiter. It is wired directly: `5V → laser module → MOSFET
drain`. See §7 for the safety interlock relay on the 5 V rail.

---

## 5. DAC-to-square-wave comparator

Phycommander's DAC0 and DAC1 produce sinusoidal waveforms generated in
software. To drive an LED's gate with a square wave (needed for
MOSFET switching), pass the sine through a comparator with a 1.65 V
threshold:

```
                   VCC_3V3
                      │
                      │
                   R5 (100 kΩ)
                      │
                      ●──────● VREF_MID (1.65 V)
                      │      │
                   R6 (100 kΩ)
                      │      C13 (100 nF)
                      │      │
                     GND     GND
                             │
                             │
                             │
                  DAC0 ──────●────┐
                                   │
                             ┌─────┴─────┐
                             │           │
                             │  TLV3201  │ comparator U4
                             │           │
                  VREF_MID ──┤+         ─┤├─── DRV_CH6 (740 nm gate)
                             │  1.65V   O│
                             │        ───┤
                             │           │
                             │ 3V3 ─ VCC │
                             │ GND ─ V-  │
                             └───────────┘
                             push-pull output
```

The comparator fires high whenever DAC0 > 1.65 V, low otherwise. The
result is a clean square wave of the same frequency as the DAC
sinusoid, suitable for driving a MOSFET gate. Repeat U4/U5 for the
second DAC channel driving CH7 (940 nm).

**Why not drive the gate with the DAC directly?** The DAC's output is
analog 0 – 3.3 V. At mid-rail the MOSFET is partially on, which
dissipates power and gives poor modulation depth. A comparator
sharpens the edge to a clean digital transition.

**Alternative**: use hardware PWM (PWM0, PWM1) for these channels, and
free DAC0/DAC1 for other uses (e.g. AC conductivity drive on the
electrochemical module). The base absorbance head uses the PWM
channels for CH0 (340 nm) and CH1 (430 nm); the DAC-compare path is
for CH6 (740 nm) and CH7 (940 nm). This is the default pin assignment
in [`FIRMWARE_SPEC_V1.1.md`](FIRMWARE_SPEC_V1.1.md) and §8 below.

---

## 6. Safety interlock

A normally-open microswitch on the enclosure cover grounds DIN5 when
the cover is closed. DIN5 is pulled up to 3.3 V by an internal
pull-up.

```
            VCC_3V3
               │
               │
            R10 (10 kΩ pull-up)
               │
               ●────── DIN5
               │
               │
           ┌───┴───┐
           │       │
           │  SW1  │  cover interlock microswitch (Omron D2F-01L)
           │       │  normally open, closes when cover is closed
           │       │
           └───┬───┘
               │
              GND
```

- When cover is **closed**: SW1 is closed, DIN5 is pulled to GND, logic 0.
- When cover is **open**: SW1 is open, R10 pulls DIN5 to 3.3 V, logic 1.

The firmware reads DIN5 every loop iteration and, when logic 1 ("cover
open"), unconditionally disables DOUT3, DOUT4, and PWM0 — the three
outputs involved in laser control. See
[`FIRMWARE_SPEC_V1.1.md`](FIRMWARE_SPEC_V1.1.md) §3.

---

## 7. Laser master-enable relay

A hardware relay in series with the laser's 5 V supply provides a
*physical* interruption of power when the interlock is open, in
addition to the firmware-level gating. Belt and suspenders.

```
            VCC_5V ───── F1 (500 mA) ─────┬────────→ VCC_LASER
                                           │
                                           │
                                     ┌─────┴─────┐
                                     │           │
                                     │  K1       │ Omron G5V-1 relay
                                     │  SPST NO  │ (normally open contact)
                                     │           │
                                     └─────┬─────┘
                                           │
                                           │
                                          GND  (laser module ground
                                                returns to GND via the
                                                modulation MOSFET Q5)
```

And the relay coil drive circuit:

```
              DOUT4 ──── R11 (1 kΩ) ──── Base
                                          │
                                        ┌─┴─┐
                                        │   │
                                        │Q2 │  BC547 NPN
                                        │   │
                                        └─┬─┘
                                          │
                                          Emitter ── GND
                                          │
                                          │Collector
                                          │
                                          ●
                                          │
                                     VCC_5V ● ──┐
                                               │
                                              K1 coil
                                               │
                                               ● to collector
                                               │
                                              D1 (1N4148)  flyback
                                               │
                                              (cathode to VCC_5V,
                                               anode to collector)
                                               │
                                              GND-return via emitter
```

Firmware sets DOUT4 high to energize K1, closing the relay contact
and supplying 5 V to the laser. When the firmware detects the
interlock open, it forces DOUT4 low regardless of host commands.

**Note**: the MOSFET Q5 (modulation driver) on the ground-return path
of the laser is separate from Q2 (relay driver) and acts as the
high-frequency modulator — carrier on DOUT3 or PWM0 controls Q5, while
DOUT4 controls Q2. Both must be commanded high and the interlock must
be closed for the laser to emit.

---

## 8. Phycommander pin-to-net assignment

Mapping of each phycommander logical channel (as exposed by v1.1
firmware) to the net names used throughout this document. This is the
source of truth for the PCB / perfboard wiring.

| phycommander channel | Net name | Function |
|---|---|---|
| **ADC0** | `VBIAS_TIA_OUT` | Primary TIA output (transmittance) |
| **ADC1** | `VBIAS_TIA2_OUT` | Secondary TIA output (90° haze) |
| **ADC2** | `VNTC_CUVETTE` | Cuvette NTC temperature (optional) |
| **ADC3** | `DAC0_LOOPBACK` | Wire-jumper to DAC0 for self-test |
| **ADC4** | `MOD_A_SIGNAL` | Electrochem module analog A (pH amp out) |
| **ADC5** | `MOD_A_CURRENT` | Electrochem module current sense (potentiostat) |
| **ADC6** | reserved | — |
| **ADC7** | reserved | — |
| **DAC0** | `DAC0` | Sine for comparator U4 → CH6 740 nm driver |
| **DAC1** | `DAC1` | Sine for comparator U5 → CH7 940 nm driver |
| **PWM0** | `PWM0` | Hardware square wave → CH0 340 nm driver |
| **PWM1** | `PWM1` | Hardware square wave → CH1 430 nm driver |
| **DOUT0** | `DRV_CH2` | Software-toggled square wave → CH2 470 nm driver |
| **DOUT1** | `DRV_CH3` | Software-toggled square wave → CH3 525 nm driver |
| **DOUT2** | `DRV_CH4` | Software-toggled square wave → CH4 590 nm driver |
| **DOUT3** | `DRV_CH5` | Software-toggled square wave → CH5 650 nm laser Q5 (INTERLOCK-GATED) |
| **DOUT4** | `LASER_EN` | Laser master enable → Q2 → K1 coil (INTERLOCK-GATED) |
| **DOUT5** | `LED_READY` | Status LED green |
| **DOUT6** | `LED_MEAS` | Status LED amber |
| **DOUT7** | `LED_FAULT` | Status LED red / laser-active indicator |
| **DOUT8** | `MOD_B_STEP` | Actuator module stepper STEP |
| **DOUT9** | `MOD_B_DIR` | Actuator module stepper DIR |
| **DOUT10** | `MOD_B_EN` | Actuator module stepper ENABLE |
| **DOUT11** | `MOD_B_PUMP_IN1` | Pump H-bridge IN1 |
| **DOUT12** | `MOD_B_PUMP_IN2` | Pump H-bridge IN2 |
| **DOUT13** | `MOD_B_PELT_DIR` | Peltier direction |
| **DOUT14** | `MOD_B_PELT_EN` | Peltier PWM / enable |
| **DOUT15** | `MOD_B_STIR_EN` | Stirrer enable |
| **DIN0** | `MOD_B_ENC_A` | Encoder A (turret) |
| **DIN1** | `MOD_B_ENC_B` | Encoder B (turret) |
| **DIN2** | `MOD_B_HOME` | Turret home sensor |
| **DIN3** | `BTN_START` | User start button |
| **DIN4** | `BTN_TARE` | User tare button |
| **DIN5** | `INTERLOCK` | Cover microswitch (SAFETY-CRITICAL) |
| **DIN6** | `EXT_TRIG` | External trigger input |
| **DIN7** | `MOD_B_PUMP_TACH` | Pump tachometer |

---

## 9. Expansion headers

Two 2×10 IDC headers (2.54 mm pitch) carry module signals off the
base station. Header A is for optical heads (or the electrochemical
module). Header B is for the actuator module.

### Header A pinout (20 pins)

| Pin | Signal | Pin | Signal |
|---|---|---|---|
| 1 | VCC_5V | 2 | GND |
| 3 | VCC_3V3 | 4 | GND |
| 5 | VCC_12V | 6 | VCC_N12V |
| 7 | DAC0 | 8 | DAC1 |
| 9 | PWM0 | 10 | PWM1 |
| 11 | VBIAS_TIA_OUT (analog in) | 12 | VBIAS_TIA2_OUT (analog in) |
| 13 | DRV_CH2 | 14 | DRV_CH3 |
| 15 | DRV_CH4 | 16 | DRV_CH5 (laser) |
| 17 | LASER_EN | 18 | INTERLOCK (from module back to base) |
| 19 | SDA (I²C, v1.2+) | 20 | SCL (I²C, v1.2+) |

Optical heads use this header. The TIA amplifiers live on the head
PCB (close to the photodiodes for noise), and their outputs return on
pins 11 and 12. The LED drivers also live on the head PCB (8 MOSFETs
+ 8 current-limit resistors, one per LED).

When the electrochemical module uses this header instead of an optical
head, pins 7, 11, 12 carry the instrumentation amplifier outputs, and
the LED driver pins (13 – 18) are unused.

### Header B pinout (20 pins)

| Pin | Signal | Pin | Signal |
|---|---|---|---|
| 1 | VCC_5V | 2 | GND |
| 3 | VCC_3V3 | 4 | GND |
| 5 | VCC_12V | 6 | VCC_N12V |
| 7 | MOD_B_STEP | 8 | MOD_B_DIR |
| 9 | MOD_B_EN | 10 | MOD_B_PUMP_IN1 |
| 11 | MOD_B_PUMP_IN2 | 12 | MOD_B_PELT_DIR |
| 13 | MOD_B_PELT_EN | 14 | MOD_B_STIR_EN |
| 15 | MOD_B_ENC_A | 16 | MOD_B_ENC_B |
| 17 | MOD_B_HOME | 18 | MOD_B_PUMP_TACH |
| 19 | SDA (I²C) | 20 | SCL (I²C) |

Actuator module uses this header.

### Module identification (optional, v1.2+)

An I²C EEPROM (Microchip 24AA02) on each module returns 32 bytes of
identification data (`{module_type, version, mfr, cal_date}`) when
queried at a fixed I²C address (e.g., `0x50` for Header A, `0x51` for
Header B). The host reads this at startup to determine which modules
are present and what modes to enable.

---

## 10. Electrochemical module

### 10.1 High-impedance pH / ORP buffer (INA116)

```
                                    VCC_12V
                                      │
                                      │
                                     pin7 (V+)
                                      │
     Electrode signal ────●───────────┤+      ┌─────► VBIAS_TIA_OUT →
     (BNC center)         │           │ INA116│        (to ADC4 on base)
                          │           │       │
     Electrode ref.   ────●───────────┤-      │
     (BNC shield)         │           │       │
                          │           │       │
                          │           │ REF  ─┤───── VREF_GND (0 V or 1.65 V)
                          │           │       │
                          │           │ RG    │  gain-set resistor
                          │           │       │  (short for gain=1,
                          │           │       │   10 kΩ for gain=2, etc.)
                          │           │       │
                          │           │ V−   ─┤─── VCC_N12V
                          │           │       │
                          │           └───────┘
                          │
                          │ guard ring on PCB around the
                          │ electrode input nets
                          │
                          └─── ESD diodes to VCC_3V3 and GND (optional)
```

- **U_electrochem_1** = TI `INA116PA`: ultra-high input impedance
  (> 10¹⁵ Ω) — does not load the pH glass electrode.
- Gain = 1 by default (open `RG`). Set gain to 10 by adding 5.56 kΩ
  between `RG` pins for a ±6 V → ±0.6 V swing matched to the ADC.
- The output goes through a 20 Hz single-pole RC filter (10 kΩ, 1 µF)
  before reaching ADC4 to reject mains pickup.

### 10.2 Conductivity driver (AC excitation + lock-in)

```
     DAC0 (1 kHz sine, 100 mVpp) ──● CELL_A (electrode 1)
                                    │
                                    │
                                   ─┴─
                                  /   \
                                 /     \
                                 \     /
                                  \___/
                                 cell (solution)
                                  ___
                                 /   \
                                 \     \
                                 /     /
                                  \___/
                                    │
                                    │
                                    ● CELL_B (electrode 2)
                                    │
                                    │
                                   R_s (10 kΩ, 0.1 %)
                                    │
                                    │
                                   GND

        CELL_B voltage ── differential amp ── ADC5 MOD_A_CURRENT
                              U_electrochem_2 (OPA2192 half)
```

- DAC0 drives one electrode with a 1 kHz sinewave at 100 mVpp
  (amplitude low to avoid electrolysis).
- The current through the solution returns to GND through R_s, so the
  voltage across R_s is proportional to cell conductance.
- A differential amp measures V(R_s) and feeds ADC5.
- On the host, lock-in at 1 kHz on ADC5 gives conductance (in-phase)
  and capacitance (quadrature).
- Conductivity `σ = G × K` where K is the cell constant from the
  calibration.

### 10.3 Potentiostat (amperometry)

A three-electrode potentiostat using the second half of OPA2192 plus
a transimpedance stage:

```
                        VCC_3V3
                          │
                         ┌┴┐
                         │ │
                         │ │
                         └┬┘
                          ● ──────────── DAC1 (potential setpoint)
                          │
                          ┌─────────────┐
                          │             │
                          │   OPA2192A  │ control amp
                          │             │
                 WE ──────┤−            │
                          │             │
                 CE ──────┤+            │
                          │             │
                          └─────────────┘
                                 │
                                 │
                                 └──────── Reference feedback
                                            (connection to electrochem
                                             cell reference electrode
                                             in 3-electrode config)
```

- **Working electrode (WE)** is held at the DAC1 voltage via feedback.
- **Reference electrode (RE)** senses the potential without drawing
  current (goes to the op-amp non-inverting input through a high-Z
  follower, not shown in full above).
- **Counter electrode (CE)** sinks the current needed to maintain the
  WE potential.
- A second op-amp converts the WE current to voltage for the ADC.

This is the standard Clark O₂ electrode drive, glucose electrode drive,
or any three-electrode amperometric sensor interface.

### 10.4 Electrochemical BOM summary

| Ref | Value / P/N | Function |
|---|---|---|
| U_e1 | TI `INA116PA` | pH / ORP buffer |
| U_e2 | TI `OPA2192IDR` | Dual op-amp (cond. + potentiostat) |
| U_e3 | TI `REF3030AIDBZR` | 3.0 V precision reference |
| R_s | 10 kΩ 0.1 % | Conductivity sense resistor |
| J_pH, J_orp, J_ise | 3× BNC | Electrode inputs |
| Passives | various | Filters, decoupling, gain setting |
| E1 | Microchip `24AA02T-I/OT` | Module ID EEPROM |

---

## 11. Actuator module

### 11.1 Stepper driver

```
                       VCC_12V
                         │
                         │
                    ┌────┴────┐
                    │         │
                    │ A4988   │ Pololu driver breakout
                    │         │
                    │ VMOT    │
                    │         │
                    │ STEP ───┤────── MOD_B_STEP  (DOUT8)
                    │         │
                    │ DIR ────┤────── MOD_B_DIR   (DOUT9)
                    │         │
                    │ EN ─────┤────── MOD_B_EN    (DOUT10)
                    │         │
                    │ SLEEP ──┤─── tied to RESET
                    │ RESET  ─┤─── tied to SLEEP
                    │ MS1/2/3 ┤─── GND (full step)
                    │         │
                    │ 1A, 1B  ├────── to stepper coil A
                    │ 2A, 2B  ├────── to stepper coil B
                    │         │
                    │ VDD ────┤──── VCC_3V3
                    │ GND     ┤──── GND
                    │         │
                    │ VREF ───┤── pot to set current limit
                    │         │
                    └─────────┘
```

### 11.2 DC motor / pump H-bridge (DRV8871)

```
                  VCC_12V
                     │
                     │
                ┌────┴────┐
                │         │
                │ DRV8871 │ TI H-bridge
                │         │
                │ IN1 ────┤──── MOD_B_PUMP_IN1 (DOUT11)
                │ IN2 ────┤──── MOD_B_PUMP_IN2 (DOUT12)
                │         │
                │ OUT1 ───┤────── pump motor +
                │ OUT2 ───┤────── pump motor −
                │         │
                │ ISENSE ─┤── Rsense 0.1 Ω to GND
                │         │
                └─────────┘
```

Duplicate for the stirrer (with different GPIO controls).

### 11.3 Peltier H-bridge (BTS7960)

Same concept but higher current (the BTS7960 handles up to 43 A, plenty
for a 40 W Peltier). Two inputs (`L_PWM`, `R_PWM`) control heating and
cooling direction.

### 11.4 Actuator BOM summary

| Ref | Value / P/N | Function |
|---|---|---|
| U_a1 | Pololu `A4988` breakout | Stepper driver |
| U_a2 | TI `DRV8871DDAR` | Pump H-bridge |
| U_a3 | TI `DRV8871DDAR` | Stirrer H-bridge |
| U_a4 | Infineon `BTS7960` breakout | Peltier H-bridge |
| M1 | NEMA 17 stepper | Turret rotation |
| M2 | 12 V peristaltic pump | Titrant delivery |
| M3 | 5 V DC motor | Magnetic stirrer |
| TEC1 | TEC1-12706 | Peltier element |
| FAN1 | 40 mm 12 V | Peltier hot-side fan |
| NTC_TEC | 10 kΩ NTC | Peltier overtemp sensor |
| J_act_B | 2×10 IDC | Connector to base |
| E2 | Microchip `24AA02T-I/OT` | Module ID EEPROM |

---

## 12. Netlist (base station, minimal build)

This is the complete netlist for the **minimum viable build**: base
station with primary TIA, 8 LED/laser drivers, safety interlock, and
relay. No modules, no secondary TIA (haze port omitted), no
electrochemical or actuator features. ~20 components.

```
# Power
VCC_5V = Arduino_Due.5V, U2.V+, R_led_all.top, K1.contact_in
VCC_3V3 = Arduino_Due.3V3, U2.rail_upper_via_bias, R10.top, LED_READY.anode_via_resistor
GND = Arduino_Due.GND, U2.V-, PD1.anode, Q_all.source, SW1.one_side, LED_READY.cathode

# Mid-rail bias
VBIAS = R3.bottom, R4.top, C10.top, C11.top, U2.in+
(R3: VCC_3V3 → VBIAS, R4: VBIAS → GND, C10 and C11 from VBIAS to GND)

# Primary TIA
PD1.cathode = U2.in-
U2.out = VBIAS_TIA_OUT
R_f = U2.in- ↔ U2.out
C_f = U2.in- ↔ U2.out

# TIA output to ADC
VBIAS_TIA_OUT = Arduino_Due.ADC0
C12 = VBIAS_TIA_OUT ↔ GND (100 pF)

# LED drivers (x7 LEDs)
LED0 (340nm): anode to VCC_5V via R_led0 (82Ω), cathode to Q0.drain
LED1 (430nm): anode to VCC_5V via R_led1 (100Ω), cathode to Q1.drain
LED2 (470nm): anode to VCC_5V via R_led2 (100Ω), cathode to Q2.drain
LED3 (525nm): anode to VCC_5V via R_led3 (100Ω), cathode to Q3.drain
LED4 (590nm): anode to VCC_5V via R_led4 (150Ω), cathode to Q4.drain
LED6 (740nm): anode to VCC_5V via R_led6 (100Ω), cathode to Q6.drain
LED7 (940nm): anode to VCC_5V via R_led7 (100Ω), cathode to Q7.drain
Q0..Q7: source to GND, gate via R_gate (100Ω) to DRV_CHx, pulldown R_gp (10kΩ) from gate to GND

# Gate drive sources
Q0.gate_drive = PWM0       (Arduino_Due pin D6)
Q1.gate_drive = PWM1       (Arduino_Due pin D7)
Q2.gate_drive = DRV_CH2    = DOUT0
Q3.gate_drive = DRV_CH3    = DOUT1
Q4.gate_drive = DRV_CH4    = DOUT2
Q5.gate_drive = DRV_CH5    = DOUT3   (laser modulation)
Q6.gate_drive = DAC0_sq    (from comparator U4 output, comparator input from DAC0)
Q7.gate_drive = DAC1_sq    (from comparator U5 output, comparator input from DAC1)

# Laser (CH5)
LASER_MODULE.pos_in = K1.contact_out (relay normally-open)
LASER_MODULE.neg_out = Q5.drain
K1.coil_hi = VCC_5V
K1.coil_lo = Q2_relay.collector (BC547)
Q2_relay.emitter = GND
Q2_relay.base = LASER_EN (DOUT4) via R11 (1kΩ)
D1 (1N4148) flyback: cathode to VCC_5V, anode to K1.coil_lo
F1 (fuse 500mA) in series on VCC_5V going into K1.contact_in

# Safety interlock
SW1.high_side = GND
SW1.low_side = INTERLOCK (DIN5)
R10 (10kΩ) from INTERLOCK to VCC_3V3

# Status LEDs
LED_READY (green) anode → R_ready (470Ω) → DOUT5, cathode → GND
LED_MEAS (amber) anode → R_meas (470Ω) → DOUT6, cathode → GND
LED_FAULT (red) anode → R_fault (470Ω) → DOUT7, cathode → GND

# Comparators (DAC → square wave for CH6, CH7)
U4 (TLV3201) for CH6 (740nm): +in = VBIAS (1.65V), -in = DAC0, out = DRV_CH6 (to Q6 gate)
U5 (TLV3201) for CH7 (940nm): +in = VBIAS (1.65V), -in = DAC1, out = DRV_CH7 (to Q7 gate)
Both U4, U5: V+ = VCC_3V3, V- = GND, output bypass 100nF to GND

# Self-test loopback
DAC0_LOOPBACK_JUMPER: from Arduino_Due.DAC0 to Arduino_Due.ADC3
(wire jumper that can be physically removed for diagnostic isolation)
```

---

## 13. Test points

The following test points help diagnosis. Each is a single-pin header
or PCB via accessible from outside the enclosure:

| TP | Net | Expected value (blank, all LEDs off) |
|---|---|---|
| TP1 | VCC_5V | 5.00 V ± 0.05 |
| TP2 | VCC_3V3 | 3.30 V ± 0.05 |
| TP3 | VCC_12V | 12.0 V ± 0.3 |
| TP4 | VCC_N12V | −12.0 V ± 0.3 |
| TP5 | VBIAS | 1.65 V ± 0.02 |
| TP6 | VBIAS_TIA_OUT (ADC0) | 1.65 V ± 0.02 (dark offset) |
| TP7 | VBIAS_TIA2_OUT (ADC1) | 1.65 V ± 0.02 |
| TP8 | INTERLOCK (DIN5) | 0 V if cover closed, 3.3 V if open |
| TP9 | LASER_EN (DOUT4) | 0 V until commanded high by host |
| TP10 | VCC_LASER (post-relay, pre-module) | 0 V until DOUT4 high + interlock closed |
| TP11 | DRV_CH5 (DOUT3) | 0 V (or pulsing if active) |
| TP12 | PWM0 output | 0 V (or pulsing if active) |

---

## 14. Assembly notes

### 14.1 Ground strategy

- Use a **star ground** on the base PCB. The TIA's analog ground goes
  to the star. The LED drivers' ground goes to the star through a
  separate trace. The expansion headers' ground pins go to the star.
- Do **not** route the LED driver ground return through the same trace
  as the TIA ground. The LED switching currents (tens of mA, tens of
  kHz) will inject common-mode noise into the TIA input otherwise.
- Ferrite bead FB1 on the 5 V rail provides a further barrier between
  the noisy LED supply and the quiet TIA supply.

### 14.2 Physical layout

- The TIA should be placed **close to the photodiode** — ideally on
  the same PCB inside the optical head — to minimize noise pickup on
  the high-impedance inverting input. Keep this trace < 20 mm.
- The DC-DC converter should be placed away from the TIA. At least 30
  mm clearance, and ideally with a ground pour separating them.
- Route the interlock microswitch wiring as a twisted pair or with a
  short run. A noise pickup here could mask cover-open events.
- The relay K1 and the MOSFETs Q0 – Q7 produce small magnetic fields
  when switching. Keep them away from the photodiode.

### 14.3 Soldering sequence

Recommended order for perfboard assembly:

1. Power: USB-in, DC-DC, bulk caps. Verify rails with a multimeter
   before adding anything else.
2. Interlock: microswitch, pull-up, R10. Verify DIN5 reads correctly.
3. Status LEDs: three LEDs + resistors. Verify with a blink test.
4. Relay: K1 + Q_relay + D1. Verify the relay clicks when DOUT4 is
   toggled.
5. LED drivers (one at a time): start with CH0 (340 nm). Verify LED
   lights when its drive net is tied to 3.3 V manually.
6. TIA: OPA381 + R_f + C_f + bias divider. Verify mid-rail at ADC0 in
   the dark.
7. Comparators for DAC0/DAC1 → CH6/CH7.
8. Laser driver: Q5 + relay contact wiring. Verify interlock gating
   before first power-on.
9. Test points: solder headers on test-point vias.
10. Expansion headers: final step, only if building modules.

### 14.4 First power-on checklist

- [ ] No smoke :)
- [ ] TP1..TP4 at expected values
- [ ] TP5 at 1.65 V
- [ ] TP6 at ~1.65 V (might be a few mV off, that's the op-amp offset)
- [ ] Interlock reads correctly (close and open cover, TP8 flips)
- [ ] Status LEDs controllable from host
- [ ] Each LED driver can be toggled (tie drive net to 3.3 V manually
  or via the host)
- [ ] Relay clicks audibly when DOUT4 is commanded high (with cover
  closed)
- [ ] **Only after all of the above**, with laser safety glasses on,
  run the electronic self-test (see
  [`../operate/FIRST_EXPERIMENT.md`](../operate/FIRST_EXPERIMENT.md) §4.1).

---

*End of hardware schematics. Questions, corrections, or KiCad
transcriptions → issues on the phycommander repository with label
`analyzer:hardware`.*
