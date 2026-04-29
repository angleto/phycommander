# Piezo Bode + Energy Harvester (concept)

> A scaled-down replica of an industrial vibration test rig. A piezo
> cantilever with a tip mass is excited from its base by a programmable
> waveform; the host measures the open-circuit response, reconstructs the
> mechanical transfer function H(f) by lock-in demodulation, finds the
> resonance, and quantifies harvestable power against real-world vibration
> spectra (bicycle frame, train floor, building floor) loaded from a CSV
> file.

This demo is the mechanical analogue of
[`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md). The same coherent
DAC↔ADC streaming that enables the optical lock-in is used here to
characterize a piezo resonator and to qualify it as a vibration energy
harvester for a given input spectrum.

---

## 1. TL;DR

Drive **DAC0** with a swept sine through an audio amplifier into a small
voice coil. The coil shakes the base of a piezo bender; the piezo tip mass
moves with its own mechanical dynamics. **ADC0** reads the piezo
open-circuit voltage. **ADC1** reads the voltage across a sense resistor in
series with the coil, providing the input "force" reference.

On the host, a Python notebook runs a coherent lock-in at each swept
frequency to recover the complex transfer function `H(f) = piezo / coil`
with > 60 dB out-of-band rejection. Once `H(f)` is known, the same notebook
loads a CSV recording of a real vibration source and computes the harvested
power on a given load resistance:

```
P_harvest(R_load) = ∫ |H(f)|² · S_input(f) · g(f, R_load) df
```

where `g(f, R_load)` is the piezo equivalent-circuit transfer to electrical
power. The host then sweeps `R_load`, finds the optimum, and drives **DOUT0**
to switch in a discrete bank of resistors to verify the prediction in
hardware.

The industrial parallel is a shock-absorber test bench: drive a hydraulic
actuator with a controlled profile, measure damper response, fit a model,
optimize. Same workflow, same math, three orders of magnitude smaller.

---

## 2. Why this demo (vs a bare Arduino sketch)

What Python on the host makes trivial:
- `numpy.fft` and `scipy.signal.welch` for spectral analysis.
- CSV / HDF5 / NumPy I/O for loading reference vibration spectra recorded
  on a phone or downloaded from public datasets.
- `scipy.optimize.minimize_scalar` for the matched-load search.
- `matplotlib` / `bokeh` / `plotly` for live transfer-function plots that
  update at every sweep step.
- Iteration in seconds: tweak the lock-in window, the sweep grid, or the
  harvester model; rerun the cell; see new plots. No compile-and-flash
  cycle.

What PhyCommander adds on top:
- DAC0 (sweep) and ADC0/ADC1 (response) are sample-coherent at 8 kHz on the
  same iso-USB microframe. Phase between drive and measurement is constant
  modulo a fixed firmware-side latency, calibrated out at startup.
- The on-chip generator allows phase-continuous frequency updates: re-issue
  `play_builtin/sine` with a new `freq_hz` and the phase accumulator is
  preserved (see
  [`../../user-guide/FUNCTION_GENERATOR.md`](../../user-guide/FUNCTION_GENERATOR.md)
  §2.1). The sweep is glitch-free.

What a bare Arduino sketch would face: hand-rolled DDS, no FFT (insufficient
FPU on Cortex-M3), no host file I/O, no plotting, and no straightforward way
to validate against external datasets.

---

## 3. Hardware bill of materials

Through-hole / DIP preferred, in line with the project's hand-solder rule.
Most of these are likely already in the operator's parts bin.

| Qty | Item | Notes | Approx cost |
|-----|------|-------|------|
| 1   | Piezo bender (PZT-5H, 30 to 50 mm, brass-shimmed) | bimorph or unimorph | 1 to 3 € |
| 1   | Tip mass (M2 brass nut + screw, or steel ball) | 1 to 5 g, glued or screwed at the free end | 0.10 € |
| 1   | Cantilever clamp | wooden block + screw, or 3D-printed | 0 |
| 1   | Voice coil + magnet (small speaker driver with cone removed, OR coil + neodymium magnet) | acts as base shaker | 1 to 3 € |
| 1   | Audio amplifier module (PAM8403 or LM386, mono) | drives the coil from DAC0 | 1 to 2 € |
| 1   | Sense resistor 1 Ω 1 W | series with the coil for current readback | 0.10 € |
| 1   | Op-amp buffer (TL072 DIP-8) | high-Z buffer between piezo and ADC0 (10 MΩ series, 100 nF for mid-rail biasing) | 0.50 € |
| 4-8 | Load resistors (100, 1k, 10k, 100k, 1M Ω) | switched bank for max-power verification | 0.20 € |
| 1   | DIP analog mux (CD4051) or small relay | DOUT-selected load resistor | 1 € |

**Total**: about 5 to 8 € on top of the base PhyCommander bench.

---

## 4. Mapping to phycommander I/O

### 4.1 Outputs

| Channel | Mode | Role |
|---|---|---|
| **DAC0** | `play_builtin/sine`, `freq_hz` updated per sweep step | drive of the audio amplifier into the voice coil |
| **DOUT0..DOUT2** | manual, 3-bit code | selects one of 8 load resistors via mux |
| **DOUT3** | manual, "amp enable" | mutes the coil drive between sweep steps |
| **DOUT15** | reactive `play_threshold` on ADC1 | hardware safety: forces amp mute if coil current exceeds preset, host-independent overload protection |

### 4.2 Inputs

| Channel | Source | Role |
|---|---|---|
| **ADC0** | piezo open-circuit voltage (after high-Z buffer, mid-rail biased) | response signal under test |
| **ADC1** | voltage across coil sense resistor | input force reference, for ratiometric H(f) |
| **ADC2** | NTC on the cantilever clamp | piezo PZT properties drift with temperature; logged for compensation |
| **DIN0** | "Start sweep" button | manual user control, debounced in Python |
| **DIN1** | "Mark interesting" button | tags the current spectrum in the CSV log |

This mapping uses 1 of 2 DACs, 0 of 8 PWM, 3 of 8 ADC, 5 of 16 DOUT, 2 of 16
DIN. It leaves room to add a second resonator on DAC1 (multi-mode
characterization) without rewiring.

---

## 5. Host software architecture

```
+---------------------------+
|  Jupyter notebook         |   sweep params, plot calls
+---------------------------+
|  bode_sweep.py            |   for each f in grid: set DAC0, lock-in N samples
|  lockin.py                |   I/Q demodulation, calibrated phase offset
|  harvester_model.py       |   piezo equivalent circuit, optimal R_load search
|  spectrum_loader.py       |   CSV / WAV import, resample to phycmd grid
+---------------------------+
|  phycmd (PyO3 client)     |   .play_builtin('sine', f, ...),
|                           |   .subscribe(on_frame) for ADC stream
+---------------------------+
            |
       physerver (Rust, RT)
            |
        SAM3X8E firmware
```

Dependencies on the host: `phycmd` (this repo,
[`../../../physerver/crates/phycmd-py/`](../../../physerver/crates/phycmd-py/)),
`numpy`, `scipy.signal`, `scipy.optimize`, `pandas` (CSV ingestion),
`matplotlib` or `plotly`, `jupyter`.

The lock-in core is the same code as in
[`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md) §7, generalized for
arbitrary input/output channel pairs.

---

## 6. Video plan

Target length: 3 to 4 minutes. Shot list:

1. **Cold open (10 s)**: close-up of the piezo cantilever vibrating, oscilloscope
   trace of the piezo voltage on a second monitor. No narration.
2. **The lab pitch (30 s)**: narrator explains the test-rig analogy, this is what
   you do when you qualify a vibration sensor or characterize a damper.
3. **First sweep (45 s)**: the operator runs one cell in the notebook. The H(f)
   plot appears. Resonance peak is obvious. Operator changes the sweep range
   in the next cell, reruns, gets a tighter view. Total elapsed: about 20 s.
4. **Real spectrum injection (60 s)**: drag a `bicycle_frame_vibration.csv`
   onto the notebook, run the harvester cell. The chart "harvested power vs
   load resistance" plots. Operator picks the optimal R, hardware switches to
   it via DOUT0, the recorded power matches the prediction within 10 %.
5. **Punchline (30 s)**: split screen, left is the Arduino IDE attempting the
   same exercise (no FFT, no CSV I/O, no plot), right is the Python
   notebook. The contrast sells itself.
6. **Outro (15 s)**: link to the next demo in the series.

---

## 7. Validation experiments

| # | Experiment | Pass criterion |
|---|---|---|
| 1 | Resonance Q-factor | `|H(f)|` peak has Q ≥ 30 (dominated by mechanical losses, not by lock-in noise) |
| 2 | Run-to-run repeatability | three consecutive sweeps agree on `f_resonance` within 0.5 % and on `|H(f_res)|` within 2 % |
| 3 | Predicted vs measured harvested power | for the optimal R_load chosen by the host model, measured RMS power on that resistor matches the prediction within ±10 % |
| 4 | Phase coherence over 8 hours | calibrated lock-in phase at `f_resonance` drifts less than 5° over 8 h continuous operation, dominated by piezo temperature, not by phycmd jitter |

---

## 8. Roadmap and extensions

- Replace single-frequency sweep with band-limited white-noise excitation +
  multi-tap correlation: trades sweep time for SNR.
- Add an external IMU on the clamp (I2C, future phycmd firmware) as an
  independent vibration reference.
- Export a one-page PDF report (`reportlab`) per session: H(f) plot,
  resonance, harvested power for each tested input spectrum.
- Fit a 1-DOF model `(m, c, k)` to H(f) via `scipy.optimize` and overlay the
  model on the data, demonstrating model-based design instead of pure
  characterization.

---

## 9. References

- [`../../user-guide/FUNCTION_GENERATOR.md`](../../user-guide/FUNCTION_GENERATOR.md):
  `play_builtin` JSON, glitch-free parameter updates, `play_threshold` for
  the hardware safety path.
- [`../../firmware/PROTOCOL.md`](../../firmware/PROTOCOL.md): wire format,
  status frame layout for ADC decoding.
- [`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md): the lock-in
  template that this demo extends to mechanical signals.
- [`../../../physerver/crates/phycmd-py/README.md`](../../../physerver/crates/phycmd-py/README.md):
  Python client API.

---

*Document version: concept draft 1, 2026-04-27. Status: concept (no code, no
hardware build yet). Implementation will land under `applications/piezo_bode_harvester/`
when started.*
