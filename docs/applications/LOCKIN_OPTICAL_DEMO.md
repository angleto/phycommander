# Lock-In Optical Spectrometer — phycommander Demo Application

> A reference application that uses **every** I/O resource phycommander
> exposes (and is planned to expose) on the Arduino Due (ATSAM3X8E) to build
> an open-hardware software lock-in amplifier for educational optical
> spectroscopy.

---

## 1. TL;DR

Modulate an LED (or low-power laser diode) with a sinusoidal reference
generated on **DAC0** at frequency *f₀* (≈1 kHz). Read the light transmitted
through a sample with a photodiode + transimpedance amplifier on **ADC0**.
On the host side, demodulate coherently against `sin(2π f₀ t)` and
`cos(2π f₀ t)` using the *same sample clock* that drives the DAC. The result
is a measurement of optical absorbance with > 60 dB rejection of room-light
flicker, sunlight, and op-amp 1/f noise — using €5–20 of components.

This demo is the natural showcase for phycommander because the operation
that defines a lock-in amplifier — *coherent multiplication of an output
reference against an input sample* — is exactly the operation that
phycommander does better than any other open DAQ in its class:
**sample-accurate, deterministic, near-zero phase noise between DAC and
ADC**, thanks to the PREEMPT_RT host loop and the firmware DMA front-end.

The base demo uses 2 channels (DAC0 + ADC0). The full demo, described in
this document, uses every available DAC, PWM, ADC, and GPIO that
phycommander exposes in v2.0.0 to turn the same instrument
into a 4-channel colorimeter, a Bode plotter, and a motor-chopped reference
spectrometer — all with one continuous firmware load and one host
application.

---

## 2. Why this demo (and not the others considered)

| Idea | Uses synchronous DAC↔ADC RT loop? | Publication angle | Verdict |
|---|---|---|---|
| Solar tracker (photodiodes → servo) | No (slow servo) | Saturated, no novelty | Reject |
| Free-space laser data link | Partially | Niche; bandwidth limited by 10 kHz Fs | Defer |
| Vibration energy harvesting (piezo + iron balls) | Almost not at all (DC accumulation) | Hard to make reproducible | Reject |
| Bike-mounted piezo strip tuning | Same | Field-test logistics kill the demo | Reject |
| **Software lock-in optical spectrometer** | **Yes — this *is* the operation phycommander is best at** | HardwareX / EJP / AJP / J. Open Hardware | **Pick** |

A lock-in is the textbook example used to teach coherent detection in
every EE/physics curriculum. Building one with a generic open DAQ — and
showing that the recovered signal is unaffected by ambient light or mains
hum — is *visually* compelling, *pedagogically* meaningful, and
*technically* demanding in exactly the way that justifies a real-time
architecture.

---

## 3. Optical lock-in in 60 seconds

Let `s(t) = A·sin(2π f₀ t)` be the reference driving the LED current. The
LED emits an optical intensity `L(t) = L₀ + k·s(t)`. After traversing a
sample with transmittance `T`, the photodiode receives
`L(t)·T + L_ambient(t)`. The TIA produces a voltage

```
v(t) = G · T · (L₀ + k·s(t)) + n(t)
```

where `n(t)` is room-light flicker, mains hum, op-amp noise, and the
photodiode dark current.

The host multiplies `v(t)` by the *same* `sin(2π f₀ t)` it just sent to
DAC0, and integrates over `N` samples. Cross-terms with frequencies
different from `f₀` integrate to zero. What remains is proportional to
`T`, with an equivalent noise bandwidth ≈ `1/(N·Ts)`. For `Fs = 10 kHz`
and `N = 10000`, the noise bandwidth is **≈ 1 Hz** — a noise reduction
factor of √(5000/1) ≈ 70 vs. a naive DC measurement on the same hardware.

The key requirement is that the reference used for demodulation must be
**phase-coherent** with the reference sent to the DAC. Phycommander
provides this for free: the same host loop that emits the DAC sample
reads back the ADC sample of the same period, and the firmware DMA
buffer guarantees the two are aligned to one sample clock (~100 µs).

---

## 4. Hardware bill of materials

| Item | Notes | Cost (EUR) |
|---|---|---|
| Arduino Due | already required by phycommander | — |
| White LED (or low-power red laser diode) | sample illumination | 0.50 |
| Photodiode (BPW34 or similar) | 7.5 mm² wide-spectrum Si photodiode | 1.00 |
| Op-amp (TL072 or OPA381) | TIA front-end | 0.50 |
| Feedback 1 MΩ + 4.7 pF cap | TIA gain & stability | 0.10 |
| LED current-limit resistor | sized for ~10 mA at 1.65 V mid-rail bias | 0.05 |
| 9 V battery + linear reg. (or USB 5 V) | TIA bias supply | 1.00 |
| Cuvette / glass tube / shot glass | sample holder | 1.00 |
| Black paint / cardboard enclosure | stray-light suppression | 0.50 |
| *(opt)* 2 more LEDs (RGB) | for the multi-channel extension §8.1 | 1.00 |
| *(opt)* Piezo disc + small mass | for the Bode plotter extension §8.2 | 2.00 |
| *(opt)* Small DC motor + IR slot encoder | for the optical chopper extension §8.3 | 5.00 |
| *(opt)* BLE module (HC-05 / HM-10) | wireless display, §8.4 | 4.00 |

**Base demo total: ~€5.** Full demo with all extensions: **~€20**.

The TIA front-end is the only "real" analog circuit and is a one-stage
inverting transimpedance amplifier with a single feedback RC pair. The
output is biased to 1.65 V (mid-rail of phycommander's 0–3.3 V ADC range)
using a half-rail resistor divider on the non-inverting input.

---

## 5. Mapping to phycommander I/O

This is the section where the demo earns its *"uses every Arduino Due
port"* claim. The default phycommander v1.0 firmware exposes 8 ADC, 2 DAC,
16 DIN, 16 DOUT, and (planned for v1.1) 2 PWM. The tables below assign
**every one of those channels to a useful function in the demo**. Channels
marked **[base]** are required for the minimal lock-in demo; **[ext]**
unlock the §8 extensions; **[opt]** are nice-to-have.

### 5.1 DAC outputs

| Logical ch. | Signal | Frequency | Purpose | Tier |
|---|---|---|---|---|
| **DAC0** | sine, full-range, mid-rail biased | f₁ ≈ 1000 Hz | Modulation of primary LED / laser source | **base** |
| **DAC1** | sine, full-range, mid-rail biased | f₂ ≈ 1700 Hz (incommensurate with f₁) | Modulation of second LED → 2nd colorimeter channel | **ext** |

The two DAC frequencies are deliberately **incommensurate** (irrational
ratio, e.g. 1000 Hz and 1700 Hz, *not* 1000 Hz and 2000 Hz) so that
lock-in cross-channel leakage stays below −50 dB after 1 s of integration.

### 5.2 PWM outputs (planned in firmware v1.1)

| Logical ch. | Mode | Purpose | Tier |
|---|---|---|---|
| **PWM0** | 16-bit duty, square wave, f₃ ≈ 5000 Hz | Modulation of third LED. Lock-in extracts the fundamental. | **ext** |
| **PWM1** | 16-bit duty, ~20 kHz carrier | Speed control of optical chopper motor / cuvette stirrer | **ext** |

Two notes on PWM in this demo:

1. **PWM-modulated LED as a third lock-in channel.** A square wave at f₃
   has its energy mostly in the fundamental (`4/π`) plus odd harmonics.
   The lock-in projects onto `sin(2π f₃ t)` and ignores the harmonics
   (provided f₃ and its odd multiples don't fall on f₁ or f₂). This is
   the standard trick used in commercial multi-wavelength fluorometers
   and gives you a 3rd colorimeter channel for free, on top of the two
   DAC channels.
2. **PWM as motor control** does *not* need to be coherent with the ADC
   loop. It just needs to set a reproducible angular frequency for the
   optical chopper. The chopper's *true* phase is then recovered
   electrically from the encoder on DIN0/DIN1, and that becomes a
   *fourth* reference (see §8.3).

### 5.3 ADC inputs (8 channels — all used)

| Logical ch. | Source | Purpose | Tier |
|---|---|---|---|
| **ADC0** | TIA #1 output | Primary photodiode — light transmitted through sample | **base** |
| **ADC1** | source pickoff | Sense resistor on the LED current, *or* a back-of-LED photodiode. Provides the *true* reference for ratiometric measurement, cancels source intensity drift | **base** |
| **ADC2** | TIA #2 output | Second photodiode at 90° to source — scattered light → turbidimetry / nephelometry channel | **ext** |
| **ADC3** | NTC / LM35 on cuvette | Temperature, used to compensate the LED's −2 mV/°C drift and the photodiode's dark-current temperature dependence | **ext** |
| **ADC4** | Microphone preamp | Acoustic input for §8.2 Bode-plotter extension (Helmholtz resonator characterization) | **ext** |
| **ADC5** | Piezo cantilever pickoff | Mechanical input for §8.2 Bode-plotter extension (mechanical resonance characterization) | **ext** |
| **ADC6** | Ambient-light photodiode | Looks at the room with the source blocked. Provides a slow baseline of ambient light that the host subtracts — *defense in depth*, on top of the lock-in demodulation | **opt** |
| **ADC7** | DAC0 readback | A wire from DAC0 to ADC7. Closes the loop on the actual DAC voltage and lets the self-test verify analog integrity each session | **opt** |

### 5.4 Digital outputs (16 channels)

| Logical ch. | Connected to | Purpose | Tier |
|---|---|---|---|
| **DOUT0** | Green status LED | "System ready" indicator | **base** |
| **DOUT1** | Amber status LED | "Measurement in progress" | **base** |
| **DOUT2** | Red status LED | "ADC saturated" or "fault" | **base** |
| **DOUT3** | Laser enable (MOSFET / SSR) | Hardware-gated by safety interlock on DIN5; refused unless cover closed | **base** |
| **DOUT4** | LED ring power | Powers the optional reference LED ring (§8.4) | **opt** |
| **DOUT5** | Stirrer relay | Cuvette magnetic stirrer (if present) | **opt** |
| **DOUT6** | Filter wheel select bit 0 | 2-bit selection of one of 4 optical filters (digital filter wheel) | **ext** |
| **DOUT7** | Filter wheel select bit 1 | idem | **ext** |
| **DOUT8** | External scope trigger out | One pulse per integration window — syncs an oscilloscope or another instrument | **opt** |
| **DOUT9** | BLE module reset (HC-05 / HM-10) | Re-arms the BLE module that broadcasts measurements wirelessly (§8.4) | **opt** |
| **DOUT10** | Audible buzzer | Beeps when measurement converges within tolerance | **opt** |
| **DOUT11..DOUT15** | reserved | Expansion | — |

### 5.5 Digital inputs (16 channels)

| Logical ch. | Source | Purpose | Tier |
|---|---|---|---|
| **DIN0** | Encoder A (chopper wheel) | Quadrature phase A — chopper rotation reference for §8.3 | **ext** |
| **DIN1** | Encoder B (chopper wheel) | Quadrature phase B | **ext** |
| **DIN2** | Encoder index Z | Once-per-revolution pulse — exact phase reference for the chopper lock-in | **ext** |
| **DIN3** | Cuvette presence microswitch | Refuses to enable laser unless a cuvette is in place | **base** |
| **DIN4** | "Start measurement" button | User control — debounced in software | **base** |
| **DIN5** | Safety interlock (cover-closed switch) | Hardware-gates the laser enable — if open, DOUT3 is forced low *in firmware*, not just on the host | **base** |
| **DIN6** | "Tare / Zero baseline" button | Captures current reading as the absorbance reference (T = 1.000) | **base** |
| **DIN7** | External trigger in | Lets an upstream instrument start a measurement run | **opt** |
| **DIN8** | Pump/stirrer tachometer | Optional flow-rate feedback | **opt** |
| **DIN9..DIN15** | reserved | Expansion | — |

### 5.6 Channel summary by tier

| Tier | DAC | PWM | ADC | DOUT | DIN |
|---|---|---|---|---|---|
| Base demo only | 1/2 | 0/2 | 2/8 | 4/16 | 4/16 |
| + §8 Extensions | 2/2 | 2/2 | 6/8 | 7/16 | 7/16 |
| + Optional features | **2/2** | **2/2** | **8/8** | 11/16 | 8/16 |

The full demo, with all three extensions and the optional features, uses
**every channel that phycommander v1.1 will expose**: 2/2 DAC, 2/2 PWM,
8/8 ADC, 11/16 DOUT (5 reserved for expansion), 8/16 DIN (8 reserved for
expansion). The 13 unused digital lines are intentional headroom for
end users to add their own peripherals without modifying the host
application.

---

## 6. Arduino Due peripherals not yet exposed by phycommander

For completeness, this section lists the Arduino Due (ATSAM3X8E)
peripherals that are *not yet* exposed by phycommander v1.0/v1.1 and how
the demo could benefit from each one in a future firmware version.

| Peripheral | Native count on SAM3X8E | phycommander exposure | Demo benefit if exposed |
|---|---|---|---|
| ADC channels | 12 (12-bit, ~1 MSPS aggregate) | 8 | 4 extra channels: 4-LED RGBW colorimeter, second TIA at different gain |
| DAC channels | 2 | 2 | none (already full) |
| PWM peripheral | up to 8 PWMH/L pins via PWM controller | 2 (planned v1.1) | More PWM-modulated LEDs, stepper for filter wheel |
| TC (Timer/Counter) | 9 channels (3 TCs × 3) | none | Hardware quadrature decode for the chopper encoder; frees CPU; better than DIN polling |
| USART | 4 (Serial1/2/3 + UART) | none (USB CDC only) | Direct connection to BLE module → no need for the DOUT9 reset trick; also enables RS-485 lab bus |
| TWI / I²C | 2 (Wire, Wire1) | placeholder | Direct readout of digital lux/colour sensors (TCS34725, BH1750, AS7341) for cross-validation |
| SPI | 1 (4-wire ICSP) | placeholder | High-speed external 16/24-bit ADC (ADS1256) for a "high-resolution mode" |
| CAN | 1 (needs transceiver) | none | Multi-instrument lab bus; sync with other phycommanders |
| USB host | yes | none (device only) | Direct USB barcode reader for sample identification |
| EBI / external memory | yes | none | External SDRAM for long capture buffers |
| RTC | yes | none | Timestamping of measurements without depending on host clock |

The first three rows are the **high-value extensions** for this demo and
form the natural roadmap for phycommander v1.2.

---

## 7. Software architecture (host side)

The host application is added under `phyclient/` as a new binary or
example target named `lockin_demo`. Its architecture has three layers:

```
+---------------------+
|   UI / TUI / web    |   <-- ratatui or warp; not on the RT loop
+---------------------+
|    demod thread     |   <-- consumes ADC frames, runs lock-in math, emits R, phi
+---------------------+
|  phyclient stream   |   <-- 64-byte packets at 10 kHz, SCHED_FIFO, mlockall
+---------------------+
          USB
+---------------------+
|  phycommander fw    |
+---------------------+
```

The lock-in demodulator is ~150 lines of Rust. The hot loop:

```rust
struct Lockin {
    f0: f64,    // reference frequency, Hz
    fs: f64,    // sample rate, Hz
    n: usize,   // integration window in samples
    phase: f64, // running phase accumulator (cycles, 0..1)
    i_acc: f64, // in-phase accumulator
    q_acc: f64, // quadrature accumulator
    k: usize,   // sample counter inside window
}

impl Lockin {
    fn step(&mut self, adc_sample: i16) -> Option<(f64, f64)> {
        let s = (2.0 * std::f64::consts::PI * self.phase).sin();
        let c = (2.0 * std::f64::consts::PI * self.phase).cos();
        self.i_acc += adc_sample as f64 * s;
        self.q_acc += adc_sample as f64 * c;
        self.phase += self.f0 / self.fs;
        if self.phase >= 1.0 { self.phase -= 1.0; }
        self.k += 1;
        if self.k >= self.n {
            let r = (self.i_acc.powi(2) + self.q_acc.powi(2)).sqrt() / self.n as f64;
            let phi = self.q_acc.atan2(self.i_acc);
            self.k = 0;
            self.i_acc = 0.0;
            self.q_acc = 0.0;
            Some((r, phi))
        } else {
            None
        }
    }
}
```

The DAC sample emitted to phycommander on the **same loop iteration** is
generated from *the same phase accumulator*:

```rust
let dac_sample = (2047.0 * (2.0 * PI * self.phase).sin() + 2048.0) as u16;
```

That single shared accumulator is what guarantees coherence: the DAC
sample and the demodulator's reference are identical sample by sample
(modulo the firmware DAC propagation delay, which is constant and folds
into a fixed phase offset that the host calibrates out at startup).

For the multi-channel extension, each LED has its own `Lockin` instance
with its own `f0`, all sharing the same `phyclient` stream and the same
ADC0 input.

---

## 8. Extensions

### 8.1 Three-channel colorimeter (single photodiode)

Drive three LEDs (R, G, B) with three independent reference frequencies
on three different output peripherals:

* **DAC0** → red LED, f₁ = 1000 Hz (sine)
* **DAC1** → green LED, f₂ = 1700 Hz (sine)
* **PWM0** → blue LED, f₃ = 5000 Hz (square; lock-in extracts the fundamental)

A single photodiode (ADC0) sees the *sum* of the three modulated lights.
Three lock-in instances on the host separate the channels with crosstalk
< −50 dB after 1 s of integration.

**Demo experiment:** an iodine–starch reaction that turns blue over time.
The blue channel's amplitude tracks the reaction kinetics in real time;
the red and green channels track absorbance changes too, giving a cheap
multi-wavelength kinetics monitor. Or: monitor a wine sample turning into
vinegar over hours/days.

### 8.2 Bode plotter / Frequency Response Analyser

Replace the LED with whatever physical "system under test" you like:

* a small loudspeaker driving a microphone in a tube → Helmholtz
  resonator characterization (mic on **ADC4**)
* a piezo disc with a small mass on top, excited by an electromagnet
  driven via DAC0 → mechanical resonance (piezo on **ADC5**)
* an op-amp filter circuit → measure the transfer function on ADC0

Sweep DAC0 through a logarithmic frequency grid (e.g. 30 Hz → 4 kHz, 50
points/decade). For each frequency, run the lock-in for 1 s and record
`(R, φ)`. Plot Bode magnitude and phase. You now have a Bode 100 clone
worth ~€4000 commercially.

### 8.3 Optical chopper with motor + encoder

The classical lock-in setup uses a *mechanical* chopper rotating in
front of the source. **PWM1** drives a small DC motor; the encoder on
**DIN0/DIN1** (with hardware-decoded quadrature in firmware v1.2, or
software-decoded in v1.1) provides the *true* phase reference. The
DAC0-generated reference is replaced by a software-generated
`sin(2π f_chopper t)` whose phase is locked to the encoder index pulse
on **DIN2**.

This extension is the demo where the motors and encoders earn their
place. It also lets you compare *electronic* lock-in (LED-modulated, DAC
reference) against *mechanical* lock-in (chopper, encoder reference) on
the **same instrument** with two configurations, side by side. That
comparison alone is worth a figure in the paper.

### 8.4 BLE remote display (optional)

A HC-05/HM-10 BLE module wired to one of the future USART pins (or, in
v1.1, controlled with brutal simplicity via DOUT9 reset and a software
serial bridge in `phyclient`) broadcasts the current `(R, φ, T_sample)`
to a phone. Useful for the live demo: put the cuvette in a fume hood or
under a UV lamp and read the result on your phone from outside the
shielded area.

---

## 9. Validation experiments (the figures of the paper)

1. **SNR vs integration time.** With the LED off, measure the noise
   floor of `R` for `N` from 100 to 100000 samples. Fit `1/√N`. Goal:
   noise floor at the digitization limit (~0.8 mV·rms / √N) with no
   excess noise from PREEMPT_RT jitter.
2. **Ambient light rejection.** Put the cuvette under a fluorescent
   ceiling lamp; under direct sunlight; under a flickering LED bulb.
   Compare `R` to the dark measurement. Goal: < 1 % deviation.
3. **Beer–Lambert verification.** Make a serial dilution of food dye in
   water. Plot `−log₁₀(T)` vs concentration. Goal: linear, R² > 0.999
   over 2–3 decades of concentration.
4. **Phase stability over hours.** Run the lock-in continuously for 8 h.
   Plot `φ(t)`. Goal: drift < 1° over 8 h, dominated by TIA temperature,
   not by phycommander jitter.
5. **Crosstalk (multi-channel extension).** Drive only the red LED;
   measure the recovered amplitude on the green and blue channels.
   Goal: crosstalk < −50 dB after 1 s integration.
6. **Mechanical vs electronic lock-in (extension §8.3).** Plot SNR vs
   modulation frequency for both modes on the same sample. Show that the
   electronic mode is limited by op-amp 1/f noise below 100 Hz and the
   mechanical mode is limited by chopper jitter above ~500 Hz.

---

## 10. Publication target

**Primary target: HardwareX** (Elsevier, peer-reviewed, open hardware,
mandates a complete BOM and reproducible build instructions). Secondary
targets: **Journal of Open Hardware**, **American Journal of Physics**
(instructional angle), **European Journal of Physics**.

The novelty claim is *not* "a lock-in amplifier" — those exist by the
hundreds. It is **"a generic open DAQ with deterministic sample-coherent
DAC↔ADC streaming, demonstrated by reproducing the lock-in technique
without any specialized analog hardware"**. The figures from §9 supply
the quantitative evidence; the validation against ambient-light scenarios
supplies the visual punch for the demo video.

---

## 11. Safety, calibration, and out of scope

* **Eye safety.** If you use a laser instead of an LED, stay below
  Class 1 (≤ 0.39 mW for 400–700 nm continuous). The cuvette and the
  enclosure must form a contained beam path. The DOUT3 / DIN5 interlock
  pair is *not optional* if a laser is used: implement the gating in
  firmware so that the laser cannot turn on if the cover switch is open
  even if the host application is misbehaving.
* **TIA bias drift.** The op-amp's input bias current and offset voltage
  drift with temperature. Compensation via ADC3 is *the* nontrivial
  calibration step.
* **DC linearity is not characterized by the lock-in.** Lock-in measures
  *amplitude at f₀*, not absolute DC. Absolute calibration is done with
  reference samples (water blank for T = 1.0, opaque blank for T = 0.0),
  triggered by the "Tare" button on DIN6.
* **Out of scope for this demo:** fluorescence (would need optical
  filters and a different illumination geometry); UV (LED choice and
  photodiode spectral response); polarization (would need polarizers).

---

## 12. Roadmap

| Phase | Deliverable | phycommander dependency |
|---|---|---|
| 1 | Base demo: DAC0 → LED → photodiode → ADC0 lock-in | v1.0 (already shipped) |
| 2 | Two-channel colorimeter (DAC0 + DAC1) | v1.0 (already shipped) |
| 3 | Three-channel colorimeter (+ PWM0) | **v1.1: PWM in firmware** |
| 4 | Bode plotter mode | v1.0 (already shipped) |
| 5 | Optical chopper + encoder reference (PWM1, DIN0–DIN2) | **v1.1: PWM + already-supported GPIO** |
| 6 | Hardware quadrature decode for the chopper | **v1.2: TC peripheral exposure** |
| 7 | Direct BLE / I²C digital sensor support | **v1.2: USART or TWI exposure** |
| 8 | Submission to HardwareX | — |

---

*Document version: draft 1, 2026-04-10. Author: Angelo Leto. Intended
license: CERN-OHL-S v2 alongside phycommander when ready for publication.*
