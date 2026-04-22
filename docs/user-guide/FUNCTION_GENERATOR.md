# On-chip Function Generator — User Guide

> **Scope.**  This guide covers the firmware-driven waveform and
> reactive-control modes exposed by the SAM3X8E (PhyCommander v2.0.0+
> firmware). For the wire-level protocol details see
> [`docs/firmware/PROTOCOL.md`](../firmware/PROTOCOL.md).

The firmware exposes every DAC/DOUT/PWM channel through a unified
"mode-per-channel" model. At any instant each output is in exactly
one of:

| Family    | Modes                                            |
| --------- | ------------------------------------------------ |
| Open-loop | `off` (manual) · `dc` · `sine` · `square` · `triangle` · `sawtooth` · `arbitrary` |
| Reactive  | `lut` · `threshold` · `pulse_trig` · `pid`       |

- **open-loop** modes compute `output = f(time)` inside the SAM3X
  (TC + DACC + PDC). Up to **1 MSPS per DAC channel** in v1 firmware.
- **reactive** modes compute `output = f(input)` from an ADC channel
  or a DIN bit-mask. Reaction latency ≈ **1 µs (DIN)** / **1 µs + one
  ADC conversion period (ADC)**.
- `off` means "manual" — the streaming `Command` frame (PhyCMD-64
  over iso/bulk) drives the channel at the USB-packet rate (8 kHz HS
  / 1 kHz bulk). Manual is the default on power-up and after `stop`.

Channels are **fully independent**: DAC0 can play a 100 kHz sine
while DAC1 runs a PID loop, DOUT5 flickers on every DIN0 edge, and
DOUT6..15 stay under manual Python control — all at once.

---

## 1. Quick start

### 1.1 Firmware + dashboard

Flash the v3 firmware (commit `7e97fc9` or newer) and run
`physerver` in **iso mode**:

```toml
# /etc/physerver/config.toml
[transport]
type = "iso"
```

`sudo systemctl restart physerver` and open
`http://<host>:8080/`. The new panel **"On-chip Function Generator"**
appears with live state polling; flip `dac0`'s mode to `sine`, set
freq, amplitude and offset, click **apply**.

### 1.2 Shell / curl

Every firmware primitive is a JSON POST away:

```bash
# Sine on DAC0, 1 kHz, full scale centred at mid-rail
curl -X POST -H 'content-type: application/json' \
     -d '{"shape":"sine","freq_hz":1000,"amplitude":4000,"offset":2048}' \
     http://localhost:8080/api/fngen/play_builtin/dac0

# See what the firmware thinks it's doing
curl -s http://localhost:8080/api/fngen/state/dac0 | python3 -m json.tool

# Back to manual
curl -X POST http://localhost:8080/api/fngen/stop/dac0
```

### 1.3 Python CLI (no REST server required)

`scripts/phycmd_waveform.py` talks directly to libusb — handy for
debugging when `physerver` isn't running or isn't reachable:

```bash
sudo systemctl stop physerver             # release the USB interface
python3 scripts/phycmd_waveform.py caps
python3 scripts/phycmd_waveform.py sine dac0 1000 --amp 4000 --off 2048
python3 scripts/phycmd_waveform.py stop dac0
sudo systemctl start physerver
```

Only one process at a time can hold the interface; pick either the
service or the direct script.

---

## 2. Open-loop waveforms

### 2.1 Built-in shapes — `play_builtin`

```json
POST /api/fngen/play_builtin/<channel>
{
  "shape":      "sine" | "square" | "triangle" | "sawtooth" | "dc",
  "freq_hz":    1000.0,
  "amplitude":  4000,
  "offset":     2048,
  "duty":       0.5
}
```

| field       | range                | notes                                           |
| ----------- | -------------------- | ----------------------------------------------- |
| `freq_hz`   | 0.001 – 500 000      | signal frequency; firmware resolution 0.001 Hz  |
| `amplitude` | 0 – 4095 (DAC)       | peak-to-peak swing in raw device units          |
| `offset`    | 0 – 4095             | mid-point around which the wave swings          |
| `duty`      | 0.0 – 1.0            | `square` only; 0.5 = 50% high                   |

Re-issuing `play_builtin` on a channel already running **updates
parameters seamlessly** — phase is preserved across the ping-pong
refill, so FM/AM sweeps have no glitch:

```python
# Log sweep 1 kHz → 10 kHz over 5 s
import time, requests
t0 = time.monotonic()
while (t := time.monotonic() - t0) < 5.0:
    requests.post("http://host:8080/api/fngen/play_builtin/dac0", json={
        "shape": "sine",
        "freq_hz": 1000.0 * 10**t,
        "amplitude": 4000, "offset": 2048,
    })
    time.sleep(0.020)                      # ~50 updates/s
```

### 2.2 Arbitrary waveforms — `play_arbitrary`

Upload up to **1024 `int16` samples** per channel. The firmware
loops them at the requested `sample_rate_hz` (oversampled to the
shared DAC clock):

```bash
# 16-point sawtooth, played at 10 kSPS → 625 Hz effective fundamental
curl -X POST -H 'content-type: application/json' \
     -d '{"samples":[0,273,546,819,1092,1365,1638,1911,
                     2184,2457,2730,3003,3276,3549,3822,4095],
          "sample_rate_hz":10000, "loop_count":0}' \
     http://host:8080/api/fngen/play_arbitrary/dac0
```

- `loop_count = 0` → loops forever; otherwise plays N times then
  auto-stops (channel back to `off`).
- `samples` are raw DAC codes (0 – 4095). Values outside the range
  are clamped.
- Best practice: use a number of samples that divides the DAC clock
  evenly so the zero-order hold doesn't alias.

### 2.3 Shared DAC clock — `dac_clock`

Both DAC channels share a single TC trigger; the default clock is
1 MSPS. Lower it to save CPU when you don't need the full bandwidth:

```bash
# 100 kSPS is plenty for audio-band signals
curl -X POST -H 'content-type: application/json' \
     -d '{"value":100000}' \
     http://host:8080/api/fngen/dac_clock

curl -s http://host:8080/api/fngen/dac_clock
# → {"value":100000}
```

At 1 MSPS the worst-case firmware CPU is ~12 % per channel for
arbitrary playback; at 100 kSPS it drops to ~1.2 %. PROTOCOL.md
§4.4 has the full cost matrix.

---

## 3. Reactive modes

Reactive modes compute `output = f(input)` inside the firmware. The
host provides the `f` (LUT entries / threshold spec / PID gains),
the SAM3X evaluates it on every ADC sweep (or DIN change) and
writes the output directly — latency is dominated by the ADC
conversion time (~5 µs at 200 kSPS), not by USB.

### 3.1 Look-up table — `play_lut`

Generic table: `output = LUT[input]`. Input is a 12-bit value
either from an ADC channel or from a packed mask of DIN bits.

```json
POST /api/fngen/play_lut/<channel>
{
  "input_src":   "adc" | "din",
  "input_arg":   <ADC channel 0–7>  OR  <DIN bit-mask, LSB-first>,
  "entries":     [int16, int16, …],   // 1..4096 entries
  "output_mask": 0xFFFF                // DOUT only — which bits to drive
}
```

Examples:

```bash
# Gamma curve on DAC0 from ADC0 — 4096-entry 12-bit input
curl -X POST -H 'content-type: application/json' \
     -d "$(python3 -c 'import json; print(json.dumps({
            "input_src":"adc", "input_arg":0,
            "entries":[int(((i/4095)**2.2)*4095) for i in range(4096)]
        }))')" \
     http://host:8080/api/fngen/play_lut/dac0

# 2-input AND gate on DOUT5 driven by DIN0 and DIN1
# input_arg = 0b11 (selects DIN0 → bit 0 of index, DIN1 → bit 1)
# 4 entries, only last one (both high) drives DOUT5 high
curl -X POST -H 'content-type: application/json' \
     -d '{"input_src":"din", "input_arg":3,
          "entries":[0, 0, 0, 32],         # only 0b11 → DOUT5 (bit 5 = 32)
          "output_mask":32}' \
     http://host:8080/api/fngen/play_lut/dout5
```

Slot limits (v1 firmware): **3 simultaneous LUTs** — one each for
DAC0, DAC1, and a shared DOUT LUT (PROTOCOL.md §3.2).

### 3.2 Threshold comparator — `play_threshold`

Analog comparator with hysteresis:
```
if input > thr_high   → output = val_high
if input < thr_low    → output = val_low
(otherwise hold)
```

```bash
# Fan on DOUT5 when ADC0 reads hotter than ~2.0 V (2000) and back off below ~1.9 V (1900)
curl -X POST -H 'content-type: application/json' \
     -d '{"input_src":"adc","input_arg":0,
          "thr_high":2000,"thr_low":1900,
          "val_high":1,"val_low":0}' \
     http://host:8080/api/fngen/play_threshold/dout5
```

Available on all DAC and DOUT channels. Hysteresis eliminates the
"chatter at threshold" failure mode that a bare comparator has.

### 3.3 Pulse trigger (monostable) — `play_pulse_trig`

```
on edge(DIN[din_bit]) → DOUT = active_level for duration_us,
                        then revert
re-triggers within cooldown_us are ignored.
```

```bash
# 100 ms high pulse on DOUT6 on every rising edge of DIN0, with 50 ms dead time
curl -X POST -H 'content-type: application/json' \
     -d '{"din_bit":0,"edge":"rising","active_level":1,
          "duration_us":100000,"cooldown_us":50000}' \
     http://host:8080/api/fngen/play_pulse_trig/dout6
```

DOUT only in v1. Up to 4 simultaneous pulse slots (TC1_CH0..2 +
TC2_CH0). Sub-µs accuracy on the pulse duration once the PIO ISR
fires.

### 3.4 PID controller — `play_pid`

Closed-loop on the device:
```
err       = setpoint − adc[input_arg]
output    = clamp(Kp·err + Ki·∫err + Kd·d(err)/dt, out_min, out_max)
```

All gains in floating-point on the wire; firmware converts to
Q16.16 fixed point internally (no FPU on Cortex-M3 so FP on the
device would cost ~20× the CPU). Anti-windup clamps the integral
to ±`integral_clamp`; the derivative term is single-pole
low-passed (α=0.25) for measurement-noise rejection.

```json
POST /api/fngen/play_pid/<channel>
{
  "input_src":       "adc",
  "input_arg":       <ADC channel 0–7>,
  "sample_rate_hz":  1000,
  "setpoint":        2048,
  "kp":              0.5,
  "ki":              0.001,
  "kd":              0.0,
  "out_min":         0,
  "out_max":         4095,
  "integral_clamp":  1000000
}
```

The **effective PID rate equals `adc_rate`** (see §4), so set that
first:

```bash
# Run ADC at 1 kSPS per channel, then start PID at 1 kHz
curl -X POST -d '{"value":1000}' http://host:8080/api/fngen/adc_rate
curl -X POST -d '{"input_src":"adc","input_arg":0,"sample_rate_hz":1000,
                  "setpoint":2048,"kp":0.5,"ki":0.001}' \
     http://host:8080/api/fngen/play_pid/dac0
```

Tuning cheat-sheet (Ziegler-Nichols, second-order plants):

1. Start with `kp=0.1, ki=0, kd=0` and ramp `kp` up until the system
   oscillates — call that gain `Ku` and the oscillation period `Tu`.
2. Set `kp = 0.6·Ku`, `ki = 2·kp/Tu`, `kd = kp·Tu/8`.
3. Fine-tune on target by watching `state.cur_phase_q24_8` and the
   ADC reading via the dashboard oscilloscope.

Two simultaneous PID slots (one per DAC channel). `stop` immediately
freezes the loop; on next `play_pid` the integrator is reset (or
preserved if the same params are re-applied — see firmware code).

---

## 4. Independent ADC rate

`adc_rate` controls the **per-channel sampling frequency** of the
8-channel ADC sweep. Default at boot is the free-running rate
(~600 kSPS/sweep), which is too fast for most control loops. Lower
it to match your PID / reactive cadence:

```bash
curl -X POST -d '{"value":2000}' http://host:8080/api/fngen/adc_rate
```

Gotcha: the ADC streaming rate **on the iso IN frame stays at 8 kHz**
(one sample per microframe). So:

- `adc_rate > 8 kHz` → only the most recent sample reaches the
  iso stream; intermediate ones are visible only to on-device
  reactive paths (LUT/THRESHOLD/PID).
- `adc_rate < 8 kHz` → the iso stream repeats the last acquired
  sample until a new one is ready.

For pure visualisation pick `adc_rate ≤ 8000`; for tight closed-loop
control, crank it up to 50 – 100 kHz and live with the stream
decimation.

---

## 5. State introspection

Every channel's live state is available at:

```
GET /api/fngen/state/<channel>
```

```json
{
  "channel_kind":        0,            // 0=DAC, 1=PWM, 2=DOUT, 3=DIN, 4=ADC
  "channel_index":       0,
  "shape":               2,            // 0=off, see PROTOCOL.md §2.3
  "shape_name":          "sine",
  "freq_mhz":            1000000,      // milli-Hz
  "duty_x10":            500,          // 0..1000 = 0..100%
  "amplitude":           4000,
  "offset":              2048,
  "phase_offset_x16":    0,            // reserved for multi-channel phase-lock
  "arb_n_samples":       0,
  "arb_loops_remaining": 0,
  "arb_sample_rate_hz":  0,
  "cur_phase_q24_8":     0x1A2B3C4D    // live phase accumulator, useful for debug
}
```

The dashboard polls this every 1 s and shows it in the live badge on
each channel row.

---

## 6. Reliability guarantees

- **USB disconnect / physerver crash → all generators stop** (see
  PROTOCOL.md §4.3). Safety: a runaway script cannot leave a DAC
  oscillating after losing control.
- **Streaming PhyCMD-64 is always served** — even when every DAC
  is in GENERATOR mode, the iso frames keep flowing (just with the
  DAC fields ignored for generator-driven channels). Status updates
  never stop.
- **Reactive and open-loop coexist** on different channels without
  interaction. The iso OUT callback respects `shape != OFF` per
  channel: for generator-driven channels the streaming value is
  silently dropped.

---

## 7. Troubleshooting

| Symptom                                                     | Check / fix                                                                                       |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `/api/fngen/*` → 503                                        | `type = "iso"` in `/etc/physerver/config.toml`; restart service                                   |
| `/api/fngen/*` → 400 "control transfer stalled"             | Firmware STALLed the SETUP request — your parameters are out of spec; check PROTOCOL.md §2.5      |
| `curl /caps` works but `play_builtin` STALLs                | Firmware is older than `7625890` (no vendor SETUP hook in conf_usb.h); reflash v3 firmware        |
| No analog signal on DAC0 even though `state` reports `sine` | DAC clock might be 0 or the TC RA config bug is back; check `dac_clock` and firmware commit `d53cd5f` |
| Dashboard says "unavailable"                                | `fngenPoll` got a non-200 — watch the browser devtools Network tab                                |
| Python script: "device not found"                           | `sudo systemctl stop physerver` first — only one process at a time can hold the interface         |

If `cur_phase_q24_8` never advances between two consecutive
`GET state` calls the DAC PDC isn't running — file the firmware
version (`GET /caps` returns semver), the dashboard URL, and a
`sudo journalctl -u physerver -n 100 --no-pager` dump.
