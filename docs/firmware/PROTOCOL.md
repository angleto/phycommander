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

Both directions use the same 64-byte fixed-length frame so that the
firmware ISR and the host transport can use a single static buffer
size everywhere. All multi-byte fields are little-endian (matches
both x86_64 and ARM Cortex-M3 native order — zero conversion cost).

#### Command (host → device, EP `0x02` bulk or EP `0x04` iso)

| Offset | Size | Field         | Notes                                                 |
| -----: | ---: | ------------- | ----------------------------------------------------- |
|     0 |   2 | `header`      | `0xAA55`                                              |
|     2 |   2 | `digital_out` | 16 GPIO output bits. Bit `i` of `1 << i` drives pin `i`. **Always applied** (every DOUT pin honours its bit on every frame; per-pin GENERATOR mode is reserved for the future via PWM/TC). |
|     4 |   2 | `dac0`        | DAC0 setpoint (0–4095). **Ignored if DAC0 is in GENERATOR mode** (`GEN_GET_STATE.shape != SHAPE_OFF`). |
|     6 |   2 | `dac1`        | DAC1 setpoint (0–4095). **Ignored if DAC1 is in GENERATOR mode.** |
|     8 |   2 | `pwm0`        | PWM0 duty (0–65535). *(reserved — firmware does not yet drive PWM peripherals; see §4.)* |
|    10 |   2 | `pwm1`        | PWM1 duty (0–65535). *(reserved)*                    |
|    12 |   1 | `flags`       | command flag bitmask, see table below                 |
|    13 |   1 | `seq_num`     | 8-bit free-running counter (host-side); device echoes in status. |
|    14 |   2 | `crc`         | CRC-16-CCITT (poly `0x1021`, init `0xFFFF`) over bytes `0..14`. |
|    16 |  48 | `reserved`    | Must be sent as zeros. Reserved for protocol extensions. |

*Total: 64 bytes.*

##### Command `flags` bitmask

| Bit | Macro                  | Meaning                                                            |
| --: | ---------------------- | ------------------------------------------------------------------ |
|   0 | `FLAG_ADC_ENABLE`      | Reserved (firmware always samples ADC at the configured rate).     |
|   1 | `FLAG_DAC_ENABLE`      | Apply `dac0` / `dac1` fields (when in MANUAL mode). When clear, MANUAL DAC writes are skipped. |
|   2 | `FLAG_PWM_ENABLE`      | Reserved (no firmware action yet).                                 |
|   3 | `FLAG_RESET_SEQ`       | One-shot: device resets its echoed `seq_num` to 0 and acknowledges by reflecting `seq_num=0` in the next status. |
|   4 | `FLAG_WATCHDOG_DISABLE`| Reserved.                                                          |
| 5-7 | reserved               | Must be 0.                                                         |

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
|     6 |  16 | `adc[0..7]`   | 8 × 16-bit ADC samples, latest from the PDC ring (decimated by the iso IN rate; see §4.1). |
|    22 |   1 | `status_flags`| status flag bitmask, see table below                   |
|    23 |   1 | `seq_num`     | echo of the last applied command's `seq_num`.          |
|    24 |   2 | `crc`         | CRC-16-CCITT over bytes `0..24`.                       |
|    26 |   2 | `loop_time_us`| device main-loop / ISR latency, microseconds.          |
|    28 |   4 | `uptime_ms`   | wall-clock since boot, milliseconds.                   |
|    32 |   2 | `error_count` | total CRC / header errors observed, saturating at 0xFFFF. |
|    34 |  30 | `reserved`    | Sent as zeros.                                         |

*Total: 64 bytes.*

##### Status `status_flags` bitmask

| Bit | Macro                       | Meaning                                                                |
| --: | --------------------------- | ---------------------------------------------------------------------- |
|   0 | `STATUS_ADC_ACTIVE`         | ADC PDC ring is producing fresh samples.                              |
|   1 | `STATUS_DAC_ACTIVE`         | At least one DAC channel is currently in GENERATOR mode.              |
|   2 | `STATUS_PWM_ACTIVE`         | Reserved (PWM driver not yet implemented).                            |
|   3 | `STATUS_ERROR`              | The most recently received command failed CRC / header check.         |
|   4 | `STATUS_WATCHDOG_TRIGGERED` | Reserved.                                                              |
|   5 | `STATUS_USB_CONFIGURED`     | Set after `udi_vendor_enable()`. Cleared on disconnect.               |
|   6 | `STATUS_OVERRUN`            | A previous status frame was dropped because the iso TX bank was full. |
|   7 | reserved                    | Always 0.                                                              |

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
|     `0x10` | IN  | `GEN_GET_CAPS`            | 0        | 0      | `Capabilities` (32 B) | Channel inventory + per-kind mode bitmask + sample-rate limits (§2.2). |
|     `0x11` | IN  | `GEN_GET_STATE`           | channel  | 0      | `ChannelState` (32 B) | Current spec + live phase for one channel (§2.3). |
|     `0x12` | OUT | `GEN_PLAY_BUILTIN`        | channel  | 0      | `WaveBuiltinSpec` (16 B) | Start a built-in waveform, **or** seamlessly update the parameters of an already-running one (phase preserved). |
|     `0x13` | OUT | `GEN_PLAY_ARBITRARY`      | channel  | 0      | `WaveArbHeader` (8 B) + `int16_t samples[N]` | Start (or replace) replay of an uploaded sample buffer. |
|     `0x14` | OUT | `GEN_STOP`                | channel  | 0      | none      | Stop the generator. Streaming `Command` frames immediately resume driving the channel. |
|     `0x18` | OUT | `DAC_SET_CLOCK`           | 0        | 0      | `u32 clock_hz` (4 B)  | Set the **shared** DAC sample clock in Hz. Range 1 – 1_000_000. Both DAC channels share this clock; per-channel signal frequency is independent (set via `freq_mHz` in `WaveBuiltinSpec`). |
|     `0x19` | IN  | `DAC_GET_CLOCK`           | 0        | 0      | `u32 clock_hz` (4 B)  | Read current shared DAC sample clock. |
|     `0x20` | OUT | `ADC_SET_RATE`            | 0        | 0      | `u32 rate_hz` (4 B)  | Set ADC sampling rate in Hz. Range 1 – 1_000_000. Independent of DAC clock. |
|     `0x21` | IN  | `ADC_GET_RATE`            | 0        | 0      | `u32 rate_hz` (4 B)  | Read current ADC rate. |
|     `0x30` | OUT | `GEN_PLAY_LUT`            | channel  | 0      | `WaveLutSpec` (12 B) + `int16_t entries[N]` | Reactive look-up: `output = LUT[input]`, where input is a 12-bit value derived from an ADC channel or a mask of DIN bits. Reaction time: ~1 µs (DIN PIO ISR) or ~5 µs (ADC EOC ISR). N up to 4096 (12-bit input). |
|     `0x31` | OUT | `GEN_PLAY_THRESHOLD`      | channel  | 0      | `WaveThresholdSpec` (16 B) | Reactive comparator with optional hysteresis. Output toggles between `val_high` and `val_low` based on `input` vs `thr_high` / `thr_low`. |
|     `0x32` | OUT | `GEN_PLAY_PULSE_TRIG`     | channel  | 0      | `WavePulseSpec` (12 B) | Reactive monostable: on edge of `input_din_bit`, drive `output` `active_level` for `duration_us`, then return to idle. |
|     `0x38` | OUT | `GEN_PLAY_PID` *(v3)*     | channel  | 0      | `WavePidSpec` (32 B)  | Reactive PID closed-loop: `output = clamp(Kp·err + Ki·∫err + Kd·d(err)/dt)` where `err = setpoint − adc[input_ch]`. |

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

### 2.2 `GEN_GET_CAPS` payload (32 bytes, all fields little-endian)

All structures in this section are designed for **direct memcpy
encode/decode**: every field is naturally aligned within the struct,
the wire layout is identical to the C / Rust struct memory layout, and
both the host (x86_64) and the SAM3X (Cortex-M3) use little-endian.
The `__attribute__((packed))` qualifier is applied as defence in
depth — given the natural alignment it does not introduce any extra
load instructions.

```c
struct __attribute__((packed)) Capabilities {  // 32 bytes
    /* offset 0 */
    uint8_t  protocol_version;     //  1 in this revision
    uint8_t  reserved0;            //  must be 0
    uint16_t firmware_minor;       //  e.g. semver minor.patch packed
    uint16_t firmware_major;
    uint16_t reserved1;
    /* offset 8 — channel counts */
    uint8_t  num_dac;              //  e.g. 2
    uint8_t  num_pwm;              //  e.g. 8
    uint8_t  num_dout;             //  e.g. 16
    uint8_t  num_din;              //  e.g. 16
    uint8_t  num_adc;              //  e.g. 8
    uint8_t  reserved2[3];
    /* offset 16 — per-kind supported-mode bitmask */
    uint8_t  modes_dac;            //  e.g. 0x07 = MANUAL|BUILTIN|ARBITRARY
    uint8_t  modes_pwm;            //  v1: 0x01 (MANUAL only)
    uint8_t  modes_dout;           //  v1: 0x01
    uint8_t  modes_din;            //  0 (read-only)
    uint8_t  modes_adc;            //  0 (read-only)
    uint8_t  reserved3[3];
    /* offset 24 — limits */
    uint32_t max_dac_sample_rate_hz;  //  typ. 1_000_000
    uint32_t max_arb_buffer_samples;  //  typ. 1024
};
```

#### Mode bitmask (`modes_*`)

| Bit | Macro            | Meaning                                          |
| --: | ---------------- | ------------------------------------------------ |
|   0 | `MODE_MANUAL`    | The streaming `Command` field drives this channel. |
|   1 | `MODE_BUILTIN`   | `GEN_PLAY_BUILTIN` accepted on this kind.        |
|   2 | `MODE_ARBITRARY` | `GEN_PLAY_ARBITRARY` accepted on this kind.      |
| 3-7 | reserved         | Must be ignored by the host (set to 0 by v1 firmware; reserved for future modes such as `MODE_PWM_DUTY`, `MODE_TC_TOGGLE`, `MODE_CLOSED_LOOP`). |

The host should always check the relevant bit before issuing a
`GEN_PLAY_*` request — for example, `caps.modes_pwm & MODE_BUILTIN`
returns 0 in v1 firmware, and the host must surface that as
"unsupported on this firmware revision" rather than blindly STALLing.

### 2.3 `WaveBuiltinSpec` / `WaveArbHeader` / `ChannelState`

```c
typedef enum : uint8_t {
    /* Open-loop (output = f(time))                              */
    SHAPE_OFF       = 0,   // channel is "manual": streaming Command frame drives it
    SHAPE_DC        = 1,   // generator-driven, constant level at `offset`
    SHAPE_SINE      = 2,
    SHAPE_SQUARE    = 3,
    SHAPE_TRIANGLE  = 4,
    SHAPE_SAWTOOTH  = 5,
    SHAPE_ARBITRARY = 6,   // generator-driven from an uploaded buffer

    /* Reactive (output = f(inputs)) — see §3.2                  */
    SHAPE_LUT        = 16, // output = LUT[input], pre-computed table
    SHAPE_THRESHOLD  = 17, // output = (input ⋛ threshold) ? hi : lo
    SHAPE_PULSE_TRIG = 18, // output = monostable triggered by DIN edge
    SHAPE_PID        = 19, // output = PID(input, setpoint)   [v3]

    /* 7..15 reserved for future open-loop shapes (e.g. ramp,
     * AM-modulated, sweep). 20..31 reserved for future reactive
     * modes (RULE_CHAIN, etc.). Hosts must treat unknown shape
     * codes returned by GEN_GET_STATE as opaque — see §7. */
} wave_shape_t;

typedef enum : uint8_t {
    CHAN_KIND_DAC  = 0,
    CHAN_KIND_PWM  = 1,
    CHAN_KIND_DOUT = 2,
    CHAN_KIND_DIN  = 3,
    CHAN_KIND_ADC  = 4,
} channel_kind_t;

struct __attribute__((packed)) WaveBuiltinSpec {  // 16 bytes
    /* offset  0 */ uint8_t  shape;        // wave_shape_t (SHAPE_OFF or SHAPE_ARBITRARY invalid here)
    /* offset  1 */ uint8_t  flags;        // reserved bits for future features (must be 0 in v1)
    /* offset  2 */ uint16_t duty_x10;     // 0–1000 = 0.0–100.0 % (SQUARE only; ignored otherwise)
    /* offset  4 */ uint16_t amplitude;    // peak-to-peak, raw device units (DAC: 0–4095)
    /* offset  6 */ uint16_t offset;       // mid-point, raw device units
    /* offset  8 */ uint32_t freq_mHz;     // signal frequency, milli-Hz (1 mHz = 0.001 Hz)
    /* offset 12 */ uint16_t phase_offset_x16; // 0–65535 = 0–360°. Reserved for v2 phase-locked
                                               //   multi-channel start; must be 0 in v1.
    /* offset 14 */ uint16_t reserved1;    // must be 0
};

struct __attribute__((packed)) WaveArbHeader {    // 8 bytes header
    /* offset  0 */ uint16_t n_samples;    // 1..max_arb_buffer_samples
    /* offset  2 */ uint16_t loop_count;   // 0 = infinite; otherwise plays N times then SHAPE_OFF
    /* offset  4 */ uint32_t sample_rate_hz; // playback rate, must be ≤ max_dac_sample_rate_hz
    // followed by int16_t samples[n_samples]
};

/* ---------- Reactive-mode payloads ---------- */

/* Input-source descriptor used by all reactive modes that read a
 * device input. Shared between LUT / THRESHOLD / PID. */
typedef enum : uint8_t {
    INPUT_SRC_NONE     = 0,
    INPUT_SRC_ADC      = 1,    // input = adc[arg & 7], 12-bit
    INPUT_SRC_DIN_MASK = 2,    // input = bits-of-DIN selected by `arg` bitmask,
                               //   packed LSB-first. e.g. arg=0b0000_0000_0000_0111
                               //   maps DIN0..DIN2 to bits 0..2 of the LUT index.
} input_src_t;

struct __attribute__((packed)) WaveLutSpec {        // 12 bytes header + entries
    /* offset  0 */ uint8_t  input_src;         // input_src_t
    /* offset  1 */ uint8_t  reserved0;
    /* offset  2 */ uint16_t input_arg;         // ADC channel (low 3 bits) or DIN bitmask
    /* offset  4 */ uint16_t n_entries;         // 1..4096; index width = ceil(log2(n_entries))
    /* offset  6 */ uint16_t output_mask;       // for DOUT outputs: bits to drive (=0xFFFF for "all 16")
                                                //   for DAC outputs: ignored (always 12-bit value used)
    /* offset  8 */ uint32_t reserved1;         // must be 0
    /* int16_t entries[n_entries] follows. For DAC outputs each entry is a
     * 0..4095 value; for DOUT outputs each entry is a 16-bit mask AND'd
     * with `output_mask` before driving the GPIO. */
};

struct __attribute__((packed)) WaveThresholdSpec {  // 16 bytes
    /* offset  0 */ uint8_t  input_src;         // input_src_t (typ. INPUT_SRC_ADC)
    /* offset  1 */ uint8_t  reserved0;
    /* offset  2 */ uint16_t input_arg;
    /* offset  4 */ uint16_t thr_high;          // upper threshold (raw input units)
    /* offset  6 */ uint16_t thr_low;           // lower threshold (≤ thr_high; equal = no hysteresis)
    /* offset  8 */ uint16_t val_high;          // output value when input > thr_high
    /* offset 10 */ uint16_t val_low;           // output value when input < thr_low
    /* offset 12 */ uint32_t reserved1;
};

typedef enum : uint8_t {
    PULSE_EDGE_RISING  = 1,
    PULSE_EDGE_FALLING = 2,
    PULSE_EDGE_ANY     = 3,
} pulse_edge_t;

struct __attribute__((packed)) WavePulseSpec {      // 12 bytes
    /* offset  0 */ uint8_t  input_din_bit;     // 0..15: which DIN bit triggers
    /* offset  1 */ uint8_t  edge;              // pulse_edge_t
    /* offset  2 */ uint8_t  active_level;      // 0 or 1: output level during pulse
    /* offset  3 */ uint8_t  reserved0;
    /* offset  4 */ uint32_t duration_us;       // 1..1_000_000 µs
    /* offset  8 */ uint32_t cooldown_us;       // 0..1_000_000 µs; re-triggers within cooldown ignored
};

struct __attribute__((packed)) WavePidSpec {        // 32 bytes (v3, slot reserved)
    /* offset  0 */ uint8_t  input_src;         // typ. INPUT_SRC_ADC
    /* offset  1 */ uint8_t  reserved0;
    /* offset  2 */ uint16_t input_arg;
    /* offset  4 */ uint32_t sample_rate_hz;    // PID loop frequency (≤ ADC rate)
    /* offset  8 */ int32_t  setpoint;          // raw input units (signed for offset)
    /* offset 12 */ int32_t  kp_q16_16;         // gains in Q16.16 fixed-point
    /* offset 16 */ int32_t  ki_q16_16;
    /* offset 20 */ int32_t  kd_q16_16;
    /* offset 24 */ uint16_t out_min;           // output clamp (DAC units 0..4095)
    /* offset 26 */ uint16_t out_max;
    /* offset 28 */ int32_t  integral_clamp;    // anti-windup limit on integral term
};

struct __attribute__((packed)) ChannelState {     // 32 bytes
    /* offset  0 */ uint8_t  channel_kind;       // channel_kind_t
    /* offset  1 */ uint8_t  channel_index;      // 0..num_<kind>-1
    /* offset  2 */ uint8_t  shape;              // wave_shape_t (SHAPE_OFF = manual)
    /* offset  3 */ uint8_t  flags;              // mirrors WaveBuiltinSpec.flags
    /* offset  4 */ uint32_t freq_mHz;           // 0 if SHAPE_OFF or SHAPE_ARBITRARY
    /* offset  8 */ uint16_t duty_x10;
    /* offset 10 */ uint16_t amplitude;
    /* offset 12 */ uint16_t offset;
    /* offset 14 */ uint16_t phase_offset_x16;
    /* offset 16 */ uint16_t arb_n_samples;      // 0 if not arbitrary
    /* offset 18 */ uint16_t arb_loops_remaining; // 0 = infinite or N/A
    /* offset 20 */ uint32_t arb_sample_rate_hz;
    /* offset 24 */ uint32_t cur_phase_q24_8;    // current phase as Q24.8 fixed-point
                                                  //   (0..0x100_0000 = 0..1.0 of a cycle).
                                                  //   Useful for diagnostics & sync verification.
    /* offset 28 */ uint32_t reserved;
};
```

`shape` doubles as the mode indicator: `SHAPE_OFF` → manual, any
other shape → generator-driven. No separate `mode` byte.

#### Encode / decode performance

All four control-plane structures (`Capabilities`, `ChannelState`,
`WaveBuiltinSpec`, `WaveArbHeader`) are designed to be:

- **memcpy-encoded** on the host (no field-by-field serialisation).
- **memcpy-decoded** on the SAM3X (no byte-swap, no alignment fixups).
- **Naturally aligned**: every 16-bit field on a 16-bit boundary,
  every 32-bit field on a 32-bit boundary. `__attribute__((packed))`
  is therefore a no-op in compiler terms but acts as a contract
  guarantee against silent layout drift.
- **Power-of-two sized** (16, 24, 32 bytes), to fit cleanly in EP0
  control packets (which are 64-byte max on full/high speed).

Decode cost on the SAM3X is bounded:
| Struct           | Size  | Cortex-M3 load instructions |
| ---------------- | ----: | --------------------------: |
| `Capabilities`   | 32 B  | 8 × `LDR` (32-bit aligned)  |
| `ChannelState`   | 32 B  | 8 × `LDR`                   |
| `WaveBuiltinSpec`| 16 B  | 4 × `LDR`                   |
| `WaveArbHeader`  | 8 B   | 2 × `LDR`                   |

For the streaming data plane (PhyCMD-64) the layout is unchanged from
prior firmware revisions; encode/decode there is dominated by the
CRC-16-CCITT computation (~14 cycles/byte with table) rather than the
field copies.

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

The firmware fast-rejects malformed requests by **STALL**ing the EP0
control transfer. The host's `libusb_control_transfer` returns
`LIBUSB_ERROR_PIPE` in that case. Validation order, fail on the first
match:

| # | Condition                                              | Firmware response                |
| - | ------------------------------------------------------ | -------------------------------- |
| 1 | Unknown `bRequest`                                     | STALL                            |
| 2 | `wLength` doesn't match the expected struct size for the request | STALL                  |
| 3 | `wIndex` out of `0..num_channels-1` range              | STALL                            |
| 4 | Channel kind doesn't have the requested mode bit set in `Capabilities.modes_<kind>` | STALL |
| 5 | `WaveBuiltinSpec.shape ∈ {SHAPE_OFF, SHAPE_ARBITRARY}` (use `GEN_STOP` / `GEN_PLAY_ARBITRARY` instead) | STALL |
| 6 | `WaveBuiltinSpec.flags`, `phase_offset_x16`, or any reserved field nonzero in v1 | STALL (forces forward-compat hygiene) |
| 7 | `WaveArbHeader.n_samples == 0` or `> max_arb_buffer_samples` | STALL (no partial write)   |
| 8 | `WaveArbHeader.sample_rate_hz > max_dac_sample_rate_hz` | STALL                           |
| 9 | DAC clock or ADC rate request out of `1..1_000_000` range | STALL                          |
| 10 | `freq_mHz` × oversample exceeds Nyquist                | accepted (aliasing is the user's problem; firmware logs a debug warning) |

A successful `GEN_PLAY_*` request applies the new configuration
atomically: the next ping-pong buffer fill picks up the new spec, so
parameter sweeps are seamless (no glitch on the analog output).
`GEN_STOP` takes effect at the next TC trigger (≤ one DAC sample
period) — typically ≤ 1 µs.

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

### 3.2 Reactive modes — closed-loop on the device

The reactive shapes (`SHAPE_LUT`, `SHAPE_THRESHOLD`, `SHAPE_PULSE_TRIG`,
`SHAPE_PID`) compute their output on the SAM3X side from one of the
device inputs (an ADC channel or a DIN bit mask). They run **inside
ISRs** rather than off the DAC PDC, so:

| Reactive mode    | Triggered by         | Reaction time | Per-instance state         |
| ---------------- | -------------------- | ------------- | -------------------------- |
| `SHAPE_LUT`      | ADC EOC / DIN PIO    | ~1 µs (DIN) / ~5 µs (ADC) | 12-byte spec + LUT entries (≤ 8 KB)        |
| `SHAPE_THRESHOLD`| ADC EOC              | ~5 µs         | 16-byte spec + 1 byte hysteresis state     |
| `SHAPE_PULSE_TRIG`| DIN PIO + dedicated TC compare | ~1 µs trigger; pulse width sub-µs accurate | 12-byte spec + 1 TC channel  |
| `SHAPE_PID`      | TC trigger at sample_rate_hz | 1 sample period | 32-byte spec + 12 bytes integrator state |

**Resource limits enforced by the firmware** (advertised through new
`Capabilities` fields, see §2.2 v2 update):

| Reactive mode      | Concurrent instances | Reason                               |
| ------------------ | -------------------- | ------------------------------------ |
| `SHAPE_LUT`        | 3 simultaneously     | Pre-allocated 24 KB SRAM (3 × 8 KB)  |
| `SHAPE_THRESHOLD`  | All output channels  | < 20 bytes/ch state                  |
| `SHAPE_PULSE_TRIG` | 4 simultaneously     | Each needs a TC channel (TC1_CH0..2 + TC2_CH0) |
| `SHAPE_PID` *(v3)* | 2 (one per DAC)      | Each needs a dedicated TC sample tick |

### 3.3 The `apply_command_frame` mode-aware filter

Once any channel is in a non-`SHAPE_OFF` state (open-loop or reactive),
the firmware ignores the corresponding field in the streaming
`Command` frame. This is the **same rule** for both open-loop and
reactive modes — the host doesn't need to know which kind of
generator is active to keep streaming PhyCMD-64 frames safely. The
"silent ignore" is implemented as a single check
(`if (channel.shape != SHAPE_OFF) skip`) and adds 2 cycles per channel
to the apply path, negligible.

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

## 6. Reference: shared type definitions (firmware & host)

These declarations are the **single source of truth** for the wire
layout. Firmware (`ATSAM3X8E_FW/src/waveform.h`) and host
(`physerver/crates/phycmd-core/src/protocol/wave_types.rs`) MUST stay
byte-identical with this section. The `static_assert` lines verify
struct sizes at compile time on both sides — break one, the build
fails immediately.

### 6.1 C (firmware)

```c
#pragma once
#include <stdint.h>

/* bRequest opcodes (vendor SETUP) ------------------------------- */
#define VREQ_GEN_GET_CAPS         0x10
#define VREQ_GEN_GET_STATE        0x11
#define VREQ_GEN_PLAY_BUILTIN     0x12
#define VREQ_GEN_PLAY_ARBITRARY   0x13
#define VREQ_GEN_STOP             0x14
#define VREQ_DAC_SET_CLOCK        0x18
#define VREQ_DAC_GET_CLOCK        0x19
#define VREQ_ADC_SET_RATE         0x20
#define VREQ_ADC_GET_RATE         0x21
/* Reactive (closed-loop) modes added in v2 */
#define VREQ_GEN_PLAY_LUT         0x30
#define VREQ_GEN_PLAY_THRESHOLD   0x31
#define VREQ_GEN_PLAY_PULSE_TRIG  0x32
#define VREQ_GEN_PLAY_PID         0x38   /* v3 — slot reserved */

/* Wave shape ---------------------------------------------------- */
typedef enum : uint8_t {
    /* Open-loop                                       */
    SHAPE_OFF = 0, SHAPE_DC, SHAPE_SINE, SHAPE_SQUARE,
    SHAPE_TRIANGLE, SHAPE_SAWTOOTH, SHAPE_ARBITRARY,
    /* Reactive (closed-loop), gap from 7..15 reserved */
    SHAPE_LUT = 16, SHAPE_THRESHOLD, SHAPE_PULSE_TRIG, SHAPE_PID,
} wave_shape_t;

/* Input source for reactive modes ------------------------------- */
typedef enum : uint8_t {
    INPUT_SRC_NONE = 0, INPUT_SRC_ADC = 1, INPUT_SRC_DIN_MASK = 2,
} input_src_t;

/* Pulse edge selector ------------------------------------------- */
typedef enum : uint8_t {
    PULSE_EDGE_RISING = 1, PULSE_EDGE_FALLING = 2, PULSE_EDGE_ANY = 3,
} pulse_edge_t;

/* Channel kind -------------------------------------------------- */
typedef enum : uint8_t {
    CHAN_KIND_DAC = 0, CHAN_KIND_PWM, CHAN_KIND_DOUT,
    CHAN_KIND_DIN, CHAN_KIND_ADC,
} channel_kind_t;

/* Mode bitmask (one bit per supported mode) -------------------- */
#define MODE_MANUAL    (1u << 0)
#define MODE_BUILTIN   (1u << 1)
#define MODE_ARBITRARY (1u << 2)

/* Streaming Command flags -------------------------------------- */
#define FLAG_ADC_ENABLE       (1u << 0)
#define FLAG_DAC_ENABLE       (1u << 1)
#define FLAG_PWM_ENABLE       (1u << 2)
#define FLAG_RESET_SEQ        (1u << 3)
#define FLAG_WATCHDOG_DISABLE (1u << 4)

/* Status flags -------------------------------------------------- */
#define STATUS_ADC_ACTIVE         (1u << 0)
#define STATUS_DAC_ACTIVE         (1u << 1)
#define STATUS_PWM_ACTIVE         (1u << 2)
#define STATUS_ERROR              (1u << 3)
#define STATUS_WATCHDOG_TRIGGERED (1u << 4)
#define STATUS_USB_CONFIGURED     (1u << 5)
#define STATUS_OVERRUN            (1u << 6)

/* Vendor SETUP payloads ---------------------------------------- */
struct __attribute__((packed)) Capabilities {
    uint8_t  protocol_version, reserved0;
    uint16_t firmware_minor, firmware_major, reserved1;
    uint8_t  num_dac, num_pwm, num_dout, num_din, num_adc;
    uint8_t  reserved2[3];
    uint8_t  modes_dac, modes_pwm, modes_dout, modes_din, modes_adc;
    uint8_t  reserved3[3];
    uint32_t max_dac_sample_rate_hz;
    uint32_t max_arb_buffer_samples;
};
_Static_assert(sizeof(struct Capabilities) == 32, "Capabilities size");

struct __attribute__((packed)) WaveBuiltinSpec {
    uint8_t  shape, flags;
    uint16_t duty_x10, amplitude, offset;
    uint32_t freq_mHz;
    uint16_t phase_offset_x16, reserved1;
};
_Static_assert(sizeof(struct WaveBuiltinSpec) == 16, "WaveBuiltinSpec size");

struct __attribute__((packed)) WaveArbHeader {
    uint16_t n_samples, loop_count;
    uint32_t sample_rate_hz;
    /* int16_t samples[n_samples] follows */
};
_Static_assert(sizeof(struct WaveArbHeader) == 8, "WaveArbHeader size");

/* Reactive-mode payloads (v2) */
struct __attribute__((packed)) WaveLutSpec {        /* 12 B header + entries */
    uint8_t  input_src; uint8_t reserved0;
    uint16_t input_arg, n_entries, output_mask;
    uint32_t reserved1;
    /* int16_t entries[n_entries] follows */
};
_Static_assert(sizeof(struct WaveLutSpec) == 12, "WaveLutSpec size");

struct __attribute__((packed)) WaveThresholdSpec {  /* 16 B */
    uint8_t  input_src; uint8_t reserved0;
    uint16_t input_arg, thr_high, thr_low, val_high, val_low;
    uint32_t reserved1;
};
_Static_assert(sizeof(struct WaveThresholdSpec) == 16, "WaveThresholdSpec size");

struct __attribute__((packed)) WavePulseSpec {      /* 12 B */
    uint8_t  input_din_bit, edge, active_level, reserved0;
    uint32_t duration_us, cooldown_us;
};
_Static_assert(sizeof(struct WavePulseSpec) == 12, "WavePulseSpec size");

struct __attribute__((packed)) WavePidSpec {        /* 32 B (v3) */
    uint8_t  input_src; uint8_t reserved0;
    uint16_t input_arg;
    uint32_t sample_rate_hz;
    int32_t  setpoint, kp_q16_16, ki_q16_16, kd_q16_16;
    uint16_t out_min, out_max;
    int32_t  integral_clamp;
};
_Static_assert(sizeof(struct WavePidSpec) == 32, "WavePidSpec size");

struct __attribute__((packed)) ChannelState {
    uint8_t  channel_kind, channel_index, shape, flags;
    uint32_t freq_mHz;
    uint16_t duty_x10, amplitude, offset, phase_offset_x16;
    uint16_t arb_n_samples, arb_loops_remaining;
    uint32_t arb_sample_rate_hz;
    uint32_t cur_phase_q24_8;
    uint32_t reserved;
};
_Static_assert(sizeof(struct ChannelState) == 32, "ChannelState size");
```

### 6.2 Rust (host)

```rust
#[repr(u8)] #[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WaveShape {
    Off = 0, Dc, Sine, Square, Triangle, Sawtooth, Arbitrary,
}

#[repr(u8)] #[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChannelKind { Dac = 0, Pwm, Dout, Din, Adc }

bitflags::bitflags! {
    #[repr(transparent)]
    pub struct ModeMask: u8 {
        const MANUAL    = 1 << 0;
        const BUILTIN   = 1 << 1;
        const ARBITRARY = 1 << 2;
    }
}

#[repr(C, packed)] #[derive(Copy, Clone, Debug)]
pub struct Capabilities {
    pub protocol_version: u8, pub _r0: u8,
    pub firmware_minor: u16, pub firmware_major: u16, pub _r1: u16,
    pub num_dac: u8, pub num_pwm: u8, pub num_dout: u8,
    pub num_din: u8, pub num_adc: u8, pub _r2: [u8; 3],
    pub modes_dac: u8, pub modes_pwm: u8, pub modes_dout: u8,
    pub modes_din: u8, pub modes_adc: u8, pub _r3: [u8; 3],
    pub max_dac_sample_rate_hz: u32,
    pub max_arb_buffer_samples: u32,
}
const _: () = assert!(std::mem::size_of::<Capabilities>() == 32);

#[repr(C, packed)] #[derive(Copy, Clone, Debug)]
pub struct WaveBuiltinSpec {
    pub shape: u8, pub flags: u8,
    pub duty_x10: u16, pub amplitude: u16, pub offset: u16,
    pub freq_mhz: u32,
    pub phase_offset_x16: u16, pub _r1: u16,
}
const _: () = assert!(std::mem::size_of::<WaveBuiltinSpec>() == 16);

#[repr(C, packed)] #[derive(Copy, Clone, Debug)]
pub struct WaveArbHeader {
    pub n_samples: u16, pub loop_count: u16,
    pub sample_rate_hz: u32,
}
const _: () = assert!(std::mem::size_of::<WaveArbHeader>() == 8);

#[repr(C, packed)] #[derive(Copy, Clone, Debug)]
pub struct ChannelState {
    pub channel_kind: u8, pub channel_index: u8, pub shape: u8, pub flags: u8,
    pub freq_mhz: u32,
    pub duty_x10: u16, pub amplitude: u16, pub offset: u16, pub phase_offset_x16: u16,
    pub arb_n_samples: u16, pub arb_loops_remaining: u16,
    pub arb_sample_rate_hz: u32,
    pub cur_phase_q24_8: u32,
    pub _r: u32,
}
const _: () = assert!(std::mem::size_of::<ChannelState>() == 32);
```

Direct memcpy / `unsafe { ptr::read(buf as *const _) }` is safe in
both directions because:
* both architectures are little-endian,
* every field is naturally aligned within the struct,
* no field requires byte-swapping.

## 7. Compatibility / versioning

`Capabilities.protocol_version` starts at `1`. Backwards-incompatible
changes will bump this byte; backwards-compatible additions (new shape
codes, new channel kinds, new `bRequest` values, new mode bits in
`modes_*`) do not. Hosts should:

* always check `caps.protocol_version >= 1` before issuing any
  vendor SETUP request,
* always check `caps.modes_<kind> & MODE_<kind>` before relying on a
  mode being supported,
* gracefully ignore unknown shape codes returned by `GEN_GET_STATE`
  and treat them as "unknown / read-only".

Reserved bytes/fields **must be zero on send** and **must be ignored
on receive** by current-revision implementations. The firmware
enforces "must be zero on send" by STALLing requests with non-zero
reserved fields (see §2.5 row 6) — this prevents host code from
silently relying on unspecified bits that a future revision may use.
