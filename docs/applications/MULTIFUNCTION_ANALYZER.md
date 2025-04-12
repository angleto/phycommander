# phycommander Multi-Function Analytical Bench Instrument

> A modular open-hardware laboratory bench instrument that integrates
> 16 distinct measurement modes — optical, electrochemical, physical, and
> closed-loop control — into a single base station driven by phycommander.
> Replaces a rack of specialized commercial instruments (spectrophotometer,
> nephelometer, fluorometer, pH meter, conductivity meter, temperature
> controller, titrator, flow velocimeter) with one scriptable platform at
> ~€300 – €500 total hardware cost.

---

**Document version**: draft 1, 2026-04-11
**Target phycommander version**: v1.1 (PWM + firmware interlock) for core modes,
v1.2 (SPI + I²C + TC) for full electrochemistry and motion
**Status**: design document — master vision for a complete analytical bench
**Intended license**: CERN-OHL-S v2
**Related documents**:
- [`LOCKIN_OPTICAL_DEMO.md`](LOCKIN_OPTICAL_DEMO.md) — minimal lock-in demonstrator (didactic starting point)
- [`BEER_ANALYZER.md`](BEER_ANALYZER.md) — focused configuration for beer QC (one specific use of this instrument)

---

## Table of Contents

1. [Vision and philosophy](#1-vision-and-philosophy)
2. [Measurement paradigms](#2-measurement-paradigms)
3. [The 16 measurement modes](#3-the-16-measurement-modes)
4. [Parameter matrix: what you can measure](#4-parameter-matrix-what-you-can-measure)
5. [Modular system architecture](#5-modular-system-architecture)
6. [Wavelength selection for the optical core](#6-wavelength-selection-for-the-optical-core)
7. [Hardware design — base station](#7-hardware-design--base-station)
8. [Hardware design — detachable modules](#8-hardware-design--detachable-modules)
9. [Mapping to phycommander I/O](#9-mapping-to-phycommander-io)
10. [Bill of materials (tiered)](#10-bill-of-materials-tiered)
11. [Firmware requirements](#11-firmware-requirements)
12. [Host software architecture](#12-host-software-architecture)
13. [Calibration strategy](#13-calibration-strategy)
14. [Application domains](#14-application-domains)
15. [Safety](#15-safety)
16. [Publication plan & roadmap](#16-publication-plan--roadmap)
17. [Appendices](#17-appendices)

---

## 1. Vision and philosophy

### 1.1 One instrument, many modes

Traditional analytical instruments are specialized: a UV-Vis
spectrophotometer, a nephelometer, a fluorometer, a pH meter, a
conductivity meter, an incubator with temperature control, an automated
titrator, a flow velocimeter — each a separate box costing €500 – €25 000,
each with its own vendor software, its own calibration procedure, its own
closed data format.

This document describes a single open-hardware instrument that performs
**all of these functions** by sharing a common detection backbone
(phycommander + UV-enhanced photodiode + transimpedance amplifier +
lock-in demodulation) and swapping one of a small set of **detachable
modules** — optical heads, electrochemical heads, actuator boards — as
needed. The host PC, running Linux PREEMPT_RT and a Python application
framework, orchestrates everything: which mode is active, which sources
are modulated, how the signal is demodulated, how the result is
calibrated against known standards, how the data is logged and reported.

The total cost of the full multi-function instrument, with every mode
enabled, is **€300 – €500 of hardware plus per-application reagents** —
less than any single commercial instrument it replaces.

### 1.2 Why this is possible

Because phycommander provides **deterministic, sample-coherent,
multi-channel DAC↔ADC streaming**, it can simultaneously:

- Generate N independent modulated excitation signals (sinusoidal or
  square-wave carriers) via its DAC, PWM, and DOUT channels
- Acquire M independent detector channels via its ADC inputs
- Drive actuators (motors, pumps, Peltier heaters) via PWM and DOUT
- Read position/state sensors (encoders, microswitches, thermistors) via
  ADC and DIN

— **all in the same 10 kHz real-time loop**, with sample-accurate timing
between every output and every input. This determinism is what enables
lock-in detection (for optical absorbance/scattering/fluorescence),
synchronous demodulation (for AC conductivity and Doppler), and closed-loop
control (for temperature, titration, motion) to coexist without conflict
in a single firmware load.

### 1.3 The modular principle

The instrument is organized into four hardware layers:

```
┌─────────────────────────────────────────────────────────────────┐
│                      HOST PC (Linux RT)                         │
│   Python multi-mode application framework                       │
│   — shared lock-in engine                                        │
│   — mode-specific state machines                                 │
│   — calibration database                                         │
│   — reporting / database                                         │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             │  USB, 10 kHz streaming
                             │
┌────────────────────────────┴────────────────────────────────────┐
│                   PHYCOMMANDER BASE STATION                     │
│   — 8 analog/PWM output channels                                 │
│   — 8 ADC inputs                                                 │
│   — 16 GPIO in/out                                               │
│   — Universal photodiode + TIA (primary detector)                │
│   — Power distribution (5 V, ±12 V, 3.3 V)                       │
│   — Safety interlock + status LEDs                               │
│   — 2× 20-pin expansion headers to modules                       │
└──┬──────────────┬───────────────┬────────────────┬───────────────┘
   │              │               │                │
   ▼              ▼               ▼                ▼
┌──────────┐  ┌─────────┐  ┌─────────────┐  ┌──────────────┐
│ OPTICAL  │  │ OPTICAL │  │  ELECTRO-   │  │   ACTUATOR   │
│  HEAD:   │  │  HEAD:  │  │  CHEMICAL   │  │    BOARD     │
│ absorb.  │  │ fluoro. │  │   MODULE    │  │              │
│ nephel.  │  │ Doppler │  │ pH, κ, ORP, │  │ motors,      │
│          │  │         │  │ amperom.    │  │ pumps,       │
│ (8 LEDs  │  │ (laser  │  │             │  │ Peltier      │
│ + laser) │  │ + filter│  │ (buffer     │  │              │
│          │  │ + 2nd   │  │  amps, AC   │  │ (drivers,    │
│          │  │ detect.)│  │  ref)       │  │  protection) │
└──────────┘  └─────────┘  └─────────────┘  └──────────────┘
```

Only one optical head is physically mounted at a time (they share the
same cuvette holder interface and the same primary photodiode), but the
electrochemical and actuator modules can coexist with any optical head.
Head swaps take about 30 seconds and do not require tools.

### 1.4 Who this is for

- **Microbreweries / wineries / distilleries** wanting a complete QC
  bench at a fraction of commercial cost
- **University teaching labs** in chemistry, biology, food science,
  physics — a single lab instrument that replaces 6–8 dedicated pieces
- **Small research groups** that need benchtop analytical capability
  without commercial vendor lock-in
- **Field / environmental / outreach** use — portable analytical bench
- **Maker / citizen science** — the most capable open-hardware
  analytical platform in its price class
- **Publication of open methods** — the instrument is end-to-end auditable,
  every step from photon to reported number is in the repository

### 1.5 How to read this document

This is a comprehensive *design and build* document. It is long. Different
readers should enter at different points:

| You are a... | Start at | Then read |
|---|---|---|
| Curious user who wants to know what it does | §1, §2, §4 | §14 (applications) |
| Builder with a specific application in mind | §4 | §14 (your domain), then §6–§10 (hardware) |
| Builder who wants to construct the full instrument | §1–§5 | §7–§12 in order |
| Software / firmware developer | §11, §12 | §9 (I/O map), §13 (calibration) |
| Safety officer / reviewer | §15 | §7.6 (interlock) |
| Paper reviewer / replicator | Entire document | §16 (roadmap), §17 (appendices) |

---

## 2. Measurement paradigms

The instrument covers **four fundamental measurement paradigms**. Every
mode in §3 is an instance of one of these.

### 2.1 Optical

Light at one or more wavelengths interacts with a sample; a photodiode
measures the result. The interaction can be **absorbance** (transmitted
light decreases), **scattering** (light is redirected), **fluorescence**
(sample absorbs at one λ and re-emits at another), or **Doppler shift**
(moving scatterers impose a frequency modulation on scattered light).
Phycommander modulates the source and demodulates the detector response
with software lock-in detection for noise rejection.

### 2.2 Electrochemical

Electrochemical sensors produce an electrical signal proportional to a
chemical species in solution. **Potentiometric** sensors (pH, ion-selective
electrodes, ORP) produce a high-impedance voltage proportional to the log
of the activity. **Conductometric** measurements inject an AC current and
measure the in-phase voltage to get solution conductivity. **Amperometric**
sensors (polarographic O₂, glucose oxidase, etc.) hold an electrode at a
fixed potential and measure the resulting current, linear in the analyte.

Phycommander provides the required high-impedance buffer, AC excitation,
and lock-in demodulation for each type.

### 2.3 Physical

Temperature, pressure, flow, humidity — measured with their standard
sensors (NTC, PT100, piezoresistive, etc.) wired through dedicated
front-end circuits to phycommander's ADC. These are straightforward but
essential for any real analytical bench.

### 2.4 Closed-loop control

Some measurements require *changing* the sample, not just observing it:
temperature-controlled incubation (enzyme assays), titration (adding
reagent until an endpoint), stirring (mixing), flow (for process
monitoring), automated sample handling (cuvette turret). Phycommander
drives these actuators with PWM and GPIO while simultaneously reading the
measurement sensors, closing the control loop in real time at 1 – 10 kHz.

---

## 3. The 16 measurement modes

Each subsection is a concise specification: *principle, hardware needed,
signal processing, calibration, typical precision*. Full details for
each mode are in §7 (hardware), §12 (software), §13 (calibration).

### 3.1 Multispectral absorbance (8-λ lock-in)

**Principle**: 8 LEDs (+1 laser) at different wavelengths, each modulated
at a different frequency, share a single cuvette and a single photodiode.
Eight parallel lock-ins on the host separate the absorbance at each
wavelength simultaneously.

**Hardware**: absorbance optical head, base station photodiode + TIA.

**Signal processing**: `A(λ) = −log₁₀(R_sample(λ) / R_blank(λ))` per channel.

**Calibration**: blank (water or solvent) for each wavelength stored in
`I₀(λ)`; Beer-Lambert linearity verification with methylene blue.

**Precision**: ±0.5 % on absorbance in the range 0.01 – 2.0 OD, noise
floor ~0.001 OD at 1 s integration.

**Details**: see [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §4 and §9.

### 3.2 Kinetic absorbance (enzyme assays)

**Principle**: same as 3.1, but the sample *changes over time* due to a
reaction (typically enzymatic). The instrument records a time series of
`A(λ, t)` and derives the rate `dA/dt` (initial velocity) or the plateau
(endpoint) depending on the assay.

**Hardware**: same as 3.1, plus a temperature-controlled cuvette holder
(mode 3.11) if the assay requires 37 °C incubation.

**Signal processing**: real-time plateau detection (slope below threshold
for N consecutive seconds) and automated reporting of the endpoint.
Initial-rate measurement by linear regression of the first 60 s.

**Calibration**: ethanol standards (1, 2.5, 5, 7.5, 10 % v/v) for ADH;
NADH standards for NAD-linked assays; enzyme-specific curves for each
analyte.

**Precision**: ±0.1 % v/v for ethanol; ±2 % for most NAD-linked assays;
generally limited by pipetting precision, not by the instrument.

**Example assays**:
- Ethanol (ADH / NAD⁺)
- Glucose (hexokinase / G6P-DH / NAD⁺)
- Lactate (lactate dehydrogenase)
- Pyruvate
- Aldehyde
- ATP (via hexokinase coupled assay)

### 3.3 Single-angle nephelometry (haze / turbidity)

**Principle**: a laser or LED illuminates the sample; a photodiode at 90°
(or forward-scattering) to the beam measures the light scattered by
particles in suspension. The signal is proportional to the concentration
and (in a complex way) to the size of the scatterers.

**Hardware**: absorbance head with a second photodiode at a 90° port
(already designed into the optical head; see §8.1).

**Signal processing**: amplitude-only lock-in at the source frequency.

**Calibration**: formazin turbidity standards (1, 5, 10, 20, 50 NTU / EBC
haze units). Formazin is cheap (~€20 for a primary stock) and stable.

**Precision**: ±0.5 EBC unit in the range 0 – 50 EBC.

**Standard methods compatible**: EBC 9.29 (beer haze), EPA 180.1 (drinking
water turbidity), ISO 7027.

### 3.4 Multi-angle nephelometry (Static Light Scattering, SLS)

**Principle**: **multiple coherent laser sources at 650 nm**, modulated at
different frequencies, mounted around the cuvette at different angles
(e.g. 30°, 45°, 60°, 90°, 120°, 150°). A single photodiode sees the
cumulative scattering and the lock-in bank separates the contribution at
each angle. The angular profile `I(θ)` is fit with Mie or Rayleigh theory
to extract particle size distribution.

**Hardware**: multi-angle scattering head (§8.3). Uses the user's
surplus 650 nm laser modules.

**Signal processing**: for each angle, lock-in amplitude. Zimm plot for
large particles, Mie inversion for colloids. Nonlinear regression on host.

**Calibration**: polystyrene latex nanoparticle standards of known
diameter (NIST traceable, €100 – €300 per set).

**Precision**: particle diameter to ±10 % in the range 50 nm – 10 µm,
depending on sample monodispersity and how well you've characterized the
instrument geometry.

**Applications**: nanoparticle QC, colloid stability, fermentation monitoring
(yeast cell density and size), milk fat globules, protein aggregation.

### 3.5 Fluorescence

**Principle**: an excitation source (LED or laser) at wavelength λ_ex
illuminates the sample; a long-pass optical filter in front of a second
photodiode blocks the excitation light but passes the red-shifted emission
light at λ_em. The emission signal is proportional to fluorophore
concentration.

**Hardware**: fluorescence optical head (§8.2). Key components:
- Excitation LED / laser (matched to the fluorophore's excitation peak)
- Long-pass emission filter (~20 nm above excitation; e.g. Thorlabs FEL0400
  for 405 nm excitation → 420 nm long-pass)
- Secondary photodiode at 90° to the excitation beam
- Black matte baffles to suppress scattered excitation

**Signal processing**: lock-in at the excitation modulation frequency.
Ratiometric normalization against a reference photodiode that monitors the
excitation source intensity directly.

**Calibration**: fluorescein or quinine sulfate standards of known
concentration.

**Precision**: ±2 % on relative fluorescence, detection limit ~nM range
for bright fluorophores.

**Applications**:
- Chlorophyll quantification (λ_ex 430, λ_em 680 nm)
- NADH fluorescence (λ_ex 340, λ_em 460 nm — secondary method to
  absorbance, more sensitive)
- DAPI/Hoechst DNA staining
- GFP-tagged protein quantification
- Fluorescein as a tracer in microfluidics or diffusion studies

### 3.6 Photoacoustic spectroscopy

**Principle**: a modulated laser heats the sample periodically; the
absorbed energy causes cyclic thermal expansion, which generates acoustic
waves at the modulation frequency, detected by a sensitive microphone.
The signal is proportional to the absorbed energy, which for thin samples
equals `(1 − T) × P_in` where `T` is transmittance and `P_in` is source
power.

**Hardware**: modified cuvette holder with an acoustic sensor (piezo disc
or electret microphone) mounted in contact with the cuvette or the sample
volume; standard absorbance optical head for the source.

**Signal processing**: lock-in at the laser modulation frequency applied
to the microphone signal.

**Calibration**: compare against transmittance measurement on the same
sample.

**Precision**: useful dynamic range extends the absorbance measurement
by ~1 OD beyond the photodiode-limited range (i.e. can measure A up to
~4 OD where the transmitted beam is completely extinguished).

**Applications**: very turbid or highly absorbing samples (whole blood,
dark beer, ink dilutions, biological tissue) where standard transmittance
measurement saturates.

### 3.7 Laser Doppler velocimetry (LDV)

**Principle**: two coherent 650 nm laser beams cross at a small angle `θ`
in the sample volume, creating an interference fringe pattern with
spacing `d = λ / (2 sin(θ/2))`. Particles moving through the fringe
pattern scatter light with a Doppler-modulated intensity at frequency
`f_D = v / d = 2 v sin(θ/2) / λ`, linear in velocity `v`.

**Hardware**: crossed-laser optical head using two of the user's 650 nm
laser modules, precisely aligned with a beam crossing in the sample
volume. A single photodiode collects the backscattered light.

**Signal processing**: FFT of the photodiode signal (not lock-in —
Doppler frequency varies with velocity, so a fixed-frequency lock-in
doesn't work). Peak detection in the spectrum gives f_D; compute v.

**Calibration**: geometric — once the beam-crossing angle θ is measured,
velocity is absolute. Independent verification with a known rotating
disk or a calibrated flow.

**Precision**: ±5 % of reading, 0 – 5 mm/s range with θ = 10° and
650 nm, up to ~50 mm/s with larger crossing angles.

**Applications**: microfluidic flow characterization, capillary flow,
fermenter mixing velocity, Brownian motion → particle sizing via
dynamic light scattering (DLS).

### 3.8 Potentiometry (pH, ORP, ion-selective electrodes)

**Principle**: a glass or solid-state electrode produces a voltage
proportional to the log of the target ion activity, referenced to a
stable reference electrode. E = E₀ + (RT/nF) × log₁₀(a_ion). For a
pH electrode at 25 °C, the slope is −59.16 mV / pH unit.

**Hardware**: electrochemical module (§8.4) with a high-impedance
instrumentation amplifier (InAmp INA116 or similar, input impedance
> 10¹⁵ Ω) to not load the electrode; output to ADC.

**Signal processing**: simple DC voltage measurement with ~20 Hz
low-pass to reject mains. Conversion to pH via two-point calibration
(pH 4 and pH 7 buffers standard, pH 10 optional).

**Calibration**: standard pH buffers (IUPAC primary standards).

**Precision**: ±0.01 pH units with a good electrode; ±0.05 pH with
the cheapest electrodes.

**Electrode types supported**:
- pH (glass membrane)
- ORP / redox (Pt or Au electrode with Ag/AgCl reference)
- Ion-selective: Na⁺, K⁺, Ca²⁺, NH₄⁺, NO₃⁻, Cl⁻, F⁻ (all via ISE)
- Dissolved oxygen (Clark electrode — actually amperometric, see 3.10)

### 3.9 Conductometry (AC conductivity)

**Principle**: an AC voltage at ~1 kHz is applied to two (or four)
electrodes immersed in the solution; the in-phase current is measured
via a sense resistor. The ratio of current to voltage gives the
conductance G, which depends on the geometry (cell constant K) and the
solution conductivity σ: `G = σ × A / l = σ / K`.

**Hardware**: electrochemical module (§8.4). DAC0 generates the AC
excitation; the current is sensed on a second ADC channel through a
precision sense resistor. Use AC (not DC) to avoid electrode polarization
and electrolysis.

**Signal processing**: **lock-in at the AC excitation frequency** — the
same lock-in engine used for optical mode, now applied to a 1 kHz
voltage. The in-phase component gives the conductance; the quadrature
component reveals any capacitive loading.

**Calibration**: KCl standard solutions of known conductivity (0.1 M
KCl → 12.9 mS/cm at 25 °C is the primary standard).

**Precision**: ±1 % of reading in the range 10 µS/cm – 100 mS/cm.

**Applications**:
- Total dissolved solids (TDS) in water
- Salinity
- Fermentation progress (ionic strength changes)
- Titration endpoint detection (conductometric titration)
- Monitoring of deionization systems

### 3.10 Amperometry (O₂, glucose, etc.)

**Principle**: an electrode is held at a fixed potential (typically
−0.6 V vs Ag/AgCl for O₂ reduction at a Pt or Au cathode); the resulting
current, proportional to the analyte concentration, is measured.

**Hardware**: electrochemical module (§8.4). A potentiostat circuit
(typically a 3-electrode op-amp configuration) maintains the working
electrode potential and converts current to voltage for the ADC.

**Signal processing**: DC measurement with slow low-pass filter.
Optional lock-in if the electrode is modulated (pulsed amperometry).

**Calibration**: known analyte concentrations.

**Precision**: ±5 % typical, down to ±1 % for well-characterized
electrodes.

**Applications**:
- Dissolved oxygen (Clark electrode)
- Glucose (glucose-oxidase + peroxidase electrode)
- Chlorine / free chlorine (gold electrode)
- Ethanol (immobilized ADH electrode — alternative to optical)

### 3.11 Temperature measurement

**Principle**: resistance or voltage of a sensor varies with temperature.
NTC thermistor (~10 kΩ @ 25 °C, negative temperature coefficient, steep
response), PT100/PT1000 (platinum RTD, linear, accurate), or
thermocouple (voltage proportional to temperature difference).

**Hardware**: passive divider for NTC/RTD on ADC; for thermocouples,
a cold-junction compensation IC (e.g., MAX31855 via SPI — requires v1.2
firmware).

**Signal processing**: Steinhart-Hart equation for NTC; linear formula
for PT100; lookup table for thermocouple.

**Calibration**: ice bath (0 °C) and boiling water (100 °C corrected for
local pressure) are free and accurate to ±0.1 °C.

**Precision**:
- NTC: ±0.1 °C
- PT100: ±0.05 °C (with 4-wire connection)
- K-type thermocouple: ±1 °C

**Applications**: essential for every thermostatted assay, and as a
stand-alone temperature measurement channel.

### 3.12 Temperature control (Peltier + PID)

**Principle**: a Peltier module (thermoelectric cooler, TEC) heats or
cools the cuvette holder. The current is bidirectional (polarity controls
heat/cool direction), driven by an H-bridge with PWM amplitude modulation.
A PID controller on the host reads the cuvette temperature (mode 3.11)
and updates the PWM duty cycle.

**Hardware**: actuator module with an H-bridge (e.g., DRV8871 or L298N
for small Peltiers; BTS7960 for larger ones); a 40 W Peltier module
fastened to the cuvette holder with heatsink and fan on the hot side.

**Signal processing**: PID loop, typically at 10 – 100 Hz update rate.
Phycommander's 10 kHz is more than sufficient.

**Calibration**: PID tuning via Ziegler-Nichols or Cohen-Coon on the
specific thermal mass.

**Precision**: ±0.1 °C hold over the range 10 – 60 °C; transient
settling ~30 s for a 20 °C step.

**Applications**: any enzyme assay (37 °C standard), DNA melting
(temperature ramps), melting-point analysis, heat of reaction studies.

### 3.13 Stirring

**Principle**: a small DC motor drives either a magnetic stirrer (with a
magnetic bar in the cuvette) or an overhead paddle. PWM controls the
motor speed.

**Hardware**: actuator module with a DC motor driver (same H-bridge as
3.12 or a simpler MOSFET for unidirectional stirring).

**Signal processing**: open-loop PWM duty cycle; optional tachometer
feedback via a reflective IR sensor reading a mark on the motor shaft
for closed-loop speed control.

**Calibration**: RPM vs duty cycle curve established once per motor.

**Applications**: cuvette mixing during titration, keeping suspensions
homogeneous during measurement, enzyme substrate mixing.

### 3.14 Automated titration

**Principle**: a peristaltic pump delivers titrant to a stirred sample at
a controlled rate (PWM). A pH or ORP electrode (mode 3.8) measures the
response in real time. When the target endpoint is detected (pH
inflection point, or predetermined value), the pump stops and the total
volume delivered is reported.

**Hardware**: actuator module with a peristaltic pump (3 V DC pump
~€15 – €30 online), stirrer (mode 3.13), pH electrode (mode 3.8),
weighing feedback optional.

**Signal processing**: real-time differentiation of pH vs volume to
detect the inflection point; or simple threshold detection.

**Calibration**: pH buffer for the electrode; known standards (e.g.,
0.1 N NaOH titrating 0.1 N HCl) to verify endpoint detection.

**Precision**: ±0.5 % of endpoint volume with a reasonable pump.

**Applications**:
- Acid-base titration (total acidity of wine, beer, juice)
- Complexometric titration (water hardness)
- Chloride by argentometric titration
- Any textbook volumetric analysis

### 3.15 Multi-cuvette handling

**Principle**: a rotating turret holds 4, 6, or 8 cuvettes and presents
each one in turn to the optical measurement position. A stepper motor
rotates the turret; a hall-effect or optical encoder detects position.

**Hardware**: actuator module with stepper driver (A4988 or DRV8825,
€5 each); NEMA 17 stepper motor; machined or 3D-printed turret.

**Signal processing**: home-to-index at startup, then step-and-measure
sequence. Position closed-loop via encoder if needed.

**Calibration**: once-per-build geometric calibration of turret
positions vs step count.

**Applications**: high-throughput sample handling, batch calibration
runs (multiple standards in sequence), parallel reaction monitoring.

### 3.16 Flow cell continuous monitoring

**Principle**: instead of a static cuvette, a flow cell is placed in
the optical path. The sample flows through continuously; the instrument
measures in real time and logs the time series.

**Hardware**: flow cell (€20 – €80 commercial; DIY possible with
3D-printing + transparent windows); peristaltic pump (shared with
titration) to drive the flow.

**Signal processing**: identical to batch mode, but with continuous
logging and event detection (pH shift, absorbance spike, turbidity step).

**Calibration**: same as batch mode, using a bypass loop through the
flow cell.

**Applications**:
- Bioreactor / fermenter inline monitoring
- Water quality continuous measurement
- Chromatography column effluent detection
- Process analytics

---

## 4. Parameter matrix: what you can measure

This matrix lists **all analytes measurable with the instrument** as
currently scoped, organized by application area. Each row gives the mode
(§3), the primary method, and the typical precision. "Mode #" refers to
the subsection number in §3.

### 4.1 Beverages (beer, wine, spirits, juice)

| Analyte | Mode | Method | Range | Precision |
|---|---|---|---|---|
| Ethanol | 3.2 | ADH enzymatic, NADH at 340 nm | 0 – 12 % v/v | ±0.1 % v/v |
| Color | 3.1 | A430 (EBC) | 2 – 50 EBC | ±0.2 EBC |
| Haze (turbidity) | 3.3 | 90° scattering at 650 nm | 0 – 50 EBC | ±0.5 EBC |
| Protein (total) | 3.1 | Bradford at 590 nm | 10 – 1000 mg/L | ±5 % |
| Polyphenols | 3.1 | Folin-Ciocalteu at 740 nm | 50 – 2000 mg/L GAE | ±10 % |
| Reducing sugars | 3.1 | DNS at 525 nm | 0.2 – 10 g/L | ±5 % |
| Glucose (specific) | 3.2 | Hexokinase enzymatic | 0.1 – 10 g/L | ±2 % |
| Iron | 3.1 | o-phenanthroline at 510 nm | 0.1 – 10 mg/L | ±10 % |
| pH | 3.8 | Glass electrode | 2 – 8 | ±0.01 |
| Titratable acidity | 3.14 | Titration to pH 8.2 | 0.1 – 10 g/L | ±2 % |
| Bitterness (IBU) | 3.1 (extended) | A275 after iso-octane extract | 5 – 100 IBU | ±2 IBU |
| Density / original gravity | external | Refractometer (manual input) | — | — |
| Dissolved O₂ | 3.10 | Clark electrode | 0.1 – 20 mg/L | ±0.1 mg/L |
| Dissolved CO₂ | external | Pressure sensor | — | — |

### 4.2 Water quality / environmental

| Analyte | Mode | Method | Range | Precision |
|---|---|---|---|---|
| Turbidity | 3.3 | 90° scattering at 860 nm (ISO 7027) | 0 – 1000 NTU | ±2 % |
| Color | 3.1 | A465 (Platinum-Cobalt) | 0 – 500 Pt-Co | ±5 % |
| pH | 3.8 | Electrode | 0 – 14 | ±0.01 |
| Conductivity (TDS) | 3.9 | AC 1 kHz | 1 µS – 100 mS/cm | ±1 % |
| Dissolved O₂ | 3.10 | Clark electrode | 0 – 20 mg/L | ±0.1 mg/L |
| Free chlorine | 3.1 | DPD at 515 nm | 0.05 – 5 mg/L | ±0.05 mg/L |
| Nitrate (NO₃⁻) | 3.1 | Cadmium reduction + Griess at 525 nm | 0.1 – 10 mg/L N | ±5 % |
| Nitrite (NO₂⁻) | 3.1 | Griess at 525 nm | 0.01 – 1 mg/L N | ±5 % |
| Ammonia (NH₃) | 3.1 | Nessler or salicylate at 630 nm | 0.05 – 5 mg/L N | ±5 % |
| Phosphate (PO₄³⁻) | 3.1 | Molybdenum blue at 880 nm (→ 940 nm) | 0.05 – 5 mg/L P | ±5 % |
| Iron | 3.1 | o-phenanthroline at 510 nm | 0.05 – 5 mg/L | ±5 % |
| Manganese | 3.1 | PAN or Formaldoxime at 480 nm | 0.05 – 5 mg/L | ±10 % |
| Copper | 3.1 | Bicinchoninate at 560 nm | 0.05 – 5 mg/L | ±10 % |
| Hardness | 3.14 | EDTA complexometric titration | — | ±5 % |
| Alkalinity | 3.14 | Acid titration to pH 4.5 | — | ±5 % |
| Temperature | 3.11 | NTC / PT100 | −10 to +100 °C | ±0.1 °C |
| Particulate size | 3.4 | Multi-angle SLS | 0.05 – 10 µm | ±10 % |

### 4.3 Biology / bioprocess

| Analyte | Mode | Method | Range | Precision |
|---|---|---|---|---|
| Cell density (OD600) | 3.1 | Absorbance at 600 nm (≈ 590 LED) | 0.05 – 2 OD | ±0.005 OD |
| Cell size distribution | 3.4 | Multi-angle SLS | 0.5 – 20 µm | ±10 % |
| Chlorophyll a | 3.1 | A662 (ethanol extract) | 0.1 – 50 µg/mL | ±3 % |
| Chlorophyll fluorescence | 3.5 | λ_ex 430, λ_em 680 | arbitrary units | ±2 % |
| Protein (Bradford) | 3.1 | Coomassie at 595 nm | 10 – 1000 µg/mL | ±5 % |
| Protein (BCA) | 3.1 | 562 nm (→ 590) | 10 – 1200 µg/mL | ±5 % |
| DNA (UV) | 3.1 (extended) | A260 (**requires 260 nm LED**) | 10 – 1000 µg/mL | ±5 % |
| NADH / NADPH | 3.1 or 3.5 | A340 or fluorescence | 1 µM – 1 mM | ±2 % |
| Glucose | 3.2 | Hexokinase enzymatic | 0.1 – 10 mM | ±2 % |
| Lactate | 3.2 | LDH enzymatic | 0.1 – 10 mM | ±3 % |
| Pyruvate | 3.2 | LDH reverse enzymatic | 0.01 – 1 mM | ±3 % |
| ATP | 3.2 or 3.5 | Luciferin-luciferase (bioluminescence → fluorescence mode) | 1 nM – 10 µM | ±5 % |
| Dissolved O₂ | 3.10 | Clark electrode | 0 – 100 % sat. | ±1 % |
| pH | 3.8 | Electrode | 1 – 14 | ±0.01 |
| Temperature | 3.11 | NTC | 0 – 60 °C | ±0.1 °C |
| Flow velocity (reactor) | 3.7 | Laser Doppler | 0 – 10 mm/s | ±5 % |

### 4.4 Chemistry lab

| Analyte | Mode | Method | Range | Precision |
|---|---|---|---|---|
| Methylene blue | 3.1 | A660 | 0.1 – 50 µM | ±1 % |
| Potassium permanganate | 3.1 | A525 | 0.01 – 1 mM | ±1 % |
| Iron(II) | 3.1 | o-phenanthroline at 510 nm | 0.5 – 50 µM | ±3 % |
| Acid-base equivalents | 3.14 | pH titration | — | ±1 % |
| Redox titration | 3.14 | ORP titration | — | ±2 % |
| Complexometric titration | 3.14 | Conductometric | — | ±2 % |
| Reaction kinetics (1st order) | 3.2 | Absorbance vs time | rate constant 10⁻⁴ – 10 s⁻¹ | ±5 % |
| Equilibrium constant | 3.1 | A vs pH / temperature | — | ±5 % |
| Refractive index | external | Abbe refractometer, manual input | — | — |
| Melting point | 3.12 | Optical transmission vs T | — | ±1 °C |

### 4.5 Food and agriculture

| Analyte | Mode | Method | Range | Precision |
|---|---|---|---|---|
| Ethanol in distilled spirits | 3.2 | ADH enzymatic (dilution 1:5000) | 20 – 60 % v/v | ±0.2 % v/v |
| Milk fat globule size | 3.4 | SLS | 1 – 10 µm | ±5 % |
| Olive oil UV absorbance (K270, K232) | 3.1 | A270, A232 in iso-octane (**requires UV-C LED**) | standard EU parameters | ±5 % |
| Wine color (OD420, OD520, OD620) | 3.1 | Three-wavelength absorbance | standard wine params | ±2 % |
| Wine SO₂ (free + total) | 3.14 | Ripper titration (iodine) | 10 – 300 mg/L | ±5 % |
| Honey diastase activity | 3.1 | Amylase kinetic assay | 3 – 30 Schade units | ±10 % |
| Starch content | 3.1 | Iodine test at 600 nm | — | ±5 % |

### 4.6 Physical measurements

| Measurement | Mode | Method | Range | Precision |
|---|---|---|---|---|
| Fluid flow velocity | 3.7 | LDV (crossed lasers) | 0 – 50 mm/s | ±5 % |
| Particle sedimentation | 3.1 spatial | 1D absorbance profile over time | — | ±2 % |
| Diffusion coefficient | 3.1 spatial | gradient relaxation | 10⁻¹¹ – 10⁻⁸ m²/s | ±10 % |
| Brownian motion → particle size (DLS) | 3.4 or 3.7 | Temporal fluctuations of scatter | 1 nm – 1 µm | ±20 % |

---

## 5. Modular system architecture

### 5.1 The three-layer stack

```
       HOST                BASE STATION              MODULES (exchangeable)
       PC                 (always connected)
      ┌──┐                   ┌────────┐
      │  │       USB         │  phy   │       expansion
      │  ├───────────────────┤  cmdr  ├───────┬────────┬───────┬─────┐
      │  │                   │        │       │        │       │     │
      └──┘                   │  TIA   │       ▼        ▼       ▼     ▼
                             │  Iₒ    │   ┌───────┐┌─────┐┌──────┐┌────┐
                             │  PD    │   │Optical││ECh. ││Actu- ││Ext.│
                             │        │   │ head  ││mod. ││ator  ││sen-│
                             │  Intlk │   │       ││     ││board ││sors│
                             └────────┘   └───────┘└─────┘└──────┘└────┘
```

**The base station** is always assembled. It contains phycommander, the
primary photodiode + TIA + laser safety interlock, and a power distribution
with 5 V, 3.3 V, and ±12 V rails. Two 20-pin expansion headers bring all
the unused phycommander I/O out to the modules.

**The optical head** plugs into the front of the base station and holds
the cuvette. Different heads (absorbance, fluorescence, multi-angle
scattering, Doppler, flow cell) share the same mechanical/electrical
interface.

**The electrochemical module** plugs into one of the expansion headers
and provides the high-impedance pH/ORP buffer, the AC conductivity
driver, and a 3-electrode potentiostat for amperometry.

**The actuator board** plugs into the other expansion header and provides
motor drivers, pump drivers, and a Peltier H-bridge for temperature
control.

### 5.2 Module interface definition

Each expansion header exposes:

- 4 analog out (2× DAC + 2× PWM, or 4× DAC if using SPI-expanded DACs)
- 4 analog in (ADC channels)
- 8 digital in/out (configurable)
- Power: 5 V @ 500 mA, 3.3 V @ 200 mA, ±12 V @ 100 mA (the ±12 V comes
  from a small onboard DC-DC converter)
- I²C (once firmware v1.2 exposes it) for identifying the module at boot

The host software auto-detects which modules are present by reading each
expansion header's I²C identification EEPROM (24AA02 or similar, ~€0.30
per module) at startup. Modules then register their capabilities with the
application framework.

### 5.3 Why modularity matters

1. **Hardware compatibility across applications**. The same base station
   runs beer QC today and environmental water analysis tomorrow — just
   swap the optical head.
2. **Incremental purchase**. Users can buy just the base station + one
   optical head to start, add electrochemistry later, add multi-angle
   scattering later still.
3. **Independent development**. Each module can be designed, tested, and
   published as a separate HardwareX paper.
4. **Easy repair**. Failed module = replace module, not whole instrument.
5. **Teaching sequence**. A university course can introduce modes one at
   a time, each module a week's lab.

---

## 6. Wavelength selection for the optical core

The 8-channel set used by the absorbance head is chosen to cover the
broadest span of analytical chemistry with minimal overlap and maximum
application coverage. Same set as in [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §3.1,
repeated here with application mapping:

| Ch. | λ (nm) | Source | Principal applications |
|---|---|---|---|
| **CH0** | 340 | UV LED (Bivar UV5TZ-340, default) | NADH, NADPH (ethanol, glucose, lactate, ATP), nicotinamide-coupled enzyme assays |
| **CH1** | 430 | Violet LED | Beer EBC color, bilirubin Soret band, chlorophyll b, wine OD420 |
| **CH2** | 470 | Blue LED | Carotenoids, blue dye indicators, Bradford secondary |
| **CH3** | 525 | Green LED | Iron-phenanthroline, DNS sugars, cytochrome c, permanganate |
| **CH4** | 590 | Amber LED | Bradford protein (595 nm primary), bromocresol green, OD600 cell density |
| **CH5** | **650** | **User's 650 nm laser** | EBC haze standard, methylene blue, chlorophyll a, iodine-starch, Free chlorine DPD |
| **CH6** | 740 | Deep red LED | Folin-Ciocalteu polyphenols, some phosphate methods, wine OD620 |
| **CH7** | 940 | NIR LED | Turbidity reference (no visible absorbers), phosphate molybdenum blue (880 nm primary), water haze |

**Extended channels** (beyond the base 8, for specific applications that
need different wavelengths):

| Ext. | λ (nm) | Source | Applications |
|---|---|---|---|
| **CX1** | 260 | UV-C LED (Bolb or Klaran, €15 – €30) | DNA/RNA quantification (A260), UV protein (A280) |
| **CX2** | 275 | UV-C LED | Beer bitterness (IBU via iso-octane extract) |
| **CX3** | 405 | UV LED (near-UV) | Bilirubin, hemoglobin, SDS-PAGE |
| **CX4** | 860 | NIR LED | ISO 7027 water turbidity standard |
| **CX5** | 1060 | NIR LED | Some process NIR applications |

The base 8 channels use the standard optical head; extended channels
plug into a second expansion port on the same head and are enabled on
an application-specific basis.

---

## 7. Hardware design — base station

### 7.1 Block diagram

```
  ┌───────────────────────────────────────────────────────────────┐
  │                     BASE STATION ENCLOSURE                    │
  │                                                               │
  │   ┌──────────────┐      ┌────────────────────────┐           │
  │   │ phycommander │      │  POWER DISTRIBUTION    │           │
  │   │ (Arduino Due)│      │  5 V (USB), 3.3 V (µC),│           │
  │   │              │      │  ±12 V (DC-DC SIM1-0512)│          │
  │   └──┬─────────┬─┘      └─────────┬──────────────┘           │
  │      │         │                  │                          │
  │      │         │                  ├───▶ optical head power   │
  │      │         │                  ├───▶ electrochem mod power│
  │      │         │                  └───▶ actuator board power │
  │      │         │                                             │
  │      │         └──────────┐                                  │
  │      │                    │                                  │
  │  ┌───┴───────┐      ┌─────▼──────┐                           │
  │  │  PRIMARY  │      │  SAFETY    │                           │
  │  │ TIA (OPA  │      │ INTERLOCK  │                           │
  │  │ 381), PD  │      │ (relay,    │                           │
  │  │ S1227-    │      │ microswit.)│                           │
  │  │ 1010BR    │      │            │                           │
  │  │           │      │ disables   │                           │
  │  │ (shared   │      │ lasers in  │                           │
  │  │ by all    │      │ firmware   │                           │
  │  │ optical   │      │ AND relay  │                           │
  │  │ heads)    │      │            │                           │
  │  └───────────┘      └────────────┘                           │
  │                                                               │
  │  ┌───────────────┐   ┌───────────────┐                       │
  │  │  EXPANSION    │   │  EXPANSION    │                       │
  │  │  HEADER A     │   │  HEADER B     │                       │
  │  │ (20-pin IDC)  │   │ (20-pin IDC)  │                       │
  │  │               │   │               │                       │
  │  │ 4 analog      │   │ 4 analog      │                       │
  │  │ 4 digital     │   │ 4 digital     │                       │
  │  │ 8 GPIO        │   │ 8 GPIO        │                       │
  │  │ power rails   │   │ power rails   │                       │
  │  │ I²C           │   │ I²C           │                       │
  │  └───────────────┘   └───────────────┘                       │
  │                                                               │
  └───────────────────────────────────────────────────────────────┘
```

### 7.2 Universal photodiode + TIA front-end

Same design as [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §4.3: Hamamatsu
S1227-1010BR photodiode + OPA381 op-amp + R_f = 10 MΩ + C_f = 1 pF + mid-rail
bias. This front-end is **shared by all optical heads** — the heads contain
only the light sources and the optical geometry, not the detector.

An additional secondary TIA channel (ADC1) is provided for heads that need
a second detector (fluorescence emission, 90° scatter, etc.).

### 7.3 Source driver stage

Eight logic-level MOSFETs (2N7000) on a small breakout board driven by
phycommander's PWM/DAC/DOUT channels per §5.1 of the beer analyzer doc.
Current-limit resistors sized per LED (§4.4 of same). Laser enable path
is gated by the safety interlock.

### 7.4 Safety interlock

Hardware + firmware implementation per [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §4.6.
Adds a **physical relay** (G5V-1, SPST, 5 V coil) on the laser power line
*in addition to* the firmware-level gating. Belt and suspenders: if the
firmware fails, the relay still opens when the cover microswitch is open.

### 7.5 Power distribution

- **5 V @ 500 mA**: directly from USB (phycommander power). Runs the LEDs,
  the laser, the digital circuits. Total draw ~150 mA worst case.
- **3.3 V @ 200 mA**: from phycommander's onboard regulator. Runs the TIA
  op-amp (rail-to-rail output, single supply).
- **±12 V @ 100 mA**: from a Meanwell SIM1-0512 DC-DC converter (~€8).
  Runs the high-impedance InAmp for pH/ORP (needs bipolar supply for
  mid-rail input common-mode) and the Peltier H-bridge gate drive.

### 7.6 PCB or perfboard

- **Phase 1 build**: everything on perfboard with point-to-point wiring.
  Fast to assemble, easy to debug. Acceptable for the first validation
  paper.
- **Phase 2**: custom PCB in KiCad, 2-layer, ~80 × 100 mm. Has connectors
  for all modules, status LEDs, microswitch. Cost ~€10 – €30 per
  prototype from JLCPCB or OSH Park.

---

## 8. Hardware design — detachable modules

### 8.1 Optical head: absorbance + nephelometry

Same as [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §4.2 – §4.6, but with an
additional 90° port that holds a second photodiode and TIA for haze
measurement. The 8 sources (7 LEDs + 1 laser) are arranged in an
illumination ring around the cuvette. A PTFE rod immediately before the
primary photodiode acts as an optical mixer.

**Interface to base station**: 20-pin IDC cable carrying 8 gate signals,
TIA outputs from both photodiodes, power, and I²C identification.

### 8.2 Optical head: fluorescence

A dedicated fluorescence head with:

- **Excitation source**: choice of modulated LED or laser in the excitation
  arm. For chlorophyll: 430 nm LED. For NADH: 340 nm LED. For fluorescein:
  470 nm LED. Modulated at 1 kHz from PWM0 with a MOSFET driver.
- **Excitation lens**: a simple plano-convex lens (f = 20 mm, €2) focuses
  the excitation light into the cuvette center.
- **Emission arm at 90°**: a second photodiode collects emitted light.
- **Long-pass filter**: in front of the emission photodiode to block
  scattered excitation. Thorlabs FEL-series filters (€40 – €80 each,
  different cut-on wavelengths). For chlorophyll: FEL0600 (600 nm LP).
- **Reference photodiode**: looks at a small back-reflection of the
  excitation source to monitor source intensity; used for ratiometric
  correction.
- **Baffles**: black matte interior, mechanical baffles between the
  excitation and emission arms to reduce stray light.

**Electrical interface**: 20-pin IDC same as absorbance head, but uses
different signal assignment (excitation driver + emission TIA + reference
TIA).

### 8.3 Optical head: multi-angle static light scattering

A cuvette holder with fixed scattering ports at 30°, 45°, 60°, 90°,
120°, 150°. Six laser sources (user's 650 nm modules) are mounted
opposite each port, each modulated at a different frequency (within the
base 8 carriers, reassigned). A single photodiode at a 0° forward
position detects the cumulative scattered light, and the lock-in bank
separates the angular components.

Alternative layout: a single source at 0° and six photodiodes at the six
angles. This requires six TIAs — more hardware but simpler optically.
Choose the former (1 detector, N modulated sources) to leverage the
multi-channel lock-in architecture.

**Calibration**: polystyrene latex nanoparticle standards. The angular
scattering profile is fit to Mie theory (for particles ~λ/10 or larger)
or Rayleigh (for smaller particles).

### 8.4 Optical head: crossed-beam Doppler

Two laser beams from two of the user's 650 nm modules are focused to
cross at a small angle (10° – 20°) in the sample volume. A single
photodiode at ~10° off the forward axis collects backscattered light.
The Doppler-modulated signal is processed by the host with an FFT to
extract velocity.

The crossing angle is set mechanically via a precision adjustable mount.
Once fixed, it determines the fringe spacing and hence the velocity
calibration. No electronic calibration needed.

### 8.5 Optical head: flow cell

Same optical layout as the absorbance head, but with a flow cell instead
of a cuvette. The flow cell is a 3D-printed body with two windows (glass
or PMMA), inlet/outlet tubing, and a 1 cm path length. Flow is driven by
the actuator module's peristaltic pump.

### 8.6 Electrochemical module

A PCB (or perfboard) carrying:

- **pH/ORP input**: INA116 instrumentation amplifier, input impedance
  > 10¹⁵ Ω, gain 1 or 10 configurable. BNC connector for the electrode.
  Output to ADC via a low-pass filter (~20 Hz).
- **Conductivity driver**: one DAC channel provides an AC 1 kHz excitation;
  a precision 10 kΩ sense resistor converts current to voltage; a second
  ADC channel reads the voltage across the sense resistor. Lock-in
  demodulation on host.
- **Potentiostat (for amperometry)**: OPA2192 (dual rail-to-rail) in a
  standard 3-electrode potentiostat configuration. Working electrode
  potential set by DAC; current measured on another op-amp as
  voltage-to-current converter. Outputs to ADC.
- **Reference voltage**: REF3030 (3.0 V precision reference) for
  calibrated measurements.
- **I²C ID EEPROM**: 24AA02 for module identification.

Connects via expansion header A.

### 8.7 Actuator module

A PCB carrying:

- **Stepper driver**: A4988 or DRV8825, controlled by 2 GPIO (STEP + DIR)
  + 1 GPIO for enable. Drives a NEMA 17 stepper for the cuvette turret.
- **DC motor driver**: DRV8871 H-bridge, controlled by 2 PWM channels
  (or 1 PWM + 1 GPIO for direction). Drives stirrer or Peltier.
- **Peristaltic pump driver**: second DRV8871 or similar, driving a 5 V
  or 12 V peristaltic pump.
- **Peltier H-bridge**: BTS7960 for large Peltiers (up to 40 W); DRV8871
  for small ones. Bidirectional current for heating and cooling.
- **Fan control**: MOSFET-switched 12 V fan output for Peltier hot side.
- **Tachometer input**: 1 GPIO for reading motor tachometer (optical or
  hall effect).
- **Safety**: overcurrent protection (polyfuse) on each output; thermal
  shutdown via onboard NTC on the Peltier driver.
- **I²C ID EEPROM**: 24AA02.

Connects via expansion header B.

### 8.8 External sensor connectors

The base station has 4 additional 3.5 mm audio jacks on the rear panel
for external sensor inputs, each wired to a different ADC channel with
its own signal conditioning:

- **Jack 1**: pH electrode (BNC → 3.5 mm via adapter, or dedicated BNC)
- **Jack 2**: temperature probe (NTC or PT100, 3-wire)
- **Jack 3**: user-defined analog input (0 – 3.3 V, low impedance)
- **Jack 4**: user-defined (0 – 10 V via divider, higher impedance)

These allow the instrument to accept any commercial probe with a
compatible output.

---

## 9. Mapping to phycommander I/O

### 9.1 Full configuration with all modules

| phycommander | Direction | Function | Module |
|---|---|---|---|
| **DAC0** | out | 740 nm source (via comparator), OR AC conductivity drive | optical OR electrochemical |
| **DAC1** | out | 940 nm source (via comparator), OR potentiostat set point | optical OR electrochemical |
| **PWM0** | out | 340 nm source (1013 Hz) | optical |
| **PWM1** | out | 430 nm source (1709 Hz) | optical |
| **ADC0** | in | Primary TIA output (transmittance / absorbance) | base station |
| **ADC1** | in | Secondary TIA output (90° scatter / fluorescence emission / Doppler / conductivity sense) | base station (shared) |
| **ADC2** | in | pH electrode via InAmp, OR ORP, OR ISE | electrochemical |
| **ADC3** | in | Cuvette temperature (NTC on holder) | base station |
| **ADC4** | in | Potentiostat current sense | electrochemical |
| **ADC5** | in | External probe 1 (pH / T / user) | external jack |
| **ADC6** | in | External probe 2 (pH / T / user) | external jack |
| **ADC7** | in | DAC0 loopback (self-test) | base station |
| **DOUT0** | out | 470 nm software-toggled carrier (571 Hz) | optical |
| **DOUT1** | out | 525 nm software-toggled carrier (881 Hz) | optical |
| **DOUT2** | out | 590 nm software-toggled carrier (1223 Hz) | optical |
| **DOUT3** | out | 650 nm laser carrier (1439 Hz), interlock-gated | optical |
| **DOUT4** | out | Laser master enable (interlock-gated) | base station |
| **DOUT5** | out | Status LED green | base station |
| **DOUT6** | out | Status LED amber | base station |
| **DOUT7** | out | Status LED red | base station |
| **DOUT8** | out | Stepper STEP | actuator |
| **DOUT9** | out | Stepper DIR | actuator |
| **DOUT10** | out | Stepper ENABLE | actuator |
| **DOUT11** | out | Peristaltic pump IN1 (H-bridge direction) | actuator |
| **DOUT12** | out | Peristaltic pump IN2 / PWM speed | actuator |
| **DOUT13** | out | Peltier direction (heat/cool) | actuator |
| **DOUT14** | out | Peltier PWM / heat enable | actuator |
| **DOUT15** | out | Stirrer PWM / enable | actuator |
| **DIN0** | in | Stepper encoder A (turret position) | actuator |
| **DIN1** | in | Stepper encoder B | actuator |
| **DIN2** | in | Stepper home sensor | actuator |
| **DIN3** | in | User start button | base station |
| **DIN4** | in | User "tare" button | base station |
| **DIN5** | in | Safety interlock (cover closed) | base station |
| **DIN6** | in | External trigger in | base station |
| **DIN7** | in | Pump flow tachometer | actuator |
| **DIN8** | in | Stirrer tachometer | actuator |
| **DIN9–15** | in | reserved | — |

**All ADC and DAC used. 2/2 PWM used. 16/16 DOUT used. 9/16 DIN used
(7 reserved).** This is the full configuration; minimal configurations
leave many channels unused.

### 9.2 Configuration tiers

| Tier | Modules | phycommander usage |
|---|---|---|
| **Starter** | Base station + absorbance head | 6 ADC, 2 DAC, 2 PWM, 8 DOUT, 2 DIN |
| **Extended** | + electrochemical | 8 ADC, 2 DAC, 2 PWM, 10 DOUT, 2 DIN |
| **Full** | + actuator | **all channels** (see §9.1) |
| **Maxed** | + multi-angle head + Doppler head (swapped in) | same as full; heads rotate in and out |

---

## 10. Bill of materials (tiered)

### 10.1 Base station

| Item | Part | Qty | Unit (€) | Total |
|---|---|---|---|---|
| Arduino Due | Arduino A000062 | 1 | 40 | 40 |
| Photodiode Hamamatsu S1227-1010BR | S1227-1010BR | 1 | 30 | 30 |
| TIA op-amp OPA381 | OPA381AIDBVR | 1 | 5.50 | 5.50 |
| Resistors / caps (TIA + dividers) | — | 1 lot | 3 | 3 |
| DC-DC ±12 V (Meanwell SIM1-0512) | SIM1-0512 | 1 | 8 | 8 |
| Comparators (TLV3201) | TLV3201 | 2 | 2 | 4 |
| Relay (laser safety) | Omron G5V-1 | 1 | 2 | 2 |
| Microswitch (interlock) | Omron D2F-01L | 1 | 1.50 | 1.50 |
| Status LEDs + resistors | — | 3 | 0.15 | 0.45 |
| Expansion headers (2× IDC 20-pin) | — | 2 | 0.50 | 1 |
| Enclosure (3D print) | — | 1 | 10 | 10 |
| PCB / perfboard | — | 1 | 10 | 10 |
| **Base station subtotal** | | | | **~€115** |

### 10.2 Absorbance optical head

| Item | Part | Qty | Unit (€) | Total |
|---|---|---|---|---|
| LED 340 nm | Bivar UV5TZ-340 | 1 | 8 | 8 |
| LEDs 430, 470, 525, 590, 740, 940 nm | various | 6 | 0.80 | 4.80 |
| Laser 650 nm (user-supplied) | — | 1 | 0 | 0 |
| MOSFETs 2N7000 | — | 8 | 0.30 | 2.40 |
| Gate resistors | — | 8 | 0.02 | 0.16 |
| LED current limit resistors | — | 7 | 0.05 | 0.35 |
| Secondary TIA (90° port) for haze | — | 1 | 5 | 5 |
| Secondary photodiode | BPW34 | 1 | 1 | 1 |
| PTFE rod mixer | — | 1 | 3 | 3 |
| Cuvette holder (3D print) | — | 1 | 5 | 5 |
| **Absorbance head subtotal** | | | | **~€30** |

### 10.3 Fluorescence optical head

| Item | Part | Qty | Unit (€) | Total |
|---|---|---|---|---|
| Excitation LED (application-specific) | various | 1 | 3 | 3 |
| Focusing lens (plano-convex 20 mm) | Thorlabs LA1074 or Aliexpress | 1 | 3 | 3 |
| Long-pass emission filter (Thorlabs FEL) | FEL0600 or similar | 1 | 50 | 50 |
| Emission photodiode | BPW34 or S1227 | 1 | 1–30 | 15 avg |
| Emission TIA | OPA381 + passives | 1 | 6 | 6 |
| Reference photodiode + small TIA | — | 1 | 3 | 3 |
| Baffle set | — | 1 | 2 | 2 |
| Head body (3D print) | — | 1 | 5 | 5 |
| **Fluorescence head subtotal** | | | | **~€85** |

### 10.4 Multi-angle nephelometer head

| Item | Part | Qty | Unit (€) | Total |
|---|---|---|---|---|
| Lasers 650 nm (user-supplied) | — | 6 | 0 | 0 |
| MOSFETs | 2N7000 | 6 | 0.30 | 1.80 |
| Current-limit resistors | — | 6 | 0.05 | 0.30 |
| Angular mount hardware | — | 1 set | 10 | 10 |
| Polystyrene latex standards (calibration) | Sigma 4205A-50 set | 1 | 120 | 120 |
| Head body (3D print) | — | 1 | 5 | 5 |
| **Multi-angle head subtotal (w/ standards)** | | | | **~€140** |

### 10.5 Doppler velocimetry head

| Item | Part | Qty | Unit (€) | Total |
|---|---|---|---|---|
| Lasers 650 nm (user-supplied) | — | 2 | 0 | 0 |
| Adjustable mounts (for beam crossing) | — | 2 | 10 | 20 |
| Focusing lenses | — | 2 | 3 | 6 |
| Detection photodiode + TIA | — | 1 | 6 | 6 |
| Head body | — | 1 | 5 | 5 |
| **Doppler head subtotal** | | | | **~€40** |

### 10.6 Electrochemical module

| Item | Part | Qty | Unit (€) | Total |
|---|---|---|---|---|
| InAmp INA116 (pH/ORP buffer) | INA116PA | 1 | 15 | 15 |
| Dual op-amp (OPA2192 for potentiostat) | OPA2192ID | 1 | 6 | 6 |
| Precision reference (REF3030) | REF3030 | 1 | 3 | 3 |
| Sense resistor (precision 0.1 % 10 kΩ) | — | 1 | 1 | 1 |
| BNC connectors (electrode inputs) | — | 3 | 2 | 6 |
| pH electrode (combination, with Ag/AgCl ref.) | Hamilton Flatrode or Sentek P11 | 1 | 40 | 40 |
| ORP electrode | Sensorex ORP-1400 | 1 | 35 | 35 |
| Ion-selective electrode (optional, e.g., Na⁺) | various | 1 | 80 | 0 (optional) |
| pH calibration buffers (4.01 / 7.00 / 10.01) | Hanna HI70007 | 1 set | 15 | 15 |
| KCl conductivity standards | Hanna HI7031 etc. | 1 set | 15 | 15 |
| PCB / perfboard | — | 1 | 5 | 5 |
| **Electrochemical module subtotal** | | | | **~€141** |

### 10.7 Actuator module

| Item | Part | Qty | Unit (€) | Total |
|---|---|---|---|---|
| Stepper driver A4988 | Pololu | 1 | 5 | 5 |
| NEMA 17 stepper motor | — | 1 | 12 | 12 |
| DC motor driver DRV8871 | TI | 2 | 3 | 6 |
| Peristaltic pump (12 V, 0.1 – 5 mL/min) | — | 1 | 20 | 20 |
| Stirrer motor (5 V DC + magnetic) | — | 1 | 8 | 8 |
| Peltier 40 W | TEC1-12706 | 1 | 6 | 6 |
| Peltier H-bridge BTS7960 | — | 1 | 6 | 6 |
| Heatsink + fan for Peltier hot side | — | 1 | 8 | 8 |
| Tachometer IR sensor | — | 1 | 2 | 2 |
| Turret (3D print) + bearing | — | 1 | 10 | 10 |
| PCB / perfboard | — | 1 | 5 | 5 |
| **Actuator module subtotal** | | | | **~€88** |

### 10.8 Reagents (application-dependent)

As in [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §6.3, plus extensions for
additional application domains:

| Application | Key reagents | Cost (~€) |
|---|---|---|
| Beer QC | Megazyme K-ETOH + Bradford + Folin + formazin | ~300 |
| Water quality | DPD (Cl), Griess (NO₂/NO₃), Nessler (NH₃), molybdenum blue (P), o-phen (Fe) | ~150 |
| Enzyme kinetics | ADH, GOD, LDH + NAD⁺/NADP⁺ | ~200 |
| Bioprocess | Protein Bradford + cell culture media | ~100 |
| Particle sizing | Polystyrene latex standards | ~120 |

### 10.9 Total cost tiers

| Tier | Contents | Cost (€) |
|---|---|---|
| **Starter** | Base station + absorbance head | **~€145** |
| **Beer QC** | Starter + beer reagents | ~€445 |
| **Extended** | Starter + electrochemical module (with pH + ORP electrodes + buffers) | ~€286 |
| **Full modular bench** | Extended + actuator module | ~€374 |
| **Full + fluorescence head** | Full + fluorescence head | ~€459 |
| **Full + multi-angle** | Full + multi-angle head + latex standards | ~€514 |
| **Full + Doppler** | Full + Doppler head | ~€414 |
| **Everything** | All of the above, all reagent kits | **~€850** |

At the **full modular bench** tier (~€374 hardware, no reagents), the
instrument covers all 16 measurement modes listed in §3 and can be
reconfigured for any of the application domains in §14 by swapping one
optical head and adding the appropriate reagents.

---

## 11. Firmware requirements

### 11.1 Phase 1 (v1.1, required for core modes)

Same as [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §7:

- Hardware PWM on PWM0, PWM1 with programmable frequency and duty cycle
- Firmware-level laser safety interlock (DIN5 hardware-gates DOUT3/DOUT4)
- Loop-synchronous DOUT toggling (existing behavior, verified)
- NTC temperature readout (no firmware change beyond reading an ADC
  channel)

### 11.2 Phase 2 (v1.2, required for advanced modes)

- **I²C master** (Wire / TWI peripheral exposure) for module identification
  EEPROMs and optional digital sensors
- **SPI master** (SPI peripheral exposure) for multi-channel DACs (octal
  DAC expansion) and fast external ADCs (ADS1256 24-bit, optional)
- **Hardware Timer/Counter for quadrature decoding** (TC peripheral) for
  the cuvette turret's optical encoder without eating host CPU
- **Additional PWM channels** (up to 6) for more independent carriers —
  removes the need for software DOUT toggling
- **Protocol extensions**:
  - `pwm[k].freq_hz`, `pwm[k].duty_u16`, `pwm[k].enable` for k ∈ [0, 5]
  - `i2c.address`, `i2c.write`, `i2c.read` commands
  - `spi.cs`, `spi.tx`, `spi.rx` commands
  - `tc[k].count`, `tc[k].index` status fields

### 11.3 Phase 3 (v1.3, nice-to-have)

- **Stepper motion controller** — offload step generation to firmware
  with acceleration ramping, so the host only issues "move to position
  X" commands
- **Hardware watchdog** on the safety interlock
- **External trigger capture** — timestamp external events on an input
  pin with microsecond resolution
- **USART bridge** to a dedicated UART on the SAM3X (for legacy probes
  or BLE modules)

### 11.4 Firmware is the critical path

Most of the modes in §3 depend on firmware v1.1 at minimum. Modes 3.8
(potentiometry), 3.9 (conductometry), 3.10 (amperometry) work with v1.1
for basic measurement but benefit from v1.2 for digital sensors and
higher-resolution ADC via SPI. Modes 3.12 – 3.15 (motion, temperature
control, titration, turret) work with v1.1 but are more pleasant with
v1.2's hardware quadrature decode and expanded PWM. Mode 3.7 (Doppler)
needs no additional firmware — the host does the FFT.

---

## 12. Host software architecture

### 12.1 Layered architecture

```
  ┌───────────────────────────────────────────┐
  │         Application UI (TUI / web)        │   <- user-facing
  ├───────────────────────────────────────────┤
  │       Application frameworks:             │
  │   beer_analyzer / water_analyzer /        │
  │   bioprocess / fermentation / lab_utility │
  ├───────────────────────────────────────────┤
  │       Mode controllers (one per §3):      │
  │   absorbance / kinetic / nephelometry /   │
  │   fluorescence / LDV / pH / conductivity /│
  │   amperometry / temperature / titration / │
  │   turret / flow                           │
  ├───────────────────────────────────────────┤
  │       Shared services:                    │
  │   lock-in engine (multi-channel) /        │
  │   FFT engine (Doppler) /                  │
  │   PID engine (temperature, stirring) /    │
  │   calibration database /                  │
  │   reporting / database / PDF export       │
  ├───────────────────────────────────────────┤
  │       Device abstraction:                 │
  │   phyclient IPC / module enumeration /    │
  │   hot-plug detection / safety watchdog    │
  ├───────────────────────────────────────────┤
  │       phycommander (physerver)            │
  └───────────────────────────────────────────┘
```

### 12.2 Module enumeration at startup

On connection, the host:

1. Verifies phycommander firmware version (must be ≥ 1.1).
2. Reads the I²C EEPROM on each expansion header to identify attached
   modules. Each EEPROM returns a 32-byte record containing
   `{module_type, module_version, manufacturer, calibration_date,
    per-channel_metadata}`.
3. Registers the available modes based on which modules are present.
4. Loads calibration constants for each registered mode from the local
   calibration database.
5. Presents the user with a menu of available measurement modes (only
   modes that the current hardware configuration supports).

### 12.3 Shared lock-in engine

The lock-in engine (see [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §8.2 for
the base class) is shared across all modes that use frequency-domain
demodulation — absorbance (8 channels), nephelometry (1 – 6 channels),
fluorescence (1 – 2 channels), conductometry (1 channel), amperometry
(1 channel if modulated). Each mode registers the channels it needs and
consumes the resulting `(R, φ)` stream.

### 12.4 Mode switching

Switching between modes at runtime involves:

1. Stopping the current mode's controller
2. Reconfiguring phycommander (new PWM frequencies, new DOUT toggling
   pattern, new ADC routing)
3. Reconfiguring the lock-in engine (new channel list)
4. Starting the new mode's controller

This takes ~100 ms. Not instantaneous but fast enough that a user can
switch modes between samples without reopening the cuvette chamber.

### 12.5 Data persistence

All measurements are logged to a local SQLite database
(`~/.phycommander/analyzer.db`) with schema:

```
TABLE measurements (
  timestamp TEXT,
  sample_id TEXT,
  mode TEXT,                -- from §3.1 – §3.16
  channel TEXT,             -- for multi-channel modes
  raw_value REAL,           -- pre-calibration
  calibrated_value REAL,    -- post-calibration
  unit TEXT,                -- mg/L, % v/v, EBC, pH, ...
  calibration_id INTEGER,   -- FK to calibration table
  temperature_C REAL,       -- ambient / cuvette
  operator TEXT,
  notes TEXT
)

TABLE calibrations (
  id INTEGER PRIMARY KEY,
  mode TEXT,
  channel TEXT,
  date TEXT,
  slope REAL,
  intercept REAL,
  r_squared REAL,
  standard_lot TEXT,
  operator TEXT
)

TABLE samples (
  sample_id TEXT PRIMARY KEY,
  description TEXT,
  source TEXT,
  collection_date TEXT,
  notes TEXT
)
```

### 12.6 Reporting

- **Real-time**: live plot of the current measurement in the UI
- **Post-measurement CSV**: one row per measurement appended to a CSV
- **PDF report**: one report per sample containing all measurements for
  that sample, with calibration metadata and signed data hash
- **LabCAT-compatible export** (optional): for integration with Laboratory
  Information Management Systems

---

## 13. Calibration strategy

Every mode in §3 has an analogous 5-stage calibration flow:

1. **Electronic self-test** — verify the analog / digital chain without
   the physical sensor (DAC → ADC loopback for optical, reference voltage
   for electrochemical, known resistance for temperature).
2. **Noise floor characterization** — establish the instrument's limit
   of detection under typical operating conditions.
3. **Blank / baseline** — the reference against which sample measurements
   are normalized (water for absorbance, neutral buffer for pH, air or
   pure water for conductivity, etc.).
4. **Linearity verification** — a serial dilution of a known standard
   spans the instrument's range; the fit (R², residual plot) confirms
   linearity.
5. **Per-assay calibration curve** — the actual calibration used for
   converting instrument readings to reported units.

Calibration results are stored in the database (§12.5). The software
refuses to report a value if the calibration for the active mode is older
than a mode-specific expiry time (30 days for chemical assays, 7 days for
pH, 1 day for amperometric O₂, etc.).

A **calibration wizard** guides the operator through each stage with
step-by-step instructions, expected inputs, and pass/fail criteria.

---

## 14. Application domains

### 14.1 Beer / wine / spirits QC

Full details in [`BEER_ANALYZER.md`](BEER_ANALYZER.md). Covers ethanol,
color, haze, protein, polyphenols, sugars, bitterness (extended), iron,
pH, titratable acidity. ~8 parameters per sample, ~10 minutes per
sample, ~€3 per sample in reagents.

### 14.2 Water quality monitoring

Target parameters: pH, conductivity, turbidity, free chlorine, nitrate,
nitrite, ammonia, phosphate, iron, manganese, temperature, dissolved
oxygen, hardness (via titration). Uses modes 3.1, 3.3, 3.8, 3.9, 3.11,
3.14 in combination.

**Reagents**: €150 starter kit covers all colorimetric tests for ~100
samples. **Reference methods**: EPA, Standard Methods, DIN.

**Deployment**: benchtop in a lab, or as a field portable kit with the
base station powered from a USB battery bank.

### 14.3 Bacterial / cell culture monitoring

Parameters: OD600 (cell density, mode 3.1), dissolved O₂ (Clark
electrode, mode 3.10), pH (mode 3.8), temperature (mode 3.11),
temperature control (mode 3.12), stirring (mode 3.13), optional
fluorescence (mode 3.5) for GFP-tagged strains.

**Typical application**: bench-scale fermenter monitoring, E. coli or
yeast batch fermentation. Logs all parameters to the database at 1 Hz,
enabling post-hoc analysis of growth curves, metabolic rates, and
response to induction.

### 14.4 Enzyme kinetics lab

Parameters: absorbance at 340 nm over time (NADH production / consumption)
with temperature control at 37 °C. Supports all NAD-linked enzyme assays
(ADH, LDH, GOD, GAPDH, ...). Initial-rate method (slope of the first
30 s) and full Michaelis-Menten curve fitting for Km and Vmax.

**Reagents**: enzyme-specific, typically €50 – €200 per enzyme from Sigma
or Megazyme.

### 14.5 Nanoparticle characterization

Parameters: angular scattering profile (multi-angle head, mode 3.4),
possibly DLS via temporal correlation of a single-angle signal.
Calibrated with NIST-traceable polystyrene latex standards.

**Typical application**: characterizing synthesized silver / gold
nanoparticles, monitoring protein aggregation, particle size QC of
pharmaceutical formulations.

### 14.6 Fluid velocimetry (Doppler)

Parameters: flow velocity via laser Doppler (Doppler head, mode 3.7).
Bench demonstrations of fluid mechanics (channel flow, jet dispersion),
microfluidic QC, capillary flow characterization.

### 14.7 Chlorophyll / photosynthesis research

Parameters: chlorophyll absorbance (CH1 at 430 nm and CH5 at 650 nm) and
chlorophyll fluorescence (fluorescence head, mode 3.5, λ_ex 430 nm,
λ_em 680 nm). Kinetic measurement of photobleaching under strong
illumination; photosynthetic efficiency (Fv/Fm) with a programmable
light pulse protocol.

### 14.8 Acid-base titration

Parameters: pH electrode reading during a controlled reagent addition.
Full titration curve captured in real time, first derivative used to
identify equivalence points. Supports single-endpoint, multi-endpoint,
and back-titration protocols.

**Typical application**: Titratable Acidity of wine, total acid of beer,
carbonate/bicarbonate alkalinity of water, salt chloride content by
Mohr's method.

### 14.9 Environmental / field analysis

Parameters: water quality as in §14.2, but in a portable configuration.
The base station + absorbance head + pH electrode, powered by a 10 Ah
USB battery bank, can run for 8 hours of continuous operation.

### 14.10 Process analytics

Parameters: any of the above, but in a flow cell configuration with
continuous data logging. Inline monitoring of a bioreactor effluent, a
chromatography column outlet, a water treatment plant process stream,
etc.

### 14.11 Chemistry teaching lab

Parameters: one instrument replaces the lab's spectrophotometer,
nephelometer, fluorometer, pH meter, conductivity meter, temperature
controller, and titrator. Students can run the full panel of classical
quantitative chemistry experiments without needing to share or switch
between instruments.

---

## 15. Safety

### 15.1 Consolidated safety rules

1. **Laser Class 3R handling** per [`BEER_ANALYZER.md`](BEER_ANALYZER.md)
   §11.1. Applies to all optical heads using the 650 nm lasers
   (absorbance/nephelometer haze channel, multi-angle, Doppler).
2. **UV LED eye safety** per [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §11.2.
   Applies to heads using the 340 nm UV LED. Extended UV-C LEDs (260 nm,
   275 nm) require additional precautions: these wavelengths are strongly
   absorbed by the cornea and can cause arc eye / photokeratitis.
3. **Chemical handling** — reagents for Folin-Ciocalteu (acid), Bradford
   (staining), DNS (irritant), nitric acid (in Griess, Nessler), and
   phenol derivatives (in some ammonia methods) all require gloves, lab
   coat, and eye protection. See §15.4 for a per-reagent summary.
4. **Electrical safety** — the instrument runs on 5 V USB + low-power
   ±12 V DC-DC. No mains-voltage circuits anywhere. The laser relay
   isolates the laser power line but the logic itself is all 3.3 – 5 V.
5. **Thermal safety** — Peltier modules can reach > 100 °C on the hot
   side if the heatsink fan fails. The firmware reads a temperature
   sensor on the Peltier heatsink and shuts down the Peltier output if
   the temperature exceeds 60 °C.
6. **Motion safety** — the actuator module's motors run at low voltage
   and low torque (NEMA 17 at 12 V, ~0.4 N·m). Cannot cause injury even
   on entanglement, but can spill sample containers if unexpectedly
   activated. Enclosed motion stages mitigate this.

### 15.2 Interlock wiring

```
  +3.3V ── 10 kΩ pull-up ──┬── DIN5 (to phycommander firmware)
                           │
                           ├── (for external logging / debug)
                           │
                      ┌────┴────┐
                      │ micro-  │
                      │ switch  │ (normally open, closed when cover closed)
                      │         │
                      └────┬────┘
                           │
                          GND
                           │
                           │
             +5V laser ────┼─── (laser supply)
                           │
                      ┌────┴────┐
                      │  RELAY  │ (G5V-1, SPST, 5 V coil driven by DOUT4)
                      │  NO     │
                      └────┬────┘
                           │
                           │
                           └─── to laser master enable MOSFET (DOUT3 modulates)
```

Belt-and-suspenders: the relay (mechanical, fail-safe) and the MOSFET
(electronic, software-controlled) are in series. Either one being open
disables the laser. Both must be closed for the laser to emit.

### 15.3 Firmware interlock pseudocode

```c
// Runs every firmware main-loop iteration, ~1 kHz
void safety_loop(void) {
    static bool interlock_ok = false;
    bool cover_closed = (DIN5_read() == 0);  // active-low

    if (!cover_closed) {
        // Force laser off regardless of host command
        PWM_set_duty(PWM0_LASER, 0);
        DOUT_clear_bit(DOUT3);  // laser modulation
        DOUT_clear_bit(DOUT4);  // laser master
        RELAY_disable();         // mechanical relay
        interlock_ok = false;
        status_flags |= INTERLOCK_OPEN;
    } else if (!interlock_ok) {
        // Re-arm requires explicit host command, not just cover close
        // (prevents "spring-back" activation)
        if (host_requested_rearm()) {
            interlock_ok = true;
            status_flags &= ~INTERLOCK_OPEN;
        }
    }

    if (interlock_ok) {
        // Normal operation: host commands take effect
        apply_host_pwm_commands();
        apply_host_dout_commands();
    }
}
```

### 15.4 Reagent safety summary

| Reagent | Hazard | PPE | Waste disposal |
|---|---|---|---|
| Folin-Ciocalteu | Acidic, irritant | Gloves, goggles | Lab waste |
| Bradford Coomassie | Staining | Gloves, lab coat | Water-dilutable |
| Dinitrosalicylic acid | Irritant | Gloves | Lab waste |
| Nitric acid (Griess) | Strong acid | Gloves, goggles, hood | Neutralize, lab waste |
| Phenol (ammonia salicylate method) | Toxic, flammable | Gloves, goggles, hood | Phenol waste stream |
| o-Phenanthroline | Mildly toxic | Gloves | Lab waste |
| Ethanol (calibration) | Flammable | Gloves | Dilute and drain |
| Methylene blue | Staining | Gloves | Lab waste |
| Heavy metal standards (Fe, Mn, Cu, Pb) | Toxic | Gloves | Hazardous waste only |
| pH / conductivity buffers | Low hazard | None specific | Drain |
| NAD⁺, NADH, enzymes | Low hazard | Gloves (aseptic) | Lab waste |

### 15.5 Pre-power-on checklist

Same as [`BEER_ANALYZER.md`](BEER_ANALYZER.md) §11.4, plus:

- [ ] Correct optical head mounted and identified by I²C (green LED on
      that head)
- [ ] Electrochemical module's electrodes rinsed and in buffer if
      mounted
- [ ] Actuator module's motor mechanical range clear of obstruction
- [ ] Reagent waste container in place and not full
- [ ] Peltier heatsink fan running if temperature control mode will be
      used

---

## 16. Publication plan & roadmap

### 16.1 Multi-paper roadmap

This is not one paper — it is (at least) six papers that together
describe the full instrument and its major modes. Each can stand alone
as a HardwareX publication, and readers of any single paper will be
able to build the corresponding configuration without needing the others.

| Paper | Topic | Based on | Status |
|---|---|---|---|
| **1** | Base instrument + absorbance head (8-channel multispectral photometer) | §3.1, §7, §8.1, §10 | Draft in progress |
| **2** | Beer QC application of paper 1 | [`BEER_ANALYZER.md`](BEER_ANALYZER.md) | Draft in progress |
| **3** | Fluorescence head | §3.5, §8.2 | Design only |
| **4** | Multi-angle nephelometer head | §3.4, §8.3 | Design only |
| **5** | Electrochemical module (pH / conductivity / amperometry) | §3.8 – §3.10, §8.6 | Design only |
| **6** | Actuator module (temperature control / titration / turret) | §3.12 – §3.15, §8.7 | Design only |
| **7** | Master system paper: integrated multi-function bench | This document | Design only |

Paper 7 is the capstone — it describes how all modules compose into the
multi-function bench, and it depends on papers 1 – 6 being published
first (or concurrently). Papers 1 and 2 can be submitted together in
Year 1; papers 3 – 6 in Year 2; paper 7 in Year 3.

### 16.2 Build roadmap

| Phase | Goal | Duration | Depends on |
|---|---|---|---|
| **0** | Design documents complete | ✓ | — |
| **1** | phycommander firmware v1.1 (PWM + interlock) | — | — |
| **2** | Base station perfboard build | 2 weeks | Phase 1 |
| **3** | Absorbance head build | 1 week | Phase 2 |
| **4** | First measurement (methylene blue linearity) | 1 day | Phase 3 |
| **5** | First beer ethanol assay | 1 week | Phase 4 + reagents |
| **6** | Cross-validation on 10 commercial beers | 2 weeks | Phase 5 |
| **7** | Papers 1 + 2 drafted | 4 weeks | Phase 6 |
| **8** | Custom PCB for base station + absorbance head | 3 weeks | Phase 7 |
| **9** | phycommander firmware v1.2 (SPI + I²C + TC) | — | — |
| **10** | Electrochemical module build | 2 weeks | Phase 9 |
| **11** | Actuator module build | 2 weeks | Phase 10 |
| **12** | Fluorescence head build | 2 weeks | Phase 10 |
| **13** | Multi-angle head build | 3 weeks | Phase 10 |
| **14** | Doppler head build | 3 weeks | Phase 10 |
| **15** | Papers 3 – 6 drafted | 12 weeks | Phases 10 – 14 |
| **16** | Master system paper | 8 weeks | Phase 15 |
| **17** | Long-term: community contributions, application notes, workshops | ongoing | — |

---

## 17. Appendices

### Appendix A: Acronym list

| Acronym | Meaning |
|---|---|
| ADC | Analog-to-digital converter |
| ADH | Alcohol dehydrogenase |
| BOM | Bill of materials |
| DAC | Digital-to-analog converter |
| DAQ | Data acquisition |
| DMA | Direct memory access |
| DLS | Dynamic light scattering |
| EBC | European Brewery Convention |
| ENBW | Equivalent noise bandwidth |
| FFT | Fast Fourier transform |
| GAE | Gallic acid equivalents |
| IBU | International bitterness units |
| ISE | Ion-selective electrode |
| LDV | Laser Doppler velocimetry |
| NAD/NADH | Nicotinamide adenine dinucleotide (ox./red.) |
| NTC | Negative temperature coefficient (thermistor) |
| OPA | Operational amplifier |
| ORP | Oxidation-reduction potential |
| PCB | Printed circuit board |
| PID | Proportional-integral-derivative |
| PREEMPT_RT | Linux real-time kernel patch |
| PWM | Pulse-width modulation |
| SLS | Static light scattering |
| SRM | Standard Reference Method (beer color, American) |
| TIA | Transimpedance amplifier |
| TEC | Thermoelectric cooler (Peltier) |
| TDS | Total dissolved solids |
| TC | Timer / Counter peripheral (SAM3X) |

### Appendix B: Key reference equations

**Beer-Lambert**: `A(λ) = ε(λ) · c · l`

**Nernst (pH at 25 °C)**: `E = E₀ − 59.16 mV × log₁₀(a_H+) = E₀ + 59.16 mV × pH`

**Conductance / conductivity**: `σ = G · K` where K is the cell constant

**Lock-in ENBW (boxcar, N samples)**: `ENBW = 1 / (N · Ts) = Fs / N`

**Doppler shift**: `f_D = 2 v sin(θ/2) / λ`

**Fringe spacing (crossed beams)**: `d = λ / (2 sin(θ/2))`

**Mie scattering (large particle, forward)**: see Bohren & Huffman ch. 3

**Poisson shot noise on photodiode**: `σ_shot = √(2 q I_photo B)` where
q is elementary charge, I_photo is DC photocurrent, B is bandwidth

### Appendix C: Related documents

- [`LOCKIN_OPTICAL_DEMO.md`](LOCKIN_OPTICAL_DEMO.md) — Minimal lock-in
  demonstrator. Good introduction before reading this document.
- [`BEER_ANALYZER.md`](BEER_ANALYZER.md) — Single-application build guide,
  more detail than §14.1 here.
- Phycommander firmware update roadmap:
  [`../firmware/FIRMWARE_UPDATES.md`](../firmware/FIRMWARE_UPDATES.md)
- Phycommander protocol reference:
  [`../technical/PROTOCOL.md`](../technical/PROTOCOL.md)
- Phycommander architecture:
  [`../technical/ARCHITECTURE.md`](../technical/ARCHITECTURE.md)

### Appendix D: References (for §10 and §14 methods)

1. EBC Analytica, *European Brewery Convention Analytica Methods*, current
   edition.
2. ASBC *Methods of Analysis*, American Society of Brewing Chemists.
3. *Standard Methods for the Examination of Water and Wastewater*, APHA.
4. Bradford, M. M. (1976). "A rapid and sensitive method for the
   quantitation of microgram quantities of protein utilizing the
   principle of protein-dye binding". *Anal. Biochem.* 72: 248 – 254.
5. Folin, O.; Ciocalteu, V. (1927). "On tyrosine and tryptophane
   determinations in proteins". *J. Biol. Chem.* 73: 627 – 650.
6. Miller, G. L. (1959). "Use of dinitrosalicylic acid reagent for
   determination of reducing sugar". *Anal. Chem.* 31: 426 – 428.
7. Bohren, C. F.; Huffman, D. R. (1983). *Absorption and Scattering of
   Light by Small Particles*. Wiley.
8. Durst, H. D. et al. (1990). "Evaluation of iron-1,10-phenanthroline
   complex formation for the determination of iron". *Microchem. J.* 42:
   190 – 195.
9. IEC 60825-1 (2014). *Safety of laser products — Part 1: Equipment
   classification and requirements*.

---

*End of document. Questions, errata, contributions → issues on the
phycommander repository. This document is the master vision for the
multi-function analytical bench; specific application configurations are
described in their own dedicated sub-documents.*
