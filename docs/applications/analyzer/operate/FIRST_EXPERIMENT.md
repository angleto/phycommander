# First experiment — Methylene blue Beer-Lambert validation

> The very first run of a newly-built analyzer. One hour of your time,
> €2 of reagents, and it either passes or fails with a clear verdict.
> If it passes, you have a working quantitative instrument. If it fails,
> this document walks you through the diagnosis tree. **Do not skip this
> experiment before running any real sample**.

---

## What this experiment proves

The instrument's absorbance at **CH5 (650 nm laser)** responds **linearly
to a known analyte concentration** over at least **two decades**. Beer-
Lambert's law says `A = ε · c · l`, so a serial dilution should give a
straight line with intercept zero, slope = ε × l, and R² > 0.999.

If this works:

- The photodiode + TIA is operating in its linear range.
- The ADC is not saturating or clipping.
- The lock-in demodulation is extracting amplitude correctly.
- The calibration (blank subtraction, `−log₁₀(I/I₀)`) is correct.
- The stray light inside the enclosure is below 1 % of the signal.
- The source stability is good enough for quantitative work.

**This is the go/no-go gate** before running ethanol or any other
chemistry-dependent measurement. A failed methylene blue run means the
instrument is not ready, and running beer samples on it will produce
unreliable results.

---

## Why methylene blue

1. **Absorption peak is at 664 nm** — very close to our 650 nm laser.
   The effective molar absorption coefficient at 650 nm is ≈ 75 000
   M⁻¹ cm⁻¹ (vs 85 000 at the peak), which means even nanomolar
   concentrations give easily measurable absorbance.
2. **Cheap and available**. Methylene blue is sold as an aquarium fish
   medication (Kordon, Polysciences, Sigma `M9140`). A single €12 bottle
   of powder lasts a lifetime.
3. **Safe** at analytical concentrations. Light skin staining if spilled
   but non-toxic at the concentrations used here.
4. **Stable** in aqueous solution for weeks in the dark.
5. **Linear** over a wide range because the molecule doesn't aggregate
   or self-quench at the low concentrations we use.
6. **Textbook**. Every undergraduate lab uses it for Beer-Lambert
   verification — you are not inventing something new, you're
   replicating a well-characterized experiment.

---

## 1. What you need

### 1.1 Hardware

- [ ] The analyzer, fully assembled (base station + absorbance head)
- [ ] Interlock microswitch working (verify before powering the laser)
- [ ] Laser safety glasses, OD 2+ for 650 nm, on your face
- [ ] Host PC with the `analyzer` Python package installed (see
      `applications/analyzer/README.md`)
- [ ] phycommander firmware ≥ v1.1 flashed and running

### 1.2 Reagents

- [ ] Methylene blue powder (Sigma `M9140`, or fish medication)
- [ ] Distilled water (supermarket bottled or deionized from the lab)
- [ ] 5 clean 1 cm cuvettes (standard PS plastic is fine for 650 nm)
- [ ] 1 mL micropipette (ideally, or a good graduated 1 mL syringe)
- [ ] 10 mL graduated cylinder
- [ ] 50 mL or 100 mL volumetric flasks × 2
- [ ] Small (5 – 10 mL) vials × 7 for the dilution series
- [ ] Lab notebook / text file open next to you
- [ ] Kimwipes or lint-free lab wipes

### 1.3 Time

About 1 hour total:
- 10 min hardware warm-up and self-test
- 15 min dilution prep
- 15 min measurements
- 20 min analysis and plot

---

## 2. Stock solution

Prepare a **100 µM methylene blue** stock. This is the mother solution
from which every working dilution is made.

1. Weigh out **3.74 mg** of methylene blue powder (the molar mass of
   methylene blue hydrate is 373.9 g/mol).
2. Dissolve completely in **100 mL** of distilled water in a volumetric
   flask. Shake until all powder is dissolved (may take 2 – 3 minutes).
3. Wrap the flask in aluminum foil to exclude light. Label:
   `MB stock 100 µM, <date>, <your initials>`.

**Concentration check**: the stock at 100 µM and 1 cm path should have
A(650) ≈ 7.5, which is *way* above the ADC's dynamic range — do not try
to measure the stock directly. Always dilute.

Stock is stable at room temperature in the dark for ~1 month.

---

## 3. Dilution series

Prepare seven working solutions by serial dilution from the 100 µM
stock. Target concentrations: **0, 0.5, 1, 2, 5, 10, 20 µM**.

| Tube | Target [MB] (µM) | Stock volume (µL) | Water volume (µL) | Final volume (µL) |
|---|---|---|---|---|
| 0 | 0 (blank) | 0 | 5000 | 5000 |
| 1 | 0.5 | 25 | 4975 | 5000 |
| 2 | 1.0 | 50 | 4950 | 5000 |
| 3 | 2.0 | 100 | 4900 | 5000 |
| 4 | 5.0 | 250 | 4750 | 5000 |
| 5 | 10.0 | 500 | 4500 | 5000 |
| 6 | 20.0 | 1000 | 4000 | 5000 |

5 mL per tube is plenty for a 1 cm cuvette (which holds ~3 mL) with
leftover for checks.

**Procedure**:

1. Label all 7 tubes.
2. Add the water first (harder to over-pipette; minimizes risk of
   carryover).
3. Add the stock with a clean pipette tip for each tube.
4. Cap each tube and invert 3 – 4 times to mix. **Do not** vortex or
   shake vigorously — methylene blue foams.
5. Visually inspect: the series should show a clear progressive
   darkening from tube 0 (colorless) to tube 6 (dark blue). If tube 6
   looks identical to tube 5, you made a dilution error — redo.

---

## 4. Hardware preparation

### 4.1 Self-test

Before the laser touches a sample, verify the electronics are working.
Run the electronic self-test from `applications/analyzer/scripts/`:

```
cd applications/analyzer
python -m scripts.self_test
```

Expected output:

```
[self-test] Connecting to phycommander...
[self-test] Firmware version: 0x0101
[self-test] DAC0 → ADC3 loopback...
[self-test]   amplitude error: 0.3 % (PASS, threshold 2 %)
[self-test]   THD: 0.4 % (PASS, threshold 1 %)
[self-test] Dark noise floor on ADC0...
[self-test]   mean: 1.651 V (PASS, expected ~1.65 V)
[self-test]   stddev: 0.6 mV (PASS, threshold 1 mV)
[self-test]   50 Hz content: -72 dB (PASS, threshold -60 dB)
[self-test] Safety interlock loopback...
[self-test]   DOUT3 blocked by open cover: PASS
[self-test]   DOUT3 released by closed cover + re-arm: PASS
[self-test] ALL CHECKS PASS.
```

If any check fails, stop here. Consult the troubleshooting table in
§7 below.

### 4.2 Warm-up

Even after the self-test passes:

1. **Power on** the analyzer and **leave it running for 15 minutes**
   before the first measurement. The TIA op-amp has a non-zero
   temperature coefficient and the LEDs warm up to their steady-state
   optical output over several minutes. Skipping warm-up produces
   baseline drift that corrupts the calibration.
2. Keep the room lights constant. Close any blinds if direct sunlight
   can fall on the enclosure.
3. Put on your laser safety glasses even though the laser is inside a
   closed enclosure. Habit is safety.

### 4.3 Interlock check

Manually push the cover microswitch while the analyzer is idle (no
laser commanded). Verify the status bar on the host shows
`INTERLOCK: OPEN` → `INTERLOCK: OK` as you release. This confirms the
safety path is live.

---

## 5. Run the experiment

### 5.1 Set up the experiment script

```
python -m scripts.methylene_blue_run
```

This script:

1. Prompts you to insert the blank cuvette (tube 0).
2. Runs a 10-second lock-in measurement on CH5 (650 nm laser,
   modulated by DOUT3 at 1439 Hz).
3. Stores the result as `I0` (reference intensity).
4. Prompts for each of the 6 sample tubes in order.
5. For each, runs a 10-second lock-in measurement, computes `A = −log₁₀(I
   / I0)`, and stores the result.
6. Plots the data in real time.
7. Fits a linear regression `A = m · c` (forced through origin, since
   the blank is at c = 0 by definition).
8. Reports `m`, `R²`, and the residuals.

### 5.2 Step-by-step

For each measurement (blank + 6 samples):

1. **Fill the cuvette** with ~3 mL from the corresponding tube. Hold by
   the ribbed side, keep the clear faces clean.
2. **Wipe** the clear faces with a Kimwipe, top to bottom, once. No
   circular motions (leaves fiber marks).
3. **Insert** the cuvette into the holder with the clear faces aligned
   with the optical axis.
4. **Close** the cover and wait for the host to confirm
   `INTERLOCK: OK`.
5. **Press Enter** in the script to start the measurement.
6. **Wait** ~10 s. Do not open the cover. Do not touch the instrument.
7. **Record** the value (the script saves it automatically).
8. **Open** the cover, remove the cuvette, discard or save it.

Repeat for all 7 tubes.

### 5.3 Expected raw data

| Tube | [MB] (µM) | A(650) expected | Notes |
|---|---|---|---|
| 0 | 0 | 0.000 ± 0.002 | Baseline = noise floor. Must be < 0.005. |
| 1 | 0.5 | ≈ 0.038 | Just above noise |
| 2 | 1.0 | ≈ 0.075 | |
| 3 | 2.0 | ≈ 0.150 | |
| 4 | 5.0 | ≈ 0.375 | |
| 5 | 10.0 | ≈ 0.750 | |
| 6 | 20.0 | ≈ 1.500 | Approaching upper limit |

"Expected" values assume ε(650 nm) ≈ 75 000 M⁻¹ cm⁻¹ and 1 cm path.
Your actual slope will depend on the real ε for your specific
methylene blue batch and the exact wavelength of your laser, but the
**linearity** is what you are testing — not the absolute slope.

---

## 6. Pass / fail criteria

The script computes the linear regression and prints:

```
Regression: A = m * c
  m       = 0.0747  OD/(µM)
  R²      = 0.99985
  max residual: 0.003 OD
  residual stddev: 0.0014 OD

Result: PASS
```

### 6.1 Pass criteria (all must hold)

- **R² ≥ 0.999** over the range [0.5, 20] µM
- **Max residual ≤ 0.01 OD** in absolute value
- **Slope m** falls within ±10 % of the theoretical
  `ε × l = 75 000 × 0.01 = 750 M⁻¹ cm⁻¹ → 0.075 OD/µM`
  (so your slope should be between 0.068 and 0.083 OD/µM)
- **Blank reading ≤ 0.005 OD** in absolute value

### 6.2 What a pass means

**Your instrument is quantitative at 650 nm.** It is ready to measure
real samples:

- Beer haze at 650 nm (EBC 9.29 method)
- Methylene blue or any other blue dye
- Chlorophyll a in ethanol extract
- Iodine-starch reaction kinetics

And — more importantly — you now trust the entire chain: photodiode,
TIA, ADC, lock-in, calibration, blank subtraction. You can extend the
calibration to the other 7 channels with confidence that the
architecture itself is sound.

### 6.3 What a fail means

Do not be discouraged — first builds almost always have *one* issue
that this test catches. Go to §7 and follow the diagnosis tree.

---

## 7. Troubleshooting

### 7.1 Symptom: blank reading is not zero

**Expected**: A(blank) ≤ 0.005 OD in absolute value.
**Observed**: A(blank) = 0.05 OD or more.

**Causes and fixes**:

1. **Stray light leak into enclosure**. Cover a desk lamp with your
   hand and watch the A(blank) reading drop. If it drops by > 0.01 OD,
   the enclosure has a leak. Find it by shining a flashlight around
   the enclosure while watching the reading. Cover leaks with black
   tape.
2. **Cuvette is dirty**. Fingerprints on the clear faces produce
   absorbance. Re-wipe with a fresh Kimwipe.
3. **Air bubble in cuvette**. Hold the cuvette up to the light and
   check — a small bubble near the bottom is invisible at first glance
   but adds scattering. Re-fill carefully.
4. **Calibration of I₀ was done with the wrong cuvette**. The script
   uses the first cuvette you load as the blank. If you accidentally
   put sample tube 0 (water) but then the cuvette is different from
   the one you use for the samples, you get a path-length mismatch.
   Use the **same cuvette** for all measurements, just rinse and
   refill.

### 7.2 Symptom: slope is off by > 10 %

**Expected**: slope = 0.068 – 0.083 OD/µM.
**Observed**: slope = 0.12 OD/µM (too steep) or 0.04 (too shallow).

**Causes and fixes**:

1. **Dilution error**. Recheck your pipetted volumes. Most common
   cause. Make a fresh dilution series with more care.
2. **Stock concentration wrong**. Recheck your weigh-in. 3.74 mg per
   100 mL = 100 µM is the spec; 7.5 mg gives 200 µM and doubles your
   slope.
3. **Wavelength mismatch**. If your laser is actually at 635 nm (some
   cheap "650 nm" modules are off), the effective ε drops. Not a
   calibration failure per se — the instrument is still linear, the
   absolute slope just reflects the actual wavelength.
4. **Laser power drift between blank and samples**. The script uses
   one blank at the beginning. If the laser drifts during the run,
   the absorbance values shift. Re-run the blank at the end of the
   series and check.

### 7.3 Symptom: R² is 0.98 (noisy)

**Expected**: R² ≥ 0.999.
**Observed**: the fit is clearly linear but the points scatter.

**Causes and fixes**:

1. **Shot noise dominated**: your laser is too dim. Verify that the
   TIA output at the brightest sample is at least 100 mV above
   baseline. If the signal is only a few mV, increase R_f or use a
   brighter laser.
2. **Mechanical instability**. The TIA output should be stable to
   within 1 mV between tube insertions. If you see ~10 mV drift, the
   cuvette isn't seating repeatably — check the holder's alignment
   guide.
3. **Integration too short**. The script uses 10 s by default. Edit
   the call to `.integrate(seconds=30)` in
   `scripts/methylene_blue_run.py` for noisier sources.

### 7.4 Symptom: saturation at high concentration

**Expected**: A = 1.5 at 20 µM, well within the 0 – 3.0 OD dynamic
range of the instrument.
**Observed**: the top point "rolls over" — the last 1 or 2 points lie
below the fit line, as if saturating.

**Causes and fixes**:

1. **ADC saturation**. Check the raw ADC0 reading at the 20 µM tube —
   if it's near 4095 (max) or 0 (min), the TIA is rail-to-rail and
   you're losing dynamic range. Reduce R_f (10 MΩ → 1 MΩ) and re-run.
2. **Stray light**. At high absorbance, any small stray light
   component becomes a significant fraction of the remaining signal.
   See §7.1.
3. **Methylene blue dimerization**. Above ~30 µM, MB starts to
   aggregate, changing its effective ε. Stay below 20 µM and the
   curve remains linear. (This is a *real* chemistry limit, not an
   instrument issue.)

### 7.5 Symptom: laser won't turn on at all

**Expected**: the script reports `CH5 amplitude: 0.15 V` at the blank.
**Observed**: CH5 amplitude is ~0 V or noise-only.

**Causes and fixes**:

1. **Interlock still open**. Check the status bar:
   `INTERLOCK: OPEN`. Close the cover firmly. If it still reads OPEN,
   the microswitch may be mechanically misaligned — adjust.
2. **Interlock needs re-arm**. After any OPEN → OK transition, the
   host must send the re-arm command. The script does this
   automatically, but a buggy cable or disconnect can leave the
   firmware in a latched state. Restart the script.
3. **Safety relay stuck**. Check TP12 (`LASER_VCC_GATED`) with a
   multimeter. If it reads 0 V with the cover closed, the relay isn't
   energizing. Check the BC547 driver circuit (§3 of
   [`../build/HARDWARE_SCHEMATICS.md`](../build/HARDWARE_SCHEMATICS.md)).
4. **Laser driver wiring mistake**. Trace the path from DOUT3 through
   the MOSFET to the laser module. A wrong MOSFET polarity is the
   most common cause.
5. **Laser module dead**. Apply 5 V to the laser module directly
   (with the cover of course open and your glasses on). If it doesn't
   light up, the module is faulty. Swap.

### 7.6 Symptom: large 50 Hz / 100 Hz ripple on ADC0

**Expected**: noise floor ~0.6 mV rms, no spectral peaks at mains
frequencies.
**Observed**: visible 10 – 50 mV rms 100 Hz pickup on the trace.

**Causes and fixes**:

1. **Ground loop**. The TIA must have a star ground going directly to
   phycommander's analog ground pin. If the TIA shares a ground
   return path with the LED drivers (which switch hundreds of mA
   quickly), you get a huge common-mode disturbance. Re-wire with
   star ground.
2. **Unshielded cable between optical head and base station**. Use a
   twisted pair or shielded cable for the TIA output.
3. **Mains adapter**. If powering from a USB wall adapter, try a
   laptop battery or USB battery bank. Some cheap adapters inject
   100 Hz noise onto USB Vcc.
4. **Nearby fluorescent fixture**. Move away from desk lamps or
   fluorescent tubes that are modulated at mains frequency. Yes, this
   really matters for a photodiode.

### 7.7 Symptom: everything passes on CH5 but other channels give nonsense

The first-experiment script only validates CH5 (650 nm laser). If CH5
passes but e.g. CH4 (590 nm amber LED) gives a bad blank, then the
issue is specific to that channel, not to the global TIA / ADC chain.
Common causes:

1. **Software-toggled DOUT not actually toggling**. Check with an
   oscilloscope on the DOUT pin. If the pin is stuck, the host code
   is not applying the toggle mask.
2. **Wrong carrier frequency**. Open `config/channels.toml` and verify
   the assigned frequency. A mismatch between the MOSFET gate drive
   frequency and the lock-in demodulator frequency gives zero
   amplitude.
3. **LED burned out / reversed polarity**. Test the LED out of circuit
   with a 3 V coin cell and a series resistor. Swap if dead.

---

## 8. Next steps

### 8.1 If passed

Congratulations. Your analyzer is validated for quantitative absorbance
measurement. Next runs:

1. **Beer-Lambert extension** — repeat the methylene blue experiment on
   every channel (run the same dilution series and use a *different*
   colored dye for each LED). This calibrates all 8 channels.
2. **Formazin haze calibration** — validate the 90° haze TIA (mode 3.3)
   with a formazin dilution series.
3. **Ethanol calibration curve** — first real beer application. Use
   ethanol standards at 1, 2.5, 5, 7.5, 10 % v/v in water, run the
   ADH enzymatic assay per [`../../BEER_ANALYZER.md`](../../BEER_ANALYZER.md)
   §10.1. Store the resulting curve in `config/default_calibration.toml`.
4. **First real beer sample**. Use a commercial beer of known ABV
   (labelled on the bottle) and verify your measurement matches
   within ±0.3 % v/v. Some commercial beers have ABV labelling
   tolerance of ±0.5 %, so this is a sanity check, not a rigorous
   cross-validation.
5. **Cross-validation on 10 commercial beers**. This is the data for
   Paper 2 (see [`../../MULTIFUNCTION_ANALYZER.md`](../../MULTIFUNCTION_ANALYZER.md)
   §16.1).

### 8.2 If failed

Re-read the relevant section of §7 above. If you've tried everything
and the issue persists, open an issue on the phycommander repository
with:

- Photos of your build (hardware, grounding, cuvette holder)
- The exact raw data from the failed run (the script saves it to
  `~/.phycommander/analyzer/logs/<timestamp>.csv`)
- The output of `self_test.py` for comparison
- A description of what you changed between "everything was fine" and
  "now it's broken", if applicable

The issue tracker label for this kind of build-support question is
`analyzer:build-help`.

---

## 9. A note on scientific honesty

The purpose of this first experiment is to **catch problems** — not to
"validate" the instrument in the sense of producing a publishable
result. Even a passing methylene blue run only tells you that the
instrument is linear *for methylene blue at 650 nm in 1 cm PMMA
cuvettes under your lab conditions*. It does not guarantee that your
ethanol numbers will be correct, or that your beer color measurements
will match EBC. Those require their own per-assay calibrations.

What a passing run **does** guarantee is that you are not fighting the
*instrument* — you're free to worry about the *chemistry*. That's a
qualitatively different and much more comfortable place to be.

---

*End of first-experiment document. Good luck with your first run.*
