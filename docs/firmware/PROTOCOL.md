# PhyCommander wire protocol

This document is the source of truth for the bytes on the wire between
the host (`physerver`, the Python bindings, or any third-party client)
and the SAM3X8E firmware. It supersedes the partial descriptions
scattered across earlier `FIRMWARE_*.md` files.

The protocol has three independent planes:

| Plane                 | USB transport       | Cadence            | Purpose                                           |
| --------------------- | ------------------- | ------------------ | ------------------------------------------------- |
| **Streaming data**    | Bulk EPs `0x81/0x02`  *(legacy)* | host-paced       | Frame-by-frame command/status. Used by `type = "usb"` mode.   |
| **Streaming data**    | Iso EPs `0x83/0x04`            | every microframe  | Frame-by-frame command/status. Used by `type = "iso"` mode.   |
| **Per-channel control** | EP0 control (vendor SETUP)     | one-shot          | Start / update / stop a generator on a pin / DAC / PWM. The "mode" of a channel is implicit in the last action issued (see §3). |

The streaming data plane carries the same `PhyCMD-64` frame shape on
both bulk and iso. The control plane is the same regardless of which
streaming transport is selected.

> **Backwards compatibility.** Devices flashed with the original
> bulk-only firmware silently ignore vendor SETUP requests. Hosts can
> probe support with `GEN_GET_CAPS`; if the device STALLs that request,
> it doesn't have the function generator at all.

---

## 1. Streaming data plane — PhyCMD-64

### 1.1 Frame layout (64 bytes, little-endian, `__attribute__((packed))`)

#### Command (host → device, EP `0x02` bulk or EP `0x04` iso)

| Offset | Size | Field         | Notes                                                 |
| -----: | ---: | ------------- | ----------------------------------------------------- |
|     0 |   2 | `header`      | `0xAA55`                                              |
|     2 |   2 | `digital_out` | 16 GPIO output bits. Always applied.                  |
|     4 |   2 | `dac0`        | DAC channel 0 setpoint (0–4095). **Ignored when DAC0 is in GENERATOR mode.** |
|     6 |   2 | `dac1`        | DAC channel 1 setpoint (0–4095). **Ignored when DAC1 is in GENERATOR mode.** |
|     8 |   2 | `pwm0`        | PWM channel 0 duty (0–65535). *(reserved — firmware does not yet drive PWM peripherals; see §4.)* |
|    10 |   2 | `pwm1`        | PWM channel 1 duty (0–65535). *(reserved)*           |
|    12 |   1 | `flags`       | bit 0 ADC_ENABLE, bit 1 DAC_ENABLE, bit 2 PWM_ENABLE, bit 3 RESET_SEQ, bit 4 WATCHDOG_DISABLE |
|    13 |   1 | `seq_num`     | 8-bit free-running counter (host-side); device echoes it in status. |
|    14 |   2 | `crc`         | CRC-16-CCITT (poly `0x1021`, init `0xFFFF`) over bytes `0..14`. |
|    16 |  48 | `reserved`    | Must be sent as zeros. Reserved for protocol extensions. |

*Total: 64 bytes.*

For iso transfers the wire packet is 256 bytes; bytes `0..63` carry the
PhyCMD-64 frame and bytes `64..255` are zero padding (firmware ignores
them, but host must zero-fill or the device's CRC check on the next
revision could fail when the layout grows).

#### Status (device → host, EP `0x81` bulk or EP `0x83` iso)

| Offset | Size | Field         | Notes                                                  |
| -----: | ---: | ------------- | ------------------------------------------------------ |
|     0 |   2 | `header`      | `0x55AA`                                               |
|     2 |   2 | `digital_in`  | 16 GPIO input bits.                                    |
|     4 |   2 | `digital_out` | echo of the most recently applied output mask.         |
|     6 |  16 | `adc[0..7]`   | 8 × 16-bit ADC samples, latest from the PDC ring.      |
|    22 |   1 | `status_flags`| bit 0 ADC_ACTIVE, bit 1 DAC_ACTIVE, bit 2 PWM_ACTIVE, bit 3 ERROR, bit 4 WATCHDOG_TRIGGERED, bit 5 USB_CONFIGURED, bit 6 OVERRUN |
|    23 |   1 | `seq_num`     | echo of the last applied command's `seq_num`.          |
|    24 |   2 | `crc`         | CRC-16-CCITT over bytes `0..24`.                       |
|    26 |   2 | `loop_time_us`| device main-loop / ISR latency, microseconds.          |
|    28 |   4 | `uptime_ms`   | wall-clock since boot, milliseconds.                   |
|    32 |   2 | `error_count` | total CRC / header errors observed, saturating.        |
|    34 |  30 | `reserved`    | Sent as zeros.                                         |

*Total: 64 bytes.*

### 1.2 Interaction with the per-channel control plane

Each output channel internally tracks "is anything playing on me right
now?" (see §3 for the action-driven mode model). When the channel is
in `SHAPE_OFF` the corresponding field of the streaming `Command` frame
is applied on every arriving frame (the DAC peripheral is written, the
GPIO bit is set, etc.). When the channel is currently playing
*anything* (built-in or arbitrary) the field is **silently ignored**
by the firmware so that the on-chip TC chain keeps driving the pin
without interruption. The host is free to keep sending whatever
value it wants — there is no error, no penalty, and no need for
host-side coordination.

The `digital_in` and `adc` arrays are always populated, regardless of
the mode of any output channel.

---

## 2. Control plane — vendor SETUP requests

All transfers use endpoint zero (the USB control endpoint).
`bmRequestType` is `0x40` (vendor / device, OUT) for "set" requests
and `0xC0` (vendor / device, IN) for "get" requests. `wIndex` carries
the channel ID (see §3.1) when the request is per-channel; otherwise
it's `0`. The `wValue` and DATA stage usage is documented per request.

The firmware STALLs unknown `bRequest` codes; clients should treat a
STALL as "this firmware revision does not support that request".

### 2.1 Request table (bRequest values reserved by the firmware)

| `bRequest` | Direction | Name                       | wIndex   | wValue | DATA stage | Description |
| ---------: | --------- | -------------------------- | -------- | ------ | ---------- | ----------- |
|     `0x10` | IN  | `GEN_GET_CAPS`            | 0        | 0      | `Capabilities` (16 B) | Channel inventory & per-kind capability matrix (§2.2). |
|     `0x11` | IN  | `GEN_GET_STATE`           | channel  | 0      | `ChannelState` (24 B) | Current spec for one channel (§2.3). |
|     `0x12` | OUT | `GEN_PLAY_BUILTIN`        | channel  | 0      | `WaveBuiltinSpec` (16 B) | Start a built-in waveform on the channel, **or** seamlessly update the parameters of an already-running one (phase is preserved across updates). |
|     `0x13` | OUT | `GEN_PLAY_ARBITRARY`      | channel  | 0      | `WaveArbHeader` (8 B) + `int16_t samples[N]` | Start (or replace) replay of an uploaded sample buffer. `sample_rate_hz` lives in the header. |
|     `0x14` | OUT | `GEN_STOP`                | channel  | 0      | none      | Stop the generator on this channel. Streaming `Command` frames immediately resume driving the channel. |
|     `0x20` | OUT | `ADC_SET_RATE`            | 0        | 0      | `u32 rate_hz` (4 B)  | Set ADC sampling rate in Hz. Range 1 – 1_000_000. |
|     `0x21` | IN  | `ADC_GET_RATE`            | 0        | 0      | `u32 rate_hz` (4 B) | Read current ADC rate. |

There is **no** explicit `SET_MANUAL` request: a channel's mode is
*implicit* in what was last requested for it. After power-on (or after
`GEN_STOP`) every channel is "manual"; from the host's point of view
that just means `GEN_GET_STATE.shape == SHAPE_OFF`. Calling
`GEN_PLAY_*` flips the channel to "generator-driven"; `GEN_STOP`
flips it back. The streaming `Command` field for that channel may be
sent unconditionally — the firmware applies it iff the channel is
currently in `SHAPE_OFF`, otherwise it is silently dropped. This
means the host is free to keep streaming `cmd.dac[0]` even while
DAC0 is playing a sine; no coordination is needed.

`bRequest` codes `0x00..0x0F`, `0x16..0x1F`, `0x22..0x7F`, `0x80..0xFF`
are reserved for future use. Requests in the standard / class ranges
(`0x00..0x1F` of `bmRequestType`'s class field) are handled by the UDC
stack and not delivered to the vendor handler.

### 2.2 `GEN_GET_CAPS` payload (16 bytes)

```c
struct Capabilities {
    uint8_t  protocol_version;   // 1
    uint8_t  num_channels;       // total channels exposed (typ. 18: DAC0/1, PWM0..7, DOUT0..7)
    uint8_t  num_dac_channels;
    uint8_t  num_pwm_channels;   // hardware PWM-capable
    uint8_t  num_dout_channels;
    uint8_t  num_din_channels;
    uint8_t  num_adc_channels;
    uint8_t  reserved0;
    uint32_t max_dac_sample_rate_hz;  // typ. 1_000_000
    uint32_t max_arb_buffer_samples;  // typ. 1024
};
```

### 2.3 `WaveBuiltinSpec` / `WaveArbHeader` / `ChannelState`

```c
typedef enum {
    SHAPE_OFF       = 0,   // channel is "manual": streaming Command frame drives it
    SHAPE_DC        = 1,   // generator-driven, constant level at `offset`
    SHAPE_SINE      = 2,
    SHAPE_SQUARE    = 3,
    SHAPE_TRIANGLE  = 4,
    SHAPE_SAWTOOTH  = 5,
    SHAPE_ARBITRARY = 6,   // generator-driven from an uploaded buffer
} wave_shape_t;

struct WaveBuiltinSpec {                  // 16 bytes
    uint8_t  shape;        // wave_shape_t. SHAPE_OFF here is a no-op
                           //   (use GEN_STOP to return to manual).
                           //   SHAPE_ARBITRARY is invalid here
                           //   (use GEN_PLAY_ARBITRARY).
    uint8_t  reserved0;
    uint16_t amplitude;    // peak-to-peak in raw device units (DAC: 0–4095)
    uint16_t offset;       // mid-point in raw device units
    uint16_t duty_x10;     // 0–1000 = 0.0–100.0 % (square only; ignored otherwise)
    uint32_t freq_mHz;     // fundamental frequency in milli-Hertz (1 mHz = 0.001 Hz)
    uint32_t sample_rate_hz; // device-side sampling rate. 0 = "auto" (firmware
                             //   picks the highest rate ≤ max_dac_sample_rate_hz
                             //   that yields ≥ 8 samples/cycle).
};

struct WaveArbHeader {                    // 8 bytes header preceding the int16 samples
    uint16_t n_samples;    // 1..max_arb_buffer_samples
    uint16_t loop_count;   // 0 = infinite, otherwise number of times to play
                           //   before the channel auto-stops (returns to SHAPE_OFF).
    uint32_t sample_rate_hz; // playback rate, in Hz. Must be ≤ max_dac_sample_rate_hz.
    // followed by int16_t samples[n_samples]
};

struct ChannelState {                     // 24 bytes
    uint8_t  channel_kind;     // 0=DAC, 1=PWM, 2=DOUT, 3=DIN, 4=ADC
    uint8_t  channel_index;    // index within its kind (DAC0 → kind=0,index=0)
    uint8_t  shape;            // wave_shape_t. SHAPE_OFF means the channel is
                               //   currently driven by the streaming plane (manual).
    uint8_t  reserved0;
    uint16_t amplitude;
    uint16_t offset;
    uint16_t duty_x10;
    uint32_t freq_mHz;
    uint32_t sample_rate_hz;
    uint16_t arb_n_samples;    // 0 if not arbitrary
    uint16_t arb_loop_count;   // remaining loops; 0 = infinite or N/A
};
```

The `shape` field doubles as the mode indicator: `SHAPE_OFF` → manual,
anything else → generator-driven. There is no separate `mode` byte.

### 2.4 Channel ID encoding (`wIndex`)

Channels are numbered `0..num_channels - 1`. The mapping is fixed for
a given firmware revision and reported by `GEN_GET_CAPS`. The current
SAM3X8E firmware exposes:

| Range          | Meaning            | Notes                                                   |
| -------------- | ------------------ | ------------------------------------------------------- |
| `0`            | DAC0               | full waveform generator support                         |
| `1`            | DAC1               | full waveform generator support                         |
| `2..9`         | PWM0..PWM7         | reserved for future PWM peripheral support; currently MANUAL-only |
| `10..25`       | DOUT0..DOUT15      | currently MANUAL-only (the generator can drive a digital pin only via PWM/TC future support) |
| `26..41`       | DIN0..DIN15        | read-only, MANUAL only                                  |
| `42..49`       | ADC0..ADC7         | read-only; rate control is global, see `ADC_SET_RATE`   |

Hosts should not assume this mapping is stable across firmware
revisions — always derive it from `GEN_GET_CAPS`.

### 2.5 Behaviour on errors

| Condition                                              | Firmware response                       |
| ------------------------------------------------------ | --------------------------------------- |
| Unknown `bRequest`                                     | STALL                                   |
| `wIndex` out of range                                  | STALL                                   |
| Channel does not support the requested mode            | STALL                                   |
| `wLength` doesn't match the expected struct size       | STALL                                   |
| Arbitrary upload exceeds `max_arb_buffer_samples`      | STALL (with no partial write)           |
| `freq_mHz` × `sample_rate_hz` exceeds Nyquist          | accepted but aliasing is the user's problem |

A successful `GEN_SET_*` request applies the new configuration
atomically: the next ping-pong buffer fill picks up the new spec, so
parameter sweeps are seamless (no glitch on the analog output).

---

## 3. Channel mode model — implicit, action-driven

Every output channel (`DAC*`, `PWM*`, `DOUT*`) has an internal "is
something playing on me?" state, but it is **not** exposed as an
explicit `mode` enum the host has to manage. The mode is implied by
the last action issued on that channel:

| Last action observed by the channel  | Effective behaviour                                                       |
| ------------------------------------ | ------------------------------------------------------------------------- |
| Power-on, or `GEN_STOP` after any generator | "manual": the value carried in the streaming `Command` frame is applied on every microframe (8 kHz HS iso / 1 kHz bulk). `GET_STATE.shape == SHAPE_OFF`. |
| `GEN_PLAY_BUILTIN` with a non-OFF shape | "generator-driven (built-in)": the on-chip TC triggers the channel's peripheral at `sample_rate_hz`; the ping-pong buffers are filled by the shape generator. The streaming field is silently ignored. |
| `GEN_PLAY_ARBITRARY`                | "generator-driven (replay)": same as above but the ping-pong buffers are seeded from a host-uploaded `int16_t samples[N]` array. After `loop_count` loops the channel auto-returns to "manual" (back to `SHAPE_OFF`). |

A second `GEN_PLAY_BUILTIN` on the same channel **does not restart**
the generator: it updates the parameters in place, and the next
ping-pong swap (≤ 1 ms later) emits the new shape with the phase
preserved. This is what makes glitch-free FM / AM sweeps possible.

The host doesn't have to track the per-channel mode itself. It can
keep streaming the `Command` frame with whatever values it likes —
the firmware applies them iff that channel happens to be in
`SHAPE_OFF`, otherwise drops them. UI layers that want to *display*
the active mode should poll `GEN_GET_STATE` (or push it on a slow
WebSocket channel — there is no benefit to receiving it at
microframe rate).

Channels are completely independent: DAC0 playing a 100 kHz sine
coexists with DAC1 in manual coexists with DOUT0..DOUT15 in manual.
The host can freely mix actions across the inventory.

---

## 4. Hardware capability matrix

| Channel    | MANUAL | BUILTIN | ARBITRARY | Sample rate range | Notes                                        |
| ---------- | :----: | :-----: | :-------: | ----------------- | -------------------------------------------- |
| DAC0, DAC1 | ✓      | ✓       | ✓         | 1 Hz – 1 MHz      | TC-triggered DACC + PDC                       |
| PWM0..PWM7 | ✓      | ⏳ planned | ⏳ planned | up to 1.28 kHz @ 16-bit duty | Hardware PWM peripheral; current firmware does not yet apply the PWM duty fields. |
| DOUT0..15  | ✓      | ⏳ planned (TC-toggle on PWM/TC-routed pins only) | — | up to ~10 MHz on PWM/TC pins | Most DOUT pins are GPIO-only and cannot be driven faster than the streaming plane. |
| DIN0..15   | read   | —       | —         | sampled per iso frame | —                                            |
| ADC0..7    | read   | —       | —         | 1 Hz – 1 MHz (global) | TC-triggered ADC + PDC                       |

`⏳ planned` items will land in a follow-up firmware revision. The
protocol slots and `bRequest` codes above are already reserved so the
host will not need to be re-rolled when the firmware catches up.

### 4.1 ADC: conversion rate vs streaming rate

`ADC_SET_RATE` controls the **conversion rate** (the rate at which the
SAM3X ADC peripheral samples its 8 channels through the PDC ring). The
**streaming rate** at which those samples reach the host is bounded by
the data plane (8 kHz HS iso, 1 kHz bulk). When the conversion rate
exceeds the streaming rate, only the most recent sample reaches the
iso IN frame and the intermediate ones are visible only to firmware
(useful for future on-chip closed-loop control). When the conversion
rate is below the streaming rate, the iso IN frame repeats the last
acquired value until a new one is ready.

If you need every ADC sample at high rates and don't have a use for
on-device control, configure `ADC_SET_RATE` to match the streaming
rate (e.g., 8000) — that way the iso IN frame always carries a fresh
value with no firmware-side decimation or duplication.

### 4.2 Sample-rate change ↔ glitch model (BUILTIN / ARBITRARY)

Repeated `GEN_PLAY_BUILTIN` on the same channel **with the same
`sample_rate_hz`** is glitch-free: the TC keeps running, and only
the next ping-pong refill picks up the new shape / freq / amp / off /
duty parameters. The phase accumulator is preserved across the
transition.

Repeated `GEN_PLAY_BUILTIN` (or first-time `GEN_PLAY_BUILTIN` after
`GEN_STOP`) **with a new `sample_rate_hz`** has a worst-case 1-sample
gap on the analog output while the TC is reprogrammed (~1 µs at the
top end of the rate range). The firmware skips the reconfig if the
new rate matches the current one, so callers that only sweep
freq/amp/duty pay zero glitch.

`GEN_STOP` releases the channel cleanly: the DAC is parked at its
last sample value, then the next streaming `Command` frame's value
takes effect immediately.

### 4.3 Generator state on USB disable / disconnect

When the USB interface is disabled (`udi_vendor_disable()` — host
disconnect, USB bus reset, or `physerver` shutdown), the firmware
**stops every running generator** and resets the affected channels to
`SHAPE_OFF`. This is a safety property: a Python script that crashes
or a network failure must not leave a 100 kHz square wave on a DAC
indefinitely.

The DAC outputs are *not* forced to zero on disable — they hold their
last sample until either (a) `udi_vendor_enable()` is called again
and a fresh streaming frame arrives, or (b) the device is power-cycled.

### 4.4 CPU / memory cost on the SAM3X

| Configuration                         | Firmware CPU             | SRAM (per channel) |
| ------------------------------------- | ------------------------ | ------------------ |
| `MANUAL` only                         | < 0.1 % (existing path)  | 0                  |
| `BUILTIN` sine @ 1 MSPS               | ~3 % (LUT lookup + interp) | 4 KB (2 × 1024 × 2) |
| `BUILTIN` sine @ 100 kSPS             | ~0.3 %                   | 4 KB                |
| `BUILTIN` square @ 1 MSPS             | ~1 % (cheaper than sine)  | 4 KB                |
| `ARBITRARY` @ 1 MSPS, 1024-sample buf | ~12 %                    | 4 KB + the buffer ≈ 6 KB |
| `ARBITRARY` @ 100 kSPS                | ~1.2 %                   | same                |

Two DACs both at 1 MSPS arbitrary ≈ 24 % SAM3X CPU. With UOTGHS ISR
overhead at 8 kHz iso (~5 %), peak total ≈ 30 %. Plenty of headroom
on a 84 MHz Cortex-M3 but worth knowing if you push every knob.

### 4.5 NVIC priorities (firmware-internal contract)

The interrupt subsystem priority levels are set by the firmware at
boot to:

| Priority | Source              | Why                                                                |
| :------: | ------------------- | ------------------------------------------------------------------ |
| 0        | UOTGHS              | iso jitter is the user-visible RT metric; never let anything delay it |
| 1        | DACC ENDTX (PDC)    | ping-pong refill must complete before the buffer underruns           |
| 2        | TC compare events   | scheduling source; no Cortex IRQ involved if used purely as PDC trigger |
| 3+       | ADC, SysTick, etc.  | best-effort                                                        |

Inverting these priorities (e.g. running the DACC refill above
UOTGHS) shows up as ≥50 µs spikes in the `RtStats` jitter histogram.
This is documented here so that future maintainers don't reorder them
"to be safe".

### 4.6 Future enhancements (reserved, not implemented)

These are intentionally left out of v1 to keep the protocol surface
manageable. The slots are reserved so they can land later without
breaking compatibility:

- **Phase-locked multi-channel start** (e.g., I/Q sine pairs). Will
  use a new request `GEN_PLAY_LOCKED_BUILTIN` (`bRequest 0x16`) that
  takes a bitmask of channels to start in lockstep with a per-channel
  phase offset.
- **PWM / TC-routed digital channels**: protocol already reserves
  channel IDs 2..25; the firmware just needs to wire them up.
- **Streaming arbitrary** (continuous host upload of new samples
  while the buffer plays — required for software-defined waveforms
  longer than 1024 samples). Will use a new bulk EP rather than EP0
  control transfers.
- **Closed-loop on the device** (e.g., PID using ADC0 as feedback
  to drive DAC0). Will use a new `GEN_PLAY_CLOSED_LOOP` request.

---

## 5. Worked examples

### 5.1 Sine sweep on DAC0 while DAC1 is driven manually from the streaming plane

```python
import phycmd, time

dev = phycmd.Device.open()                                # VID 0x2341 PID 0x003e
caps = dev.get_capabilities()
assert caps.num_dac_channels >= 2

# (1) Start a 1 kHz sine on DAC0 with full-scale swing.
#     The streaming plane keeps running undisturbed; cmd.dac[0] is
#     ignored by firmware as long as DAC0 is generator-driven.
dev.gen_play_builtin("dac0", shape="sine",
                     freq_hz=1000.0, amplitude=4000, offset=2048,
                     sample_rate_hz=100_000)

# (2) Meanwhile, drive DAC1 manually from a Python loop at the iso
#     bus rate. The same dev handle multiplexes both planes safely.
for k in range(8000):
    dev.cmd.dac[1] = (k * 17) & 0xFFF      # whatever the user wants
    time.sleep(125e-6)                     # ~8 kHz update rate

# (3) Sweep DAC0 frequency 1 kHz → 10 kHz over 5 s without phase
#     discontinuity. Each call updates the *parameters* of the running
#     generator; phase is preserved across the ping-pong swap.
t0 = time.monotonic()
while (t := time.monotonic() - t0) < 5.0:
    dev.gen_play_builtin("dac0", shape="sine",
                         freq_hz=1000.0 * (10.0 ** t),
                         amplitude=4000, offset=2048,
                         sample_rate_hz=100_000)
    time.sleep(0.020)

# (4) Stop the generator. The next streaming cmd.dac[0] is applied
#     immediately by the firmware (no transition request needed).
dev.gen_stop("dac0")
```

### 5.2 Square wave at 100 kHz on DAC0, DC level on DAC1, GPIO toggled by host loop

```python
dev.gen_play_builtin("dac0", shape="square",
                     freq_hz=100_000, amplitude=4095, offset=2048, duty=0.5,
                     sample_rate_hz=1_000_000)          # 10 samples/period

dev.gen_play_builtin("dac1", shape="dc", offset=1024)    # 25%-of-FSR DC

# DOUT bits remain in manual mode; toggle DOUT5 on the host's own
# 1 ms loop, completely independent of what DAC0/DAC1 are doing.
state = False
while True:
    state = not state
    dev.cmd.digital_out = (1 << 5) if state else 0
    time.sleep(0.001)
```

Each `gen_play_builtin` completes in ~5 ms (USB control transfer +
firmware refill of the inactive ping-pong buffer). The analog
output keeps tracking the live parameters without glitches.

---

## 6. Compatibility / versioning

`Capabilities.protocol_version` starts at `1`. Backwards-incompatible
changes will bump this byte; backwards-compatible additions (new shape
codes, new channel kinds, new `bRequest` values) do not. Hosts should
gracefully ignore unknown shape codes returned by `GEN_GET_STATE` and
treat them as "unknown / read-only".
