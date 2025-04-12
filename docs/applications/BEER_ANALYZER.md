# Multispectral Beer Analyzer — phycommander Application

> An 8-wavelength lock-in multiplexing photometer built on phycommander, designed
> to measure ethanol, color (EBC), haze (EBC), protein, polyphenols, reducing
> sugars, and iron content in beer — using a single shared photodiode and a
> €100 bill of materials.

---

**Document version**: draft 1, 2026-04-11
**Target phycommander version**: v1.1 (PWM + firmware-level interlock)
**Status**: design document, not yet built
**Intended license**: CERN-OHL-S v2
**Publication target**: HardwareX (primary), J. Open Hardware (secondary)

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Principle of operation](#2-principle-of-operation)
3. [Wavelength selection](#3-wavelength-selection)
4. [Hardware design](#4-hardware-design)
5. [Mapping to phycommander I/O](#5-mapping-to-phycommander-io)
6. [Bill of materials](#6-bill-of-materials)
7. [Firmware requirements](#7-firmware-requirements)
8. [Host software](#8-host-software)
9. [Calibration procedures](#9-calibration-procedures)
10. [Experimental protocols](#10-experimental-protocols)
11. [Safety](#11-safety)
12. [Extensions](#12-extensions)
13. [Publication plan & roadmap](#13-publication-plan--roadmap)
14. [Appendices](#14-appendices)

---

## 1. Introduction

### 1.1 What this instrument is

A desktop multispectral photometer + nephelometer that illuminates a standard
1 cm cuvette with up to 8 light sources of different wavelengths — each
modulated at a different frequency — and reads the transmitted (or scattered)
light with a single UV-enhanced silicon photodiode. Eight software lock-in
demodulators run in parallel on the host PC and separate the 8 absorbance (or
scattering) channels with cross-channel leakage below −50 dB after one second
of integration.

The result, after calibration, is a quantitative measurement of all 8
absorbances simultaneously, updated every second, with sub-milliabsorbance
noise and > 60 dB rejection of ambient light, mains hum, and source drift.

### 1.2 What it measures — specification summary

| Parameter | Method | Principal λ | Precision goal | Range |
|---|---|---|---|---|
| **Ethanol** (% v/v) | Alcohol dehydrogenase + NAD⁺ → NADH at 340 nm | 340 nm | ±0.1 % v/v | 0 – 12 % v/v |
| **Color** (EBC) | Direct absorbance at 430 nm, degassed sample | 430 nm | ±0.2 EBC unit | 2 – 50 EBC |
| **Turbidity** (EBC haze) | Scattering at 90°, 650 nm per EBC 9.29 | 650 nm | ±0.5 EBC unit | 0 – 20 EBC |
| **Protein** | Bradford / Coomassie G-250 complex | 590 nm | ±5 % (rel.) | 10 – 1000 mg/L |
| **Polyphenols** | Folin-Ciocalteu molybdenum blue | 740 nm | ±10 % (rel.) | 50 – 2000 mg/L GAE |
| **Reducing sugars** | DNS (dinitrosalicylic acid) | 525 nm | ±5 % (rel.) | 0.2 – 10 g/L |
| **Iron** | o-phenanthroline complex | 525 nm | ±10 % (rel.) | 0.1 – 10 mg/L |
| **Bitterness** (IBU) | ISO-octane extract, A275 (UV-C) | 275 nm | n/a | **not in base build, Fase 2** |

For each parameter the document provides the full protocol (reagents,
dilutions, incubation time, cuvette handling, calibration curve, calculation).
See [§10](#10-experimental-protocols).

### 1.3 Who this is for

- Microbreweries and craft brewers who need routine QC but cannot justify the
  €25 000 – €50 000 cost of a commercial beer analyzer.
- University food-science, biology, and chemistry teaching labs.
- Open-hardware and reproducible-measurement research projects.
- Home brewers who want quantitative ethanol measurement beyond the
  hydrometer / refractometer combo.

### 1.4 What phycommander provides

- **Deterministic 10 kHz sample-coherent DAC ↔ ADC streaming** over USB,
  PREEMPT_RT host. This is what enables software lock-in detection at multiple
  frequencies simultaneously with low phase noise.
- **8 ADC channels, 2 DAC, 2 PWM (v1.1), 16 DIN, 16 DOUT** — enough I/O to
  drive 8 modulated light sources, read the photodiode and auxiliary sensors,
  and gate the laser through a safety interlock.
- **Open firmware and protocol** — every step of the signal chain, from the
  ATSAM3X8E DMA ADC buffer to the Python lock-in on the host, is readable and
  auditable.

### 1.5 How to read this document

Sections 2–3 explain *why* the instrument is designed this way. Sections 4–7
are the hardware reference. Section 8 is the software reference. Sections 9–10
are the operating manual. Section 11 is safety. Sections 12–14 are extensions
and paperwork.

A first-time reader building the instrument should go in this order:

1. Read §1, §2, §3 to understand the concept.
2. Read §11 (safety) before touching any component.
3. Use §6 (BOM) to order parts.
4. Build hardware per §4 and §5.
5. Deploy firmware per §7.
6. Deploy software per §8.
7. Run §9 calibration.
8. Run §10 protocols.

---

## 2. Principle of operation

### 2.1 Absorbance spectroscopy in 60 seconds

Beer-Lambert's law:

```
A(λ) = -log₁₀(I(λ) / I₀(λ)) = ε(λ) · c · l
```

where `A` is absorbance, `I` is transmitted light intensity, `I₀` is
incident light intensity, `ε(λ)` is the molar absorption coefficient of the
absorbing species at wavelength λ, `c` is concentration, and `l` is the
optical path length (1 cm for a standard cuvette). Measure `A`, know `ε` and
`l`, solve for `c`.

Problem: `I` and `I₀` must be measured through the same optical system at the
same moment. Conventional spectrophotometers solve this with (a) a chopper
wheel and a single detector or (b) a beam splitter and two detectors. Both
are mechanical, fragile, and expensive.

### 2.2 Lock-in multiplexing — the key idea

**Modulate each light source at a different frequency**. A single photodiode
sees the sum of all modulated intensities plus ambient light plus detector
noise. On the host, run N parallel **software lock-in demodulators**, each
tuned to one of the modulation frequencies. The k-th lock-in extracts the
amplitude of the k-th source, and the other sources — being at different
frequencies — integrate to zero over a sufficiently long window.

Mathematically, for a received photodiode voltage

```
v(t) = Σₖ Aₖ · sin(2π fₖ t + φₖ) + n(t)
```

the k-th lock-in computes

```
Iₖ = (1/N) Σᵢ v(tᵢ) · sin(2π fₖ tᵢ)
Qₖ = (1/N) Σᵢ v(tᵢ) · cos(2π fₖ tᵢ)
Rₖ = √(Iₖ² + Qₖ²)     (amplitude of channel k)
```

Cross-channel leakage `|R_k(drive_j ≠ k)| / |R_k(drive_k)|` falls below −50 dB
after a 1 s integration provided the modulation frequencies are pairwise
incommensurate (i.e., their differences are large compared to 1 / T_int).

The result: **one detector, one integration, N independent measurements**.
No moving parts. No chopper. No beam splitter.

### 2.3 Why phycommander

The lock-in technique is useless if the modulation and the demodulation
references drift in phase relative to each other. This is why most Arduino
lock-in projects struggle: the USB round-trip jitter destroys phase coherence.

Phycommander solves this because the modulation reference (host phase
accumulator → PWM/DAC/DOUT on the MCU) and the demodulation reference (same
host phase accumulator → numerical sin/cos) are computed *from the same
software variable*. Sample-by-sample coherence is guaranteed. Hardware PWM on
the SAM3X8E is clocked from the same 84 MHz system clock that triggers the
ADC DMA buffer, so even the hardware-generated carriers stay within ~12 ns of
phase alignment with ADC samples.

### 2.4 The ethanol challenge

Ethanol is **optically transparent** from 200 nm to the mid-infrared — it has
no usable absorbance band in the UV-Vis range accessible to silicon
photodiodes and LED sources. Commercial UV-Vis beer analyzers do not measure
ethanol directly; they infer it from density, refractive index, or near-IR
overtone bands (~1100 – 1900 nm) using InGaAs or PbS detectors. None of
those paths is open to phycommander.

The only route to a quantitative optical ethanol measurement using a
silicon-range instrument is **the enzymatic assay with alcohol dehydrogenase
(ADH) and NAD⁺**:

```
CH₃CH₂OH + NAD⁺   ──ADH──▶   CH₃CHO + NADH + H⁺
```

NADH absorbs strongly at 340 nm (ε = 6220 M⁻¹ cm⁻¹) while NAD⁺ does not.
Reaction performed in pyrophosphate buffer at pH 8.8 with acetaldehyde trap,
the equilibrium is driven fully toward NADH production, and the absorbance
increase at 340 nm over the baseline is stoichiometrically proportional to
the initial ethanol concentration. The reaction reaches plateau in 5–10
minutes. This is **the** standard ethanol assay in food / clinical /
industrial labs, accurate to ±0.1 % v/v after calibration.

The enzymatic assay is **slow** (minutes) and requires **reagents**, but it
gives commercial-grade quantitative accuracy on a €100 instrument. Sections
§6.3 and §10.1 cover the reagent options and the protocol in detail.

---

## 3. Wavelength selection

### 3.1 The eight channels

| Ch. | λ (nm) | Source type | Primary use |
|---|---|---|---|
| **CH0** | 340 | UV LED (Bivar UV5TZ-340 or Vishay VLMU3100-GS08) | NADH (ethanol via ADH, glucose via hexokinase), other enzymatic assays |
| **CH1** | 430 | Violet LED (Kingbright L-7113PBC or Cree C503B-BAS) | Beer color (EBC official method: A430 × 25 = EBC) |
| **CH2** | 470 | Blue LED (Kingbright L-7113PBC-J) | General blue, carotenoids, chlorophyll b |
| **CH3** | 525 | Green LED (Kingbright L-7113GC) | Bradford protein (secondary), iron/phenanthroline, DNS sugars |
| **CH4** | 590 | Amber LED (Kingbright L-7113SYC) | Bradford protein (595 nm primary, 590 close enough) |
| **CH5** | **650** | **5 V laser diode module** (user-supplied, KY-008 style) | **EBC haze** (official turbidity method, 90° scattering at 650 nm), methylene blue, chlorophyll a |
| **CH6** | 740 | Deep red LED (Vishay VSLY5940 or OSRAM SFH 4356) | Polyphenols (Folin-Ciocalteu, λ_max 765 nm — 740 nm gives ~80 % sensitivity, acceptable) |
| **CH7** | 940 | NIR LED (Vishay TSAL6100 or similar) | Haze reference: no common chromophore absorbs at 940 nm, so any signal here is pure scattering. Used for haze/absorbance separation |

**Total LEDs**: 7. **Plus**: 1 red laser diode (5 V, 650 nm, from the user's
existing stock).

### 3.2 Why 340 nm and not 365 nm (default choice)

NADH's absorption peak is at 340 nm with ε = 6220 M⁻¹ cm⁻¹. At 365 nm the
extinction coefficient drops to ≈ 3400 M⁻¹ cm⁻¹ (about 55 % of peak). A 340
nm UV LED gives 1.8× better sensitivity for all NADH-based assays (ethanol,
glucose, aldehydes) compared to a 365 nm LED of the same optical power.

- **340 nm LEDs**: Bivar UV5TZ-340, Vishay VLMU3100-GS08, Nichia NCSU234B.
  Cost ~€8 – €15 each, ~1000 h lifetime, ~20 mW optical output.
- **365 nm LEDs**: Nichia NSPU510CS, Kingbright L-7113UVC. Cost ~€3, ~3000 h
  lifetime, ~30 mW optical output.

**Default for this document: 340 nm** for NADH sensitivity. If you prefer to
save €5–10 and accept ~40 % less ethanol precision (→ ~±0.3 % v/v instead of
±0.1 % v/v), substitute a 365 nm LED on CH0 with no other hardware change.

### 3.3 Modulation frequencies

The 8 carriers must be pairwise incommensurate and all within [fmin, fmax].
For Fs = 10 kHz and N = 10 000 samples (1 s integration), the constraint is:

- fmin > 200 Hz (below this the integration window contains too few cycles)
- fmax < 2500 Hz (above Fs/4, waveform fidelity degrades for square waves
  generated by software DOUT toggling; hardware PWM channels can go up to
  ~Fs/2 = 5 kHz cleanly)

Chosen frequencies (all in Hz):

| Channel | λ (nm) | f (Hz) | Carrier source |
|---|---|---|---|
| CH0 | 340 | 1013 | PWM0 (hardware) |
| CH1 | 430 | 1709 | PWM1 (hardware) |
| CH2 | 470 | 571 | DOUT0 (software toggle) |
| CH3 | 525 | 881 | DOUT1 (software toggle) |
| CH4 | 590 | 1223 | DOUT2 (software toggle) |
| CH5 | 650 (laser) | 1439 | DOUT3 (software toggle, gated by DIN5 interlock) |
| CH6 | 740 | 1931 | DAC0 → external comparator → gate |
| CH7 | 940 | 2311 | DAC1 → external comparator → gate |

All frequencies chosen so that no `|fᵢ − fⱼ|` or `|fᵢ + fⱼ|` falls within
±5 Hz of any other frequency or its harmonics up to the 3rd order. See
[§14 Appendix A](#appendix-a-modulation-frequency-selection) for the
verification.

---

## 4. Hardware design

### 4.1 System block diagram

```
                        ┌─────────────────────────────────────────┐
                        │           OPTICAL ENCLOSURE             │
                        │              (black ABS)                │
                        │                                         │
                        │   [7 LEDs + 1 laser] → illum. ring      │
                        │             │                           │
                        │             ▼                           │
                        │     [CUVETTE HOLDER 1 cm]               │
                        │             │          └─── 90° port ─┐ │
                        │             ▼                          │ │
                        │     [photodiode 0°]                    │ │
                        │             │              [photo-     │ │
                        │             │               diode 90°] │ │
                        │             │                (haze)    │ │
                        │             │                   │      │ │
                        │             ▼                   ▼      │ │
                        │         TIA 0°                TIA 90°  │ │
                        │             │                   │      │ │
                        └─────────────┼───────────────────┼──────┘ │
                                      │                   │         │
                                     ADC0               ADC1        │
                                                                    │
  ┌─────────────────────────────────────────────────────────────┐   │
  │                  PHYCOMMANDER (Arduino Due)                 │   │
  │                                                             │   │
  │   PWM0 (340 nm)  ──┐                                        │   │
  │   PWM1 (430 nm)  ──┤                                        │   │
  │   DAC0 (740 nm)  ──┤                                        │   │
  │   DAC1 (940 nm)  ──┤── 8 MOSFET driver board ── LEDs+laser ─┘   │
  │   DOUT0 (470)    ──┤                                            │
  │   DOUT1 (525)    ──┤                                            │
  │   DOUT2 (590)    ──┤                                            │
  │   DOUT3 (650) ───┐ │                                            │
  │                  │ │                                            │
  │   ADC0 ◄─── TIA 0° (transmittance)                              │
  │   ADC1 ◄─── TIA 90° (scatter / haze)                            │
  │   ADC2 ◄─── NTC on cuvette (temp comp)                          │
  │   ADC3 ◄─── DAC0 loopback (self-test)                           │
  │                  │                                              │
  │   DIN5 ◄─────────┤── [cover microswitch]                        │
  │   DOUT4 ─────────┘── [laser master enable MOSFET]               │
  │                    (hardware-gated by DIN5 in firmware)         │
  └─────────────────────────────────────────────────────────────────┘
                      │
                     USB (10 kHz stream)
                      │
                      ▼
  ┌─────────────────────────────────────────────────────────────┐
  │                    HOST PC (Linux PREEMPT_RT)               │
  │   physerver   →   lockin_engine   →   assay_runner   →  UI  │
  │   (Rust, RT)      (Python, 8× LI)      (Python)        (Py) │
  └─────────────────────────────────────────────────────────────┘
```

### 4.2 Optical layout

**Illumination geometry**: a ring of 8 sources (7 LEDs + 1 laser) aimed at
the center of the cuvette from the same side. The LEDs are at the corners of
an octagon with ~15 mm radius, pointing inward at ~15° from the cuvette's
main optical axis. The central aperture on the far side of the cuvette is a
3 mm hole in front of the transmittance photodiode (0° port).

**Haze port**: a second 3 mm hole at 90° with a second photodiode + TIA. Both
TIAs share the same analog front-end design; only the mechanical position
differs. (Haze port is optional for the base build — if omitted, the 650 nm
laser is used in trans­mittance mode only and haze is estimated from the
difference between 650 nm and 940 nm absorbances.)

**Integrating element**: immediately before the transmittance photodiode,
insert a 3 mm × 10 mm PTFE (Teflon) rod as a Lambertian diffuser / optical
mixer. This scrambles the angular distribution of the different LED beams so
that the photodiode sees a uniform illumination regardless of which LED is
currently driven. **This is crucial**: without the PTFE mixer, different LEDs
will hit the photodiode at different angles and you'll see cross-talk between
LEDs and spatial non-uniformity in the sample.

**Cuvette holder**: standard 10 mm × 10 mm × 45 mm cuvette. 3D-printable STL
in `docs/applications/beer_analyzer/cuvette_holder.stl` (to be added).
Inserts from the top; o-ring for light-tight closure; spring-loaded retention.

### 4.3 Transimpedance amplifier (TIA)

```
                                R_f (10 MΩ)
                           ┌────/\/\/\────┐
                           │              │
                           │    C_f (1 pF)│
                           ├─────||───────┤
                           │              │
           BPW34/S1227     │              │
             cathode ──────┤              │
              │            │    ──┐       │
              │            └───┐  │       │
              ├─── (reverse     ├─┼──▶ OPA381 ──────── ADC0
              │     bias        │  │     │           (phycommander)
              │     optional)   ├─┘       │
              │    (+5V thru 1M)│         │
              │                 │         │
             anode ──────┬──────┘         │
                         │                │
                         │                │
                         └── 1.65 V ──────┘
                         (mid-rail bias via
                          resistor divider
                          + bypass cap)
```

**Design choices**:

- **Photodiode**: Hamamatsu S1227-1010BR (10 × 10 mm active area,
  UV-enhanced, 200 – 1100 nm spectral range, low dark current, NEP ≈ 2 × 10⁻¹⁴
  W/√Hz). Cost: ~€30. Strongly recommended over BPW34 (which is cheaper at
  ~€1 but has poor UV response < 450 nm and 10× smaller area).
- **Op-amp**: Texas Instruments OPA381. Single-supply 3.3 – 36 V,
  transimpedance-optimized (low input bias current ≈ 50 fA, gain-bandwidth
  ~18 MHz, low voltage noise 8 nV/√Hz). Cost: ~€5. Alternatives: LMP7721
  (even lower bias current, €7), OPA128 (research-grade, €30). **Do not use
  TL072 or NE5532** — their bias currents are far too high for a 10 MΩ TIA
  and will dominate the dark signal.
- **Feedback resistor R_f = 10 MΩ**: sets the transimpedance gain. For a
  signal photocurrent of 1 nA (typical for weakly absorbing sample at low LED
  power), output is 10 mV, giving ~2 % of ADC full-scale. Tune between 1 MΩ
  and 100 MΩ during calibration: start at 10 MΩ and adjust up if the noise
  floor dominates, or down if strong samples saturate.
- **Feedback capacitor C_f = 1 pF**: compensates the TIA pole from photodiode
  capacitance (3.5 pF for BPW34, 50 pF for S1227) + op-amp input capacitance
  (~7 pF). Formula: `C_f ≈ √(C_in / (2π R_f GBW))`. For S1227 on OPA381:
  `C_f ≈ √(60 pF / (2π × 10 MΩ × 18 MHz)) ≈ 0.7 pF`. Round up to 1 pF.
- **Mid-rail bias**: 1.65 V bias on the op-amp's non-inverting input centers
  the output in the 0 – 3.3 V ADC range, allowing bipolar photocurrent swing
  (though in practice only one polarity is used for absorbance measurements).
  Resistor divider: 100 kΩ / 100 kΩ from 3.3 V to GND, 10 µF bypass cap.
- **Photodiode reverse bias (optional)**: for fastest response and lowest
  capacitance, reverse-bias the photodiode at ~5 V through a 1 MΩ resistor.
  Improves bandwidth but adds a small dark current. Not strictly needed at 1
  kHz modulation. Omit for simplicity in v1.

### 4.4 LED driver (per channel, × 7 LEDs)

Each LED is switched to ground through a logic-level N-channel MOSFET:

```
                  +5 V (or LED-specific rail)
                    │
                    │
                   ─┼─── LED (anode)
                    │
                    │
                   ─┴─ LED (cathode)
                    │
                    │
                   R_s (current-limit,
                   computed per LED)
                    │
                    │
                    ├──── Drain
                    │
                    │         ┌──── Source ──── GND
                    │         │
                    └─── MOSFET (2N7000 / IRLML2502)
                                │
                               Gate
                                │
                                └─── Gate resistor 100 Ω ── phycommander DOUT/PWM
```

**MOSFET choice**: 2N7000 (TO-92 through-hole, cheap) or IRLML2502 (SOT-23
SMD, smaller). Both are logic-level (V_GS(th) ~ 1.5 V), handle 200 mA
continuous, and switch in < 100 ns. Gate resistor 100 Ω damps ringing.

**Current limit resistor R_s**: sized per LED based on its forward voltage V_f
and desired forward current I_f. For a 5 V rail:

```
R_s = (5 V - V_f - V_DS(MOSFET)) / I_f
```

Typical values (assume V_DS ≈ 0.1 V at 20 mA):

| λ (nm) | V_f (V) | I_f (mA) | R_s (Ω) | Power (mW) |
|---|---|---|---|---|
| 340 | 3.4 | 20 | 75 | 1.5 |
| 430 | 3.2 | 20 | 85 | 1.7 |
| 470 | 3.2 | 20 | 85 | 1.7 |
| 525 | 3.0 | 20 | 95 | 1.9 |
| 590 | 2.0 | 20 | 145 | 2.9 |
| 740 | 1.9 | 30 | 100 | 3.0 |
| 940 | 1.3 | 40 | 90 | 3.6 |

Use standard E12 values (68, 82, 100, 120, 150 Ω).

### 4.5 Laser driver (CH5, the 5 V 650 nm module)

The 5 V laser module is treated the same way as the LEDs, with one critical
difference: the **master enable** is hardware-gated by the safety interlock.

```
  +5 V ── Fuse(500 mA) ── [Laser Master Enable MOSFET] ── Laser module (+)
                                       │
                                      Gate
                                       │
                                       └── DOUT4 (via firmware AND DIN5)

  Laser module (−) ── [Modulation MOSFET] ── GND
                             │
                            Gate
                             │
                             └── DOUT3 (modulation carrier)
```

Two MOSFETs in series with the laser: one for modulation (DOUT3) and one for
master enable (DOUT4). **Both must be ON for the laser to emit.** The master
enable is *forced low in phycommander firmware* whenever DIN5 (the cover
interlock) reads "cover open". See [§11](#11-safety) for the full safety
story.

### 4.6 Safety interlock circuit

```
        +3.3 V
          │
        10 kΩ pull-up
          │
          ├────── DIN5 (phycommander)
          │
          │    ┌──── normally open microswitch ──── GND
          └────┤
               │ closed only when enclosure cover is closed
               └────
```

Closed cover → DIN5 reads logic 0.
Open cover → DIN5 reads logic 1 (pulled up).

**Firmware logic** (to be added in v1.1):

```
on every loop iteration:
    if DIN5 == 1 (cover open):
        force DOUT4 = 0  (disable laser master)
        force DOUT3 = 0  (disable laser modulation)
        set status flag "INTERLOCK_OPEN"
    else:
        allow host to drive DOUT4 and DOUT3 normally
```

This logic lives on the ATSAM3X8E, not on the host. If the host crashes or
the USB cable is unplugged, DOUT4 and DOUT3 are still forced low by the
firmware as long as the cover is open. **This is mandatory** for a Class 3R
laser.

### 4.7 Enclosure

Target: a light-tight, vibration-damped box with controlled internal geometry.

- **Material**: 3D-printed ABS or PETG, black filament. Matt black interior.
  No glossy surfaces.
- **Dimensions**: approximately 150 mm × 120 mm × 80 mm (external).
- **Cuvette access**: hinged top lid with the cover interlock microswitch.
- **Ports**: cutouts for phycommander cable, 5 V power input.
- **Mounting**: LEDs + photodiodes on a small PCB inside, rigidly mounted.
- **Thermal**: LEDs dissipate ~20 mW each; no cooling needed. The TIA op-amp
  is the most temperature-sensitive component; mount it away from the LEDs.

STL / STEP / KiCad files to be committed under
`docs/applications/beer_analyzer/hardware/`.

### 4.8 Power supply

- Phycommander + LEDs: 5 V via USB (sufficient at ~200 mA aggregate).
- TIA rail: 3.3 V from phycommander's onboard regulator (clean, low-noise).
- **Do not** share the LED ground return with the TIA ground on the PCB —
  use a star ground with the TIA ground connected directly to the phycommander
  analog ground pin. Otherwise the LED switching currents will couple into
  the TIA as common-mode noise.

---

## 5. Mapping to phycommander I/O

### 5.1 Pin assignment

| phycommander ch. | Direction | Function | Notes |
|---|---|---|---|
| **PWM0** | out | Modulation carrier for 340 nm LED | 1013 Hz, 50 % duty, hardware-generated |
| **PWM1** | out | Modulation carrier for 430 nm LED | 1709 Hz, 50 % duty, hardware-generated |
| **DAC0** | out | Sine @ 1931 Hz → external comparator → gate of 740 nm LED driver | Software-generated each loop |
| **DAC1** | out | Sine @ 2311 Hz → external comparator → gate of 940 nm LED driver | Software-generated each loop |
| **ADC0** | in | Transmittance photodiode TIA output | Primary signal, all 8 lock-ins |
| **ADC1** | in | Haze photodiode TIA output (90° port) | Optional, haze channel only |
| **ADC2** | in | NTC thermistor on cuvette holder | Temperature compensation |
| **ADC3** | in | DAC0 loopback | Self-test of analog output |
| **ADC4–7** | in | reserved | Future expansion (multi-angle nephelometer, second TIA gain range) |
| **DOUT0** | out | Software-toggled carrier 571 Hz → gate of 470 nm LED | Software toggles each loop iteration |
| **DOUT1** | out | Software-toggled carrier 881 Hz → gate of 525 nm LED | Software toggles each loop iteration |
| **DOUT2** | out | Software-toggled carrier 1223 Hz → gate of 590 nm LED | Software toggles each loop iteration |
| **DOUT3** | out | Software-toggled carrier 1439 Hz → gate of 650 nm **LASER** | Hardware-gated by firmware via DIN5 |
| **DOUT4** | out | Laser master enable | Hardware-gated by firmware via DIN5 |
| **DOUT5** | out | Status LED green (system ready) | UX |
| **DOUT6** | out | Status LED amber (measurement in progress) | UX |
| **DOUT7** | out | Status LED red (fault / ADC saturated) | UX |
| **DOUT8–15** | out | reserved | Expansion |
| **DIN5** | in | Cover interlock microswitch | Safety-critical |
| **DIN0–4, DIN6–15** | in | reserved | Expansion (multi-cuvette turret, user buttons, external trigger) |

### 5.2 Channel count check

- **DAC**: 2/2 used.
- **PWM**: 2/2 used.
- **ADC**: 4/8 used (4 reserved for expansion).
- **DOUT**: 8/16 used (8 reserved).
- **DIN**: 1/16 used (15 reserved).

All 4 "analog-generation-equivalent" channels (DAC×2 + PWM×2) are used for
the 4 sources that need the cleanest carriers (UV, EBC color, polyphenols,
NIR haze reference). The 4 software-toggled DOUT channels serve the less
demanding carriers (visible LEDs with plenty of signal).

---

## 6. Bill of materials

### 6.1 Electronics

| Item | Part number | Supplier | Qty | Unit (€) | Total (€) |
|---|---|---|---|---|---|
| Photodiode UV-enhanced, 10×10 mm | Hamamatsu S1227-1010BR | Mouser / Digikey | 1 | 30.00 | 30.00 |
| TIA op-amp | TI OPA381AIDBVR | Mouser / Digikey | 1 | 5.50 | 5.50 |
| R_f, 10 MΩ, 1 %, 0.1 W | Vishay MRS25 | Mouser | 1 | 0.30 | 0.30 |
| C_f, 1 pF, NP0/C0G | Murata GRM1555C1H1R0 | Mouser | 1 | 0.10 | 0.10 |
| Bias divider, 100 kΩ ×2, 10 µF | Any | — | 1 set | 0.50 | 0.50 |
| Comparator (for DAC0, DAC1 → gate) | TI TLV3201 | Mouser | 2 | 2.00 | 4.00 |
| MOSFET logic-level (LED + laser drivers) | Vishay 2N7000 | Mouser / RS | 9 | 0.30 | 2.70 |
| Gate resistor 100 Ω (1/8 W) | Any | — | 9 | 0.02 | 0.18 |
| LED current limit resistors (per §4.4) | Any | — | 7 set | 0.05 | 0.35 |
| LED 340 nm | **Bivar UV5TZ-340** (default) | Mouser | 1 | 8.00 | 8.00 |
| LED 430 nm violet | Kingbright L-7113PBC-B-U | TME / RS | 1 | 1.00 | 1.00 |
| LED 470 nm blue | Kingbright L-7113PBC-J | TME / RS | 1 | 0.50 | 0.50 |
| LED 525 nm green | Kingbright L-7113GC | TME / RS | 1 | 0.50 | 0.50 |
| LED 590 nm amber | Kingbright L-7113SYC | TME / RS | 1 | 0.50 | 0.50 |
| **Laser 650 nm 5 V** | user-supplied (KY-008 style) | — | 1 | 0 | **0** |
| LED 740 nm deep red | Vishay VSLY5940 | Mouser / TME | 1 | 0.80 | 0.80 |
| LED 940 nm NIR | Vishay TSAL6100 | Mouser / TME | 1 | 0.50 | 0.50 |
| NTC 10 kΩ (temp sensor) | Murata NTCLE100 | Mouser | 1 | 0.30 | 0.30 |
| Cover interlock microswitch | Omron D2F-01L | RS / TME | 1 | 1.50 | 1.50 |
| Status LEDs (3 × 5 mm) | Any | — | 3 | 0.10 | 0.30 |
| Status LED resistors (3 × 470 Ω) | Any | — | 3 | 0.02 | 0.06 |
| Protoboard or custom PCB | — | — | 1 | 5.00 | 5.00 |
| Headers, sockets, wire, solder | — | — | 1 lot | 5.00 | 5.00 |
| **Electronics subtotal** | | | | | **€67.09** |

### 6.2 Optics / mechanics

| Item | Spec | Supplier | Qty | Unit (€) | Total (€) |
|---|---|---|---|---|---|
| Cuvettes, standard plastic 1 cm, 100 pack | PMMA or PS, visible-only | Amazon / Aliexpress | 1 | 5.00 | 5.00 |
| Cuvettes, UV-transparent PMMA-UV, 10 pack | transparent to ~300 nm, required for 340 nm CH0 | BrandTech / Sigma | 1 | 15.00 | 15.00 |
| PTFE rod, 3 mm × 30 mm (optical mixer) | natural PTFE | RS / Cowie Tech | 1 | 3.00 | 3.00 |
| 3D-printed enclosure (filament) | black PETG or ABS, matt | — | 1 | 5.00 | 5.00 |
| Fasteners, hinges, o-ring | — | — | 1 lot | 5.00 | 5.00 |
| **Optics/mechanics subtotal** | | | | | **€33.00** |

### 6.3 Reagents (per-assay kits)

| Item | Purpose | Supplier | Cost (€) | # assays |
|---|---|---|---|---|
| **Megazyme K-ETOH Ethanol Kit** (recommended default) | ADH + NAD⁺ + buffers for ~100 ethanol assays | Megazyme | 120 | 100 |
| — *alternative*: DIY from Sigma (ADH A3263 100 mg + NAD⁺ N7004 500 mg + Tris-PPi buffer) | Same | Sigma-Aldrich | 85 | ~500 (scale-dependent) |
| Bradford reagent, 1 × 500 mL | Protein assay | Bio-Rad Cat. 500-0006 | 40 | ~300 |
| Bovine Serum Albumin (BSA) for protein standard curve | Calibration | Sigma A7906 | 20 | — |
| Folin-Ciocalteu reagent, 2 N, 500 mL | Polyphenol assay | Sigma F9252 | 35 | ~500 |
| Na₂CO₃ anhydrous, 500 g | Buffering for Folin-C | Any chem supplier | 5 | ∞ |
| Gallic acid monohydrate, 10 g | Polyphenol standard | Sigma G7384 | 15 | — |
| Dinitrosalicylic acid (DNS), 100 g | Reducing sugar assay | Sigma D0550 | 25 | ~500 |
| D-Glucose standard, 100 g | Sugar calibration | Any | 5 | — |
| o-Phenanthroline monohydrate | Iron assay | Sigma 131377 | 15 | ~500 |
| Formazin 4000 NTU | Turbidity standard | Hach Cat. 2461100 | 25 | — |
| Ethanol absolute (analytical grade, 500 mL) | Calibration | Sigma | 15 | — |
| Reagent-grade water, 2 L | Blanks | — | 5 | — |
| **Reagents subtotal (with Megazyme K-ETOH)** | | | | **€325** |
| **Reagents subtotal (with DIY ADH/NAD⁺)** | | | | **€290** |

### 6.4 Total cost summary

| Tier | Electronics + optics | Reagents | **Total** | Good for |
|---|---|---|---|---|
| **Minimum electronic build** (BPW34, no UV cuvettes, no 340 nm) | ~€50 | ~€50 (Bradford + Folin only) | **~€100** | visible-only chemistry, no ethanol |
| **Beer analyzer default** (S1227 + 340 nm UV LED + UV cuvettes + Megazyme K-ETOH) | ~€100 | ~€325 | **~€425** | full 8-parameter beer analysis, 100 ethanol assays included |
| **Beer analyzer DIY reagents** (S1227 + DIY ADH/NAD⁺) | ~€100 | ~€290 | **~€390** | same, cheaper scaling for ~500 ethanol assays |
| **Premium** (+ custom PCB, better cuvettes, spare parts) | ~€200 | ~€325 | **~€525** | production-quality build |

**Cost per full beer analysis after first 100 samples**: approximately €2 –
€3 per sample (reagents only), assuming the hardware amortizes across
several hundred measurements.

---

## 7. Firmware requirements

Phycommander firmware v1.0 is sufficient for a non-laser version of this
instrument (LEDs only) running entirely from software-toggled DOUT channels
and the DAC channels for the cleanest carriers. For the laser-based EBC haze
channel (CH5), v1.1 is required, with three additions:

### 7.1 Hardware PWM on PWM0 and PWM1

- Frequency programmable from host, range 100 Hz – 50 kHz.
- Duty cycle programmable, default 50 %.
- Independently enable/disable.
- Clocked from the SAM3X8E system clock (84 MHz), derived via TC peripheral
  or PWM peripheral as preferred.
- Expose as protocol fields `pwm[0].freq_hz`, `pwm[0].duty_u16`,
  `pwm[0].enable`, same for index 1.

### 7.2 Firmware-level safety interlock for the laser

- Monitor DIN5 on every main loop iteration (1 kHz or higher).
- When DIN5 reads the "cover open" state, unconditionally force DOUT4 and
  DOUT3 to logic 0, *independently of any host command*.
- When DIN5 reads "cover closed", allow normal host control of DOUT4/DOUT3.
- Report the interlock state in the status packet (new bit in the flags
  field).

### 7.3 Loop-synchronous DOUT toggling

- When host writes a command packet, the 16-bit `digital_out` field must be
  latched to the DOUT pins at the *same instant* the ADC sample is taken.
  This is already the case in v1.0 (DMA-synchronous) but must be explicitly
  validated for the multi-channel software-toggle use case.
- No change expected; mentioned here for completeness.

### 7.4 Optional: temperature readout from NTC

- NTC wired to ADC2 (or another unused analog input) through a pull-up divider.
- Converted to temperature in °C on the host side (Steinhart-Hart or a
  lookup table). No firmware change needed beyond reading the ADC channel
  normally.

---

## 8. Host software

### 8.1 Architecture

Four layers, all on the host PC:

```
  [UI layer]   TUI or web, ratatui/flask/warp, not on the RT loop
       │
       ▼
  [assay_runner.py]   Per-assay state machines (blank, calibrate, measure, report)
       │
       ▼
  [lockin_engine.py]   8 parallel lock-in demodulators, 10 kHz consume rate
       │
       ▼
  [physerver IPC]   shared memory ring buffer, 64-byte frames, 10 kHz
       │
       ▼
     USB → phycommander → ATSAM3X8E
```

Python is the primary implementation language — the user prefers it, the
instrument's duty cycle is ~1 second per measurement (not microseconds), and
the numerical work is dominated by numpy/scipy operations that are already
C-speed. If the 10 kHz hot loop turns out to be marginal, the consumer side
can be moved to Rust while keeping the assay logic in Python.

### 8.2 Multi-lock-in demodulator (illustrative)

```python
# lockin_engine.py — core of the multi-channel demodulator
import numpy as np
from dataclasses import dataclass
from typing import List

FS = 10_000.0  # Hz, phycommander USB loop rate

@dataclass
class Channel:
    name: str
    freq_hz: float
    gate_index: int  # which phycommander output drives this LED/laser
    gate_kind: str   # 'pwm', 'dac_compare', or 'dout_toggle'

CHANNELS = [
    Channel('340nm', 1013.0, 0, 'pwm'),
    Channel('430nm', 1709.0, 1, 'pwm'),
    Channel('470nm',  571.0, 0, 'dout_toggle'),
    Channel('525nm',  881.0, 1, 'dout_toggle'),
    Channel('590nm', 1223.0, 2, 'dout_toggle'),
    Channel('650nm_laser', 1439.0, 3, 'dout_toggle'),  # gated by DIN5
    Channel('740nm', 1931.0, 0, 'dac_compare'),
    Channel('940nm', 2311.0, 1, 'dac_compare'),
]

class LockinBank:
    """Eight parallel software lock-ins sharing one ADC stream."""
    def __init__(self, channels: List[Channel], window_s: float = 1.0):
        self.channels = channels
        self.N = int(window_s * FS)
        self.phase = np.zeros(len(channels))
        self.I = np.zeros(len(channels))
        self.Q = np.zeros(len(channels))
        self.k = 0
        self.phi_offset = np.zeros(len(channels))  # filled at calibration

    def step(self, adc_sample: int) -> dict | None:
        """Consume one ADC sample. Return (R, phi) dict every window_s."""
        s = np.sin(2 * np.pi * self.phase)
        c = np.cos(2 * np.pi * self.phase)
        self.I += adc_sample * s
        self.Q += adc_sample * c
        self.phase += np.array([ch.freq_hz / FS for ch in self.channels])
        self.phase %= 1.0
        self.k += 1
        if self.k >= self.N:
            R = np.sqrt(self.I**2 + self.Q**2) / self.N
            phi = np.arctan2(self.Q, self.I) - self.phi_offset
            result = {
                ch.name: {'R': float(R[i]), 'phi': float(phi[i])}
                for i, ch in enumerate(self.channels)
            }
            self.I[:] = 0.0
            self.Q[:] = 0.0
            self.k = 0
            return result
        return None

    def drive_outputs(self, k: int) -> tuple[int, dict]:
        """Return (digital_out_bitmask, pwm_config) for loop iteration k."""
        dout = 0
        for i, ch in enumerate(self.channels):
            if ch.gate_kind == 'dout_toggle':
                phase_k = (k * ch.freq_hz / FS) % 1.0
                if phase_k < 0.5:
                    dout |= (1 << ch.gate_index)
        # DAC/PWM outputs configured once at startup, not per-iteration
        return dout, {}
```

### 8.3 Assay state machine (illustrative)

```python
# assay_runner.py — per-assay protocol as a state machine
from enum import Enum

class AssayState(Enum):
    IDLE = 0
    BLANK = 1
    SAMPLE = 2
    REACTION = 3  # for kinetic assays (e.g., ethanol)
    DONE = 4
    ERROR = 5

class EthanolAssay:
    """ADH enzymatic assay, kinetic, plateau at ~5 min."""
    def __init__(self, bank: LockinBank):
        self.bank = bank
        self.state = AssayState.IDLE
        self.baseline_A340 = None
        self.trace = []  # list of (t, A340) for plateau detection

    def on_blank(self, R_340: float):
        # R_340 is the amplitude at 340 nm with water in cuvette
        self.I0_340 = R_340
        self.state = AssayState.SAMPLE

    def on_sample(self, R_340: float, t: float):
        A = -np.log10(R_340 / self.I0_340)
        self.baseline_A340 = A
        self.state = AssayState.REACTION
        self.trace = [(t, A)]

    def on_reaction_step(self, R_340: float, t: float):
        A = -np.log10(R_340 / self.I0_340)
        self.trace.append((t, A))
        if self._is_plateau():
            self.state = AssayState.DONE
            return self._compute_ethanol()
        return None

    def _is_plateau(self) -> bool:
        """Detected when last 60 s of A340 have slope < 0.001 OD/min."""
        if len(self.trace) < 60:
            return False
        recent = self.trace[-60:]
        t0, A0 = recent[0]
        t1, A1 = recent[-1]
        slope = (A1 - A0) / ((t1 - t0) / 60.0)  # OD per minute
        return abs(slope) < 0.001

    def _compute_ethanol(self) -> float:
        """Return ethanol in % v/v."""
        A_plateau = self.trace[-1][1]
        delta_A = A_plateau - self.baseline_A340
        # NADH: epsilon = 6220 M^-1 cm^-1 at 340 nm, l = 1 cm
        c_NADH_mM = (delta_A / 6.220)
        # Stoichiometry: 1 mol EtOH → 1 mol NADH
        # Apply dilution factor (typically 1:1000 for beer)
        DILUTION = 1000
        c_EtOH_mM_in_beer = c_NADH_mM * DILUTION
        # MW(EtOH) = 46.07 g/mol; density = 0.789 g/mL
        g_per_L = c_EtOH_mM_in_beer * 46.07 / 1000
        mL_per_L = g_per_L / 0.789
        percent_v_v = mL_per_L / 10
        return percent_v_v
```

### 8.4 Live plot

A matplotlib live plot window with:

- one subplot per active channel (absorbance vs time)
- a kinetic overlay for the ethanol assay showing the trace and detected
  plateau
- a bottom status bar with: wall-clock time, interlock state, ADC saturation
  indicator, current assay state

`matplotlib.animation.FuncAnimation` at 1 Hz is sufficient (the lock-in
emits one result per second).

### 8.5 Reporting

At the end of each sample, `assay_runner` produces:

- A CSV row appended to `~/.phycommander/beer_analyzer/results.csv` with
  columns: timestamp, sample_id, ethanol_%, color_EBC, haze_EBC, protein_mg_L,
  polyphenols_mg_L, sugars_g_L, notes.
- A PDF report (via `reportlab`) with the 8 channel traces, the calibration
  curves used, the plateau detection for ethanol, and a signed hash of the
  raw data file. Path: `~/.phycommander/beer_analyzer/reports/`.

### 8.6 File layout under `phyclient/examples/beer_analyzer/`

```
phyclient/examples/beer_analyzer/
├── README.md
├── beer_analyzer.py       # main entry point
├── lockin_engine.py       # §8.2
├── assay_runner.py        # §8.3
├── assays/
│   ├── __init__.py
│   ├── ethanol.py         # EthanolAssay
│   ├── color_ebc.py       # ColorEBCAssay
│   ├── haze_ebc.py        # HazeEBCAssay
│   ├── bradford.py        # BradfordProteinAssay
│   ├── folin.py           # FolinPolyphenolAssay
│   ├── dns_sugars.py      # DNSReducingSugarsAssay
│   └── iron.py            # IronAssay
├── calibration/
│   ├── __init__.py
│   ├── self_test.py       # DAC→ADC loopback
│   ├── dark_noise.py      # noise floor
│   └── linearity.py       # Beer-Lambert verification
├── plotting/
│   └── live_plot.py
├── reporting/
│   └── pdf_report.py
├── config.toml            # wavelengths, frequencies, gains, calibrations
└── tests/
    └── test_lockin_math.py
```

---

## 9. Calibration procedures

Calibration is done in five stages, in order. Each stage must pass before the
next is attempted.

### 9.1 Electronic self-test (DAC → ADC loopback)

**Goal**: verify the analog chain (DAC output, wiring, ADC input) is
functional before any optics are involved.

**Procedure**:
1. Short ADC3 to DAC0 with a jumper wire (this is the loopback path defined
   in §5.1).
2. Run `python -m beer_analyzer.calibration.self_test`.
3. The script writes a 1 kHz sine to DAC0 at 50 % amplitude and reads ADC3.
4. **Pass criterion**: ADC3 reconstructed amplitude within ±2 % of DAC0 sent
   amplitude, THD < 1 %, no missing samples in the 1 s capture.

### 9.2 Dark current and noise floor

**Goal**: characterize the photodiode + TIA without any light source.

**Procedure**:
1. Close the enclosure. No illumination.
2. Record ADC0 for 60 s at 10 kHz.
3. Compute mean (dark offset), standard deviation (noise floor), and PSD
   (should be white except for any mains pickup at 50/100 Hz).
4. **Pass criterion**: noise floor < 1 mV rms; no spectral lines above the
   white floor except possibly at 50 Hz (mains) and harmonics; dark offset
   within ±100 mV of the 1.65 V mid-rail bias.

If the noise floor is higher than 1 mV rms, investigate TIA grounding (see
§4.8) before proceeding.

### 9.3 Wavelength-by-wavelength intensity reference (I₀)

**Goal**: establish the reference intensity I₀(λ) for each channel, with a
water-filled cuvette as the blank.

**Procedure**:
1. Fill a cuvette with distilled water.
2. Place in the holder, close the enclosure.
3. For each channel individually: enable only that channel's modulation,
   record ADC0 for 10 s with the lock-in active, log the recovered R value.
4. Store as `I0[channel]` in `config.toml`.

These I₀ values are the denominators for all subsequent absorbance
calculations: `A(λ) = -log10(I_sample(λ) / I0(λ))`.

### 9.4 Absorbance linearity verification (Beer-Lambert)

**Goal**: verify that the instrument responds linearly to absorbance over
the range 0 – 2 OD.

**Procedure**:
1. Prepare a serial dilution of methylene blue in water at the following
   nominal concentrations: 0, 0.5, 1, 2, 5, 10, 20 µM.
2. Measure each on CH5 (650 nm laser — methylene blue peak is at 664 nm,
   close enough).
3. Plot A vs concentration. Fit linear regression.
4. **Pass criterion**: R² > 0.999 over 0.5 – 10 µM; slope consistent with
   ε(methylene blue, 664 nm) ≈ 85 000 M⁻¹ cm⁻¹ within ±10 %.

If this fails, the most likely causes are: saturated ADC at the water blank,
stray light leakage, or photodiode reverse-bias issues.

### 9.5 Per-assay calibration curves

For each chemical assay, a dedicated calibration curve with known standards.
These are run once per reagent batch and stored in `config.toml`.

- **Ethanol**: ethanol standards at 1.0, 2.5, 5.0, 7.5, 10.0 % v/v, diluted
  1:1000, read on CH0 after enzymatic reaction plateau.
- **Color EBC**: no calibration needed (direct A430 × 25 is the standard
  formula), but a blank must be fresh degassed water or beer solvent.
- **Haze EBC**: formazin standards at 1, 5, 10, 20 EBC haze units, read on
  CH5 at 90° scattering.
- **Bradford protein**: BSA standards at 100, 250, 500, 750, 1000 mg/L.
- **Folin polyphenols**: gallic acid standards at 50, 100, 250, 500, 1000
  mg/L.
- **DNS sugars**: glucose standards at 0.5, 1, 2, 5, 10 g/L.
- **Iron**: FeSO₄ standards at 0.5, 1, 2, 5 mg/L.

Each calibration curve is a linear regression stored as `slope`, `intercept`,
`R²`, and `date_ran`. The software refuses to report a value in physical
units if the calibration for that assay is older than 30 days.

---

## 10. Experimental protocols

### 10.1 Ethanol (enzymatic, ADH/NAD⁺)

**Reagents**: Megazyme K-ETOH kit (or DIY equivalent from §6.3).

**Procedure**:
1. Degas the beer sample (centrifuge 10 min at 3000 × g, or gently shake in
   an open tube for 2 min).
2. Dilute the degassed beer 1:100 in distilled water. Then dilute 1:10 again
   (final dilution 1:1000).
3. Pipette into a UV-transparent cuvette:
   - 0.1 mL diluted sample
   - 2.0 mL buffer (Tris-PPi, pH 8.8, from kit)
   - 0.2 mL NAD⁺ solution (from kit)
4. Mix by inversion. Place in holder. Record baseline A340 for 60 s.
5. Add 0.05 mL ADH solution (from kit). Mix.
6. Start kinetic acquisition. Record A340 until plateau (typically 5 – 8
   minutes).
7. Instrument computes `[ethanol] = f(A_plateau − A_baseline)` using the
   calibration curve from §9.5.

**Typical result**: a 5 % v/v beer will show a plateau A340 rise of ~0.27
(for the 1:1000 dilution, 1 cm path). Noise floor of the plateau detection
is ~0.002 OD, translating to ~0.05 % v/v resolution.

### 10.2 Color (EBC)

**Procedure**:
1. Degas beer sample.
2. If haze > 1 EBC unit, centrifuge (5 min at 5000 × g) or filter through 0.45
   µm membrane. **Haze interferes with the color measurement** — the EBC
   method explicitly requires a clarified sample.
3. Fill a standard plastic cuvette.
4. Run measurement on CH1 (430 nm). Instrument reads A430 directly.
5. Report: `EBC = A430 × 25`.

### 10.3 Turbidity / haze (EBC)

**Procedure**:
1. Degas beer sample. **Do not** filter or centrifuge — haze is what you
   want to measure.
2. Fill a cuvette.
3. Run measurement on CH5 (650 nm laser) in nephelometry mode (90° detector
   / ADC1 if the haze port is installed; otherwise use the difference
   `A(650) − A(940)` as a proxy for scattering).
4. Instrument compares to the formazin calibration curve.
5. Report: haze in EBC units.

### 10.4 Protein (Bradford)

**Reagents**: Bio-Rad Bradford reagent, BSA for calibration.

**Procedure**:
1. Degas sample. Dilute as needed (beer is typically 300 – 800 mg/L protein;
   dilute 1:2 or 1:5 to fit the assay range).
2. Mix 0.1 mL sample + 5.0 mL Bradford reagent. Invert to mix.
3. Incubate 5 minutes at room temperature.
4. Fill cuvette, measure on CH3 (525 nm) or CH4 (590 nm). Bradford peak is
   595 nm; 590 is optimal.
5. Read absorbance. Apply calibration curve.
6. Report: protein concentration in mg/L.

### 10.5 Polyphenols (Folin-Ciocalteu)

**Reagents**: Folin-Ciocalteu reagent (2 N), sodium carbonate (20 % w/v),
gallic acid standards.

**Procedure**:
1. Degas sample. Dilute 1:10 in water.
2. Mix in a cuvette:
   - 0.1 mL diluted sample
   - 0.5 mL Folin-C reagent (freshly diluted 1:10 in water)
   - 2.0 mL 20 % Na₂CO₃ solution
3. Incubate 30 minutes at room temperature, protected from direct light.
4. Measure on CH6 (740 nm). Folin-C blue complex peak is at 765 nm; 740 nm
   gives ~80 % sensitivity.
5. Apply gallic acid calibration curve.
6. Report: polyphenols in mg/L gallic acid equivalents (GAE).

### 10.6 Reducing sugars (DNS)

**Reagents**: dinitrosalicylic acid solution (1 % w/v DNS + 30 % sodium
potassium tartrate in 0.4 N NaOH), glucose standards.

**Procedure**:
1. Dilute sample 1:10 or 1:20 in water.
2. Mix 1.0 mL sample + 1.0 mL DNS reagent in a test tube.
3. Heat in boiling water bath for exactly 5 minutes.
4. Cool to room temperature in ice water.
5. Add 8 mL water, mix.
6. Transfer to cuvette, measure on CH3 (525 nm). DNS-sugar complex peak is
   at 540 nm; 525 is acceptable.
7. Apply glucose calibration curve.
8. Report: reducing sugars in g/L glucose equivalents.

### 10.7 Iron (optional)

**Reagents**: o-phenanthroline (0.1 % w/v), hydroxylamine hydrochloride
(10 % w/v), sodium acetate buffer pH 4.5.

**Procedure**:
1. Dilute or digest sample as needed.
2. Mix 2 mL sample + 1 mL hydroxylamine (reduces Fe³⁺ → Fe²⁺) + 1 mL buffer
   + 1 mL o-phenanthroline.
3. Incubate 10 minutes.
4. Measure on CH3 (525 nm). Fe²⁺-phenanthroline complex peak is at 510 nm;
   525 is adequate.
5. Apply FeSO₄ calibration curve.
6. Report: iron in mg/L.

---

## 11. Safety

### 11.1 Laser Class 3R handling rules

The 5 V 650 nm laser modules used in this instrument are typically rated 1 –
5 mW. At ~5 mW they are **Class 3R** per IEC 60825-1. Class 3R lasers can
cause retinal damage on direct exposure; the natural blink reflex is
insufficient protection.

**Mandatory rules**:

1. The laser beam path is **fully enclosed** in the instrument. The only way
   to see the beam is to open the enclosure. Opening the enclosure
   mechanically trips the cover interlock microswitch, which forces the
   laser off *in the firmware of the ATSAM3X8E*, not only in the host
   software.
2. During initial alignment (first power-on, with the enclosure open for
   visual beam inspection), **laser safety glasses rated OD 2+ for 650 nm**
   are worn. Suppliers: Laservision, Thorlabs, Edmund Optics. Cost ~€15.
3. Never point the laser beam at another person, a mirror, or a glossy
   surface that could produce a specular reflection.
4. Never operate the instrument with the cover interlock bypassed or
   defeated.
5. The status LED on the front of the enclosure must be lit red whenever
   the laser is enabled. Any discrepancy between the software state and the
   status LED indicates a wiring fault and the instrument must be powered
   off until the fault is found.

### 11.2 UV LED safety

The 340 nm UV LED radiates at a wavelength that is harmful to the eyes and
skin on prolonged exposure. Output is ~1.5 mW optical. The interlock story
is the same as the laser: the UV LED is inside the enclosure, and the
firmware disables it when the cover is open.

### 11.3 Chemical handling

- **Folin-Ciocalteu reagent**: acidic and mildly caustic. Use gloves.
- **Sodium carbonate**: mildly caustic. Use gloves.
- **Bradford reagent**: stains skin and clothing (Coomassie Brilliant Blue).
  Use gloves and a lab coat.
- **Methylene blue**: staining agent. Use gloves.
- **Iron standards**: heavy metal, dispose as hazardous waste.
- **Dinitrosalicylic acid**: irritant. Use gloves and eye protection.

All reagents should be disposed of through a laboratory waste stream, not
the household drain. If operating in a home brewery setting without lab
waste facilities, consider limiting the instrument to the ethanol, color,
haze, and Bradford assays (all of which produce small volumes of
water-dilutable waste).

### 11.4 Pre-power-on checklist

Before every session:

- [ ] Cover closed, interlock microswitch reports "closed" (green status LED
      lit).
- [ ] No reagent spills in the enclosure.
- [ ] Cuvette clean, correctly oriented, fully seated.
- [ ] No jumpers bypassing the safety interlock.
- [ ] Software reports `INTERLOCK_OK` on startup.
- [ ] Self-test passes (§9.1).
- [ ] Laser safety glasses available if cover will be opened.

---

## 12. Extensions

### 12.1 UV-C channel for IBU (bitterness)

Add a 275 nm UV-C LED (Crystal IS Klaran, Seoul Viosys, or Bolb, ~€15 –
€30) on a 9th channel. Use an external digital logic chip (e.g. 74HC161
counter) to generate a 9th modulation carrier, or reuse one of the DAC
channels with a fresh frequency assignment.

Replace CH0 cuvettes with **fused silica** (€15 – €25 each) for the UV-C
transmittance path.

Protocol: extract isohumulones from beer with iso-octane per ASBC Beer-23
method. Read A275 of the iso-octane phase. IBU = A275 × 50.

**Expected precision**: ±2 IBU (limited by the UV-C LED's spectral width and
stability).

### 12.2 Multi-angle nephelometer with extra 650 nm lasers

The user's additional 650 nm laser modules (2 – 3 units beyond the one used
for CH5) can be mounted around the cuvette at different scattering angles
(e.g. 45°, 135°, 180° back-scatter) and modulated at independent frequencies
using spare DOUT channels. The single photodiode detects all scattering
contributions; separate lock-ins on the host reconstruct the angular
scattering profile.

Applications: particle size distribution (static light scattering, SLS),
aggregation / flocculation kinetics, colloid stability.

Publication: separate HardwareX paper as "Multi-angle nephelometer accessory
for multispectral photometer", completely distinct from the base
spectrophotometer paper.

### 12.3 Automated multi-cuvette turret

A motorized turret with 6 or 8 cuvette positions, driven by a stepper motor
on DOUT8 – DOUT11 (4 step/direction signals via a ULN2003 or A4988 driver)
and position-sensed by an optical encoder on DIN0 – DIN2. Allows batch
processing of samples without opening the enclosure.

Phycommander already supports the required I/O (unused DOUT and DIN
channels are reserved in §5.1).

### 12.4 Integration with density / refractive-index measurements

A cheap refractometer (€5 – €20 on Amazon/Aliexpress) or a home-brewing
hydrometer provides density. Combined with optical ethanol (from the
enzymatic assay), density gives you the **original gravity** and
**apparent attenuation** — the two parameters every home brewer wants. The
density reading is entered manually into the software as part of the assay
workflow; it does not need an electronic interface.

---

## 13. Publication plan & roadmap

### 13.1 Paper structure (target: HardwareX)

- **Title**: *An open-hardware 8-channel multispectral beer analyzer using
  lock-in multiplexing on a deterministic DAQ*.
- **Abstract**: we describe an open-source instrument that measures ethanol,
  color, haze, protein, polyphenols, and reducing sugars in beer with
  commercial-grade precision for ~€400 total (~€100 hardware + ~€300
  reagents per 100 samples). The instrument uses software lock-in detection
  on 8 LEDs/laser modulated at distinct frequencies through a shared
  photodiode, enabled by the deterministic DAC-ADC streaming of the
  phycommander open DAQ platform.
- **Figures**:
  1. System photograph + block diagram (§4.1).
  2. Electronic schematic of TIA + LED drivers (§4.3 – §4.4).
  3. Pin mapping and firmware architecture (§5, §7).
  4. Beer-Lambert linearity validation with methylene blue (§9.4).
  5. Ethanol calibration curve and sample kinetic trace (§10.1).
  6. Cross-validation of all 8 parameters against a reference instrument
     (Anton Paar Alcolyzer or commercial kit) on 10 commercial beers.
- **Supplementary**: BOM, STL files, KiCad schematics, firmware source,
  host software source, calibration datasets.

### 13.2 Roadmap

| Phase | Deliverable | Status |
|---|---|---|
| **0** | This document | ✓ draft 1 |
| **1** | Firmware v1.1 with PWM and interlock | pending — §7 is the spec |
| **2** | Electronic build (breadboard) | pending BOM order |
| **3** | 1st calibration run (§9.1 – §9.4) | pending Phase 2 |
| **4** | 1st ethanol assay with known standard | pending Phase 3 |
| **5** | Electronic build (custom PCB) | optional; breadboard is acceptable for paper |
| **6** | 3D-printed enclosure | pending mechanical CAD |
| **7** | Cross-validation on 10 commercial beers | pending Phase 4 |
| **8** | Draft paper | pending Phase 7 |
| **9** | Submit to HardwareX | — |
| **10** | Fase 2: multi-angle nephelometer | parallel, separate paper |

---

## 14. Appendices

### Appendix A: Modulation frequency selection

The 8 modulation frequencies must satisfy:

1. Each `fᵢ ∈ [200, 2500]` Hz (practical bounds from §3.3).
2. `|fᵢ − fⱼ| > 5 Hz` for all i ≠ j (so that 1 s integration separates
   them).
3. `|fᵢ + fⱼ − fₖ| > 5 Hz` for all i, j, k (no 3rd-order
   intermodulation products).
4. No fᵢ is a rational multiple of any fⱼ with denominator ≤ 3 (so that
   square-wave harmonics of one channel don't alias into the fundamental of
   another channel, up to the 3rd harmonic).

The set `{1013, 1709, 571, 881, 1223, 1439, 1931, 2311}` Hz satisfies all
four constraints by construction. Verification script in
`phyclient/examples/beer_analyzer/tests/test_freq_set.py`.

### Appendix B: Lock-in math expanded

Given a signal

```
v(t) = Σₖ Aₖ sin(2π fₖ t + φₖ) + n(t)
```

the lock-in for channel k computes

```
Iₖ(T) = (2/T) ∫₀ᵀ v(t) sin(2π fₖ t) dt
Qₖ(T) = (2/T) ∫₀ᵀ v(t) cos(2π fₖ t) dt
```

For any `j ≠ k`, the cross-term is

```
(2/T) ∫₀ᵀ Aⱼ sin(2π fⱼ t + φⱼ) sin(2π fₖ t) dt
  = (Aⱼ/T) [ sin((fⱼ−fₖ)πT + φⱼ)/(2π(fⱼ−fₖ)) − sin((fⱼ+fₖ)πT + φⱼ)/(2π(fⱼ+fₖ)) ]
```

which, for `|fⱼ − fₖ| ≫ 1/T`, decays as `1/(π (fⱼ−fₖ) T)`. With `T = 1` s
and `|fⱼ − fₖ| ≥ 100` Hz, the cross-term is less than `1/(100π) ≈ 3 × 10⁻³`
times the diagonal term — that is, −50 dB. This is where the
"cross-channel leakage < −50 dB after 1 s" claim comes from.

### Appendix C: TIA design equations (reference)

For a photodiode TIA with feedback R_f and parasitic photodiode capacitance
C_d + op-amp input capacitance C_i:

- **Noise gain pole**: `f_p = 1 / (2π R_f (C_d + C_i))`
- **Noise gain zero** (from C_f): `f_z = 1 / (2π R_f C_f)`
- **Compensation condition**: `C_f = √((C_d + C_i) / (2π R_f GBW))` for
  45° phase margin
- **Output voltage noise density**:
  `e_n_out = e_n_op × √(1 + (f / f_z)²)` approximately

For S1227-1010BR (C_d ≈ 50 pF), OPA381 (C_i ≈ 7 pF, GBW ≈ 18 MHz),
R_f = 10 MΩ: `C_f ≈ 0.7 pF`, round up to 1 pF. Stable, slightly
over-compensated.

### Appendix D: Safety checklist (printable)

```
PRE-POWER-ON
[ ] Enclosure cover closed
[ ] Interlock GREEN (software reports INTERLOCK_OK)
[ ] No reagent spills inside enclosure
[ ] Cuvette seated correctly
[ ] No jumpers on interlock bypass
[ ] Laser safety glasses available

DURING MEASUREMENT
[ ] Do not open enclosure without software prompt
[ ] Do not defeat the interlock

POST-SESSION
[ ] Power off laser master enable
[ ] Remove cuvette
[ ] Wipe down enclosure interior with dry cloth (no solvents)
[ ] Dispose of reagent waste through lab waste stream
```

### Appendix E: Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| High noise floor (>1 mV rms) | Ground loop, LED switching noise | Star ground the TIA separately from LED returns |
| ADC saturated at blank | R_f too high for light level | Reduce R_f to 1 MΩ, or add ND filter in front of photodiode |
| Ethanol reading ~0 % on beer | Dilution wrong, or reagent expired | Verify 1:1000 dilution; check kit expiry date |
| Color reading negative | Blank saturated ADC; sample is weakly absorbing | Re-run blank with fresh water; verify cuvette is clean |
| Haze reading wrong sign | Photodiode sees direct beam instead of scattered | Misaligned 90° port; re-check optics |
| Cross-channel leakage visible | Modulation frequencies too close | Re-run frequency verification script |
| Interlock reports OPEN with closed cover | Microswitch bent or misadjusted | Mechanical adjustment |
| Firmware reports PWM_FAULT | PWM frequency out of range | Check `config.toml` frequency assignment |

---

*End of document. Questions, errata, or contributions → issues on the
phycommander repository.*
