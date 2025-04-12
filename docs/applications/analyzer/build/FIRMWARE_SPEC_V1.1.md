# Firmware specification — phycommander v1.1 for the analyzer

> This document specifies the minimum set of firmware changes required
> on the ATSAM3X8E (Arduino Due) to support the multi-function analyzer.
> It is addressed to whoever maintains `ATSAM3X8E_FW/` and is written as
> an implementation reference, not as a design exploration.
>
> **Version target**: phycommander firmware **v1.1**
> **Base**: phycommander firmware v1.0 (as of git tag `v1.0`)
> **Scope**: PWM peripheral exposure + laser safety interlock + protocol
> field additions
> **Depends on**: nothing (self-contained)
> **Unblocks**: the entire analyzer project, the laser-based beer/water
> haze measurement, and the published roadmap for firmware v1.2.

---

## 1. Summary of required additions

1. **Expose two hardware PWM channels** (PWM0, PWM1) with host-programmable
   frequency and duty cycle
2. **Implement a hardware-gated safety interlock** on DIN5 that
   unconditionally disables DOUT3, DOUT4, and PWM0 output when the
   interlock reads "open"
3. **Add three new fields to the 64-byte command packet** and one new
   field to the 64-byte status packet to carry the PWM parameters and the
   interlock state
4. **Add one new status flag** (`INTERLOCK_OPEN`) to the existing status
   bitfield

No change to the 64-byte packet layout *size*, no change to the CRC
algorithm, no change to the USB/serial transport. This update is
backwards compatible for any host that simply ignores the new fields.

---

## 2. PWM peripheral configuration

### 2.1 Pin assignment

| Logical channel | Arduino Due pin | ATSAM3X8E pin | PWM block | Notes |
|---|---|---|---|---|
| **PWM0** | D6 | PC24 / PWMH7 | PWM block 0, channel 7 | |
| **PWM1** | D7 | PC23 / PWMH6 | PWM block 0, channel 6 | |

Rationale: pins D6 and D7 on the Due are already routed to the PWM
peripheral on PC24 and PC23 respectively, so no board rework is
required. These are the same pins that Arduino's `analogWrite()`
library uses.

### 2.2 PWM clock source

Use the 84 MHz master clock (`PWM_MCK`) as the PWM input. The PWM block
has an internal prescaler; use it to generate the desired output
frequency.

For a target output frequency `f_out`:

```
period_count = f_PWM_clock / f_out
f_PWM_clock = 84 MHz / prescaler
```

For **f_out ≥ 20 Hz** and **f_out ≤ 50 kHz** (the range the analyzer
uses), pick a prescaler that gives `period_count` in the range
~1680 – 4.2 × 10⁶, i.e., plenty of resolution.

**Recommended approach**: fix the prescaler at CLKA divider = 8 (so
`f_PWM_clock` = 10.5 MHz), then compute `period_count = 10_500_000 /
f_out` on each frequency update. This gives 16-bit resolution on the
period and 16-bit resolution on the duty cycle independently.

### 2.3 Duty cycle

Duty cycle is specified as a 16-bit unsigned integer, where `0` means
always-low and `65535` means always-high:

```
duty_count = (duty_u16 / 65535) × period_count
```

When the host sets `duty_u16 = 32768` it expects exactly 50.0 % duty —
verify the math handles the edge cases.

### 2.4 Output enable

A separate enable bit controls whether the PWM output drives the pin or
not. When disabled, the pin is driven low (not high-impedance), so that
the downstream MOSFET gate is pulled to ground.

### 2.5 API (firmware internal)

```c
// In phyextension/pwm.h (new file)

#include <stdint.h>
#include <stdbool.h>

typedef struct {
    uint16_t freq_hz;      // 0 = disabled, else target output frequency
    uint16_t duty_u16;     // 0..65535, duty cycle Q16
    bool     enabled;
    bool     gated;         // set by safety_loop() if interlock open
} pwm_channel_t;

extern pwm_channel_t g_pwm[2];  // PWM0, PWM1

void pwm_init(void);
void pwm_set(uint8_t channel, uint16_t freq_hz, uint16_t duty_u16,
             bool enable);
void pwm_force_off(uint8_t channel);   // used by safety_loop
```

Implementation outline (pseudo-code):

```c
void pwm_set(uint8_t ch, uint16_t freq_hz, uint16_t duty_u16, bool en) {
    if (ch >= 2) return;
    g_pwm[ch].freq_hz = freq_hz;
    g_pwm[ch].duty_u16 = duty_u16;
    g_pwm[ch].enabled = en;
    if (en && freq_hz > 0 && !g_pwm[ch].gated) {
        uint32_t channel_peripheral =
            (ch == 0) ? PWM_CHANNEL_7 : PWM_CHANNEL_6;
        uint32_t period = 10500000UL / freq_hz;
        if (period < 2) period = 2;
        uint32_t duty = ((uint32_t)duty_u16 * period) / 65535UL;
        PWMC_SetPeriod(PWM, channel_peripheral, period);
        PWMC_SetDutyCycle(PWM, channel_peripheral, duty);
        PWMC_EnableChannel(PWM, channel_peripheral);
    } else {
        uint32_t channel_peripheral =
            (ch == 0) ? PWM_CHANNEL_7 : PWM_CHANNEL_6;
        PWMC_DisableChannel(PWM, channel_peripheral);
        // drive pin low
        pio_set_output(PIOC, (ch == 0) ? PIO_PC24 : PIO_PC23,
                       LOW, DISABLE, DISABLE);
    }
}
```

---

## 3. Safety interlock

### 3.1 Hardware interface

| Signal | Arduino Due pin | Function |
|---|---|---|
| **DIN5** (interlock sense) | D5 / PC25 | Input with pull-up; low when cover closed, high when open |
| **DOUT3** (laser modulation) | D3 / PC28 | Output driving the laser modulation MOSFET gate |
| **DOUT4** (laser master enable) | D4 / PC26 | Output driving the laser master-enable MOSFET + external safety relay coil |

### 3.2 Firmware logic

Run the safety check on **every main loop iteration**, before any other
host-commanded output is applied.

```c
// In phyextension/safety.c (new file)

#include "pwm.h"
#include "dout.h"
#include "din.h"

static bool s_interlock_armed = false;

void safety_loop(void) {
    bool cover_closed = (din_read(5) == 0);  // active-low

    if (!cover_closed) {
        // Force laser-related outputs off, regardless of host command
        pwm_force_off(0);              // PWM0 (if used for laser carrier)
        dout_force_clear(3);           // DOUT3 laser modulation
        dout_force_clear(4);           // DOUT4 laser master enable
        g_pwm[0].gated = true;
        s_interlock_armed = false;
        g_status_flags |= STATUS_INTERLOCK_OPEN;
    } else {
        g_pwm[0].gated = false;
        g_status_flags &= ~STATUS_INTERLOCK_OPEN;
        // "Spring-back arm": when the cover closes, the laser stays OFF
        // until the host explicitly sends a re-arm command. This prevents
        // accidental laser activation by closing the cover while the host
        // still has DOUT3/DOUT4 high in its last command.
        if (!s_interlock_armed) {
            dout_force_clear(3);
            dout_force_clear(4);
            if (host_sent_rearm_command()) {
                s_interlock_armed = true;
            }
        }
    }
}
```

### 3.3 Re-arm protocol

After `INTERLOCK_OPEN` has been observed, the host must send an explicit
re-arm command before laser output is allowed again. This is a new
command flag:

```c
#define CMD_FLAG_REARM_INTERLOCK  (1 << 15)
```

The host sets this flag for exactly one command packet after verifying
that `STATUS_INTERLOCK_OPEN` is cleared. The firmware's
`host_sent_rearm_command()` is true for one iteration when this flag
appears in the incoming command.

### 3.4 Interaction with existing DOUT handling

The existing loop-synchronous DOUT latch (v1.0 behavior) is preserved.
`dout_force_clear()` is a separate override that takes precedence over
the host-commanded DOUT value:

```c
void apply_host_dout_command(uint16_t host_dout_bitmap) {
    uint16_t effective = host_dout_bitmap & ~g_dout_force_clear_mask;
    uint16_t forced_high = g_dout_force_set_mask;
    dout_write(effective | forced_high);
}
```

---

## 4. Protocol additions

### 4.1 Existing command packet (64 bytes)

See [`../../../technical/PROTOCOL.md`](../../../technical/PROTOCOL.md) for the
v1.0 layout. The v1.1 additions fit inside the existing "reserved"
region at bytes 28–43 without breaking the layout.

### 4.2 New fields — command packet

```
Byte offset   Size  Name                 Type     Description
────────────   ───   ──────────────────   ──────   ──────────────────────
28            2     pwm[0].freq_hz       uint16  PWM0 frequency in Hz
30            2     pwm[0].duty_u16      uint16  PWM0 duty cycle Q16
32            1     pwm[0].flags         uint8   bit0 = enable
33            1     (reserved)           uint8   align
34            2     pwm[1].freq_hz       uint16  PWM1 frequency in Hz
36            2     pwm[1].duty_u16      uint16  PWM1 duty cycle Q16
38            1     pwm[1].flags         uint8   bit0 = enable
39            1     (reserved)           uint8   align
40            2     cmd_flags_ext        uint16  bit0 = CMD_FLAG_REARM_INTERLOCK
42            2     (reserved)           uint16  alignment / future use
```

**All remaining bytes (44–63)** are unchanged from v1.0.

### 4.3 New fields — status packet

```
Byte offset   Size  Name              Type     Description
────────────   ───   ───────────────   ──────   ─────────────────────────
44            2     status_flags_ext  uint16  bit0 = INTERLOCK_OPEN
46            2     loop_time_us      uint16  (existing, unchanged)
...
```

Status flag bit definitions:

```c
#define STATUS_INTERLOCK_OPEN       (1 << 0)
#define STATUS_PWM0_FAULT           (1 << 1)  // e.g., freq out of range
#define STATUS_PWM1_FAULT           (1 << 2)
#define STATUS_OVERCURRENT          (1 << 3)
#define STATUS_RESERVED_4_15        // for future use
```

### 4.4 CRC

The existing CRC-16-CCITT covers the entire 62-byte payload (followed by
the 2-byte CRC). Nothing changes — the new fields are inside the payload
and automatically covered.

### 4.5 Backward compatibility

A v1.0 host talking to a v1.1 firmware: the host never sets `pwm[*]` or
`cmd_flags_ext`, so those fields are zero, meaning PWM is disabled —
identical to v1.0 behavior. The new status bits are read by the v1.0
host as part of the generic status flags, which v1.0 already tolerates.

A v1.1 host talking to v1.0 firmware: the firmware ignores the new
command fields (it doesn't parse them), so PWM stays off. The host
detects this by reading the firmware version field and disables
laser-requiring modes.

### 4.6 Firmware version field

The existing firmware version field (byte 62 of the status packet in
v1.0) must be updated from `0x0100` to `0x0101` for v1.1. The host uses
this to gate the analyzer's laser-dependent modes.

---

## 5. Main loop integration

```c
// In phyextension/main.c, the existing main loop becomes:

void main_loop_iteration(void) {
    // 1. Read DIN values (existing)
    uint16_t din = din_read_all();

    // 2. Safety check — MUST run before any host-commanded output
    safety_loop();

    // 3. Receive host command packet (existing, via USB bulk or CDC)
    if (host_packet_available()) {
        command_packet_t cmd;
        host_read_packet(&cmd);
        if (crc_check(&cmd)) {
            // 4. Apply DOUT (existing, but goes through force-clear mask)
            apply_host_dout_command(cmd.digital_out);

            // 5. Apply DAC (existing)
            dac_write(0, cmd.dac[0]);
            dac_write(1, cmd.dac[1]);

            // 6. NEW: apply PWM configuration
            pwm_set(0, cmd.pwm[0].freq_hz, cmd.pwm[0].duty_u16,
                    cmd.pwm[0].flags & 1);
            pwm_set(1, cmd.pwm[1].freq_hz, cmd.pwm[1].duty_u16,
                    cmd.pwm[1].flags & 1);

            // 7. NEW: handle re-arm flag
            if (cmd.cmd_flags_ext & CMD_FLAG_REARM_INTERLOCK) {
                // safety_loop() reads this flag via host_sent_rearm_command()
            }
        }
    }

    // 8. DMA-buffered ADC sample (existing)
    adc_sample_t sample;
    adc_read_dma_circular(&sample);

    // 9. Construct and send status packet (existing + new fields)
    status_packet_t status;
    status.adc = sample;
    status.digital_in = din;
    status.status_flags_ext = g_status_flags;
    status.firmware_version = 0x0101;  // NEW
    status.loop_time_us = measure_loop_time_cyccnt();
    crc_append(&status);
    host_write_packet(&status);
}
```

---

## 6. Acceptance tests

The following tests must pass before v1.1 is released. All can be run
against the existing host-side `phycmd_selftest.py` with minor additions.

### 6.1 PWM frequency accuracy

1. Host commands `pwm[0].freq_hz = 1000, duty_u16 = 32768, enable = 1`.
2. Host measures the frequency on the PWM0 pin using an external
   oscilloscope or by wiring PWM0 to an ADC channel via a simple RC
   low-pass and running FFT on the sampled output.
3. **Pass**: measured frequency within ±0.1 % of 1000 Hz, duty cycle
   within ±1 % of 50 %.

### 6.2 PWM frequency range

Repeat 6.1 with `freq_hz` ∈ {20, 100, 500, 1000, 2000, 5000, 10000,
50000}. All must pass ±0.2 %.

### 6.3 Duty cycle resolution

Set `pwm[0].freq_hz = 1000`, sweep `duty_u16` from 0 to 65535 in 256
steps. Measure output duty cycle. **Pass**: linear response with
deviation < 0.1 % from expected at each point.

### 6.4 Safety interlock gating

1. Host commands laser on: `dout[3] = 1`, `dout[4] = 1`, `pwm[0]` at
   1 kHz enabled.
2. Open the cover (or short DIN5 to +3.3 V directly).
3. **Pass**: within one main loop iteration (< 100 µs),
   - DOUT3 output goes low, verified with scope
   - DOUT4 output goes low, verified with scope
   - PWM0 output goes low, verified with scope
   - `STATUS_INTERLOCK_OPEN` appears in the next status packet
4. Close the cover (DIN5 back to GND). Host confirms
   `STATUS_INTERLOCK_OPEN` is cleared.
5. Host continues commanding laser on, but outputs remain low — the
   re-arm protocol is in effect.
6. Host sends a command with `cmd_flags_ext |= CMD_FLAG_REARM_INTERLOCK`.
7. **Pass**: subsequent iterations, DOUT3, DOUT4, and PWM0 resume
   following the host command.

### 6.5 Spring-back protection

1. Host has laser commanded on.
2. Open and close the cover rapidly (within 10 ms).
3. **Pass**: laser outputs stay low even after close. Host must re-arm
   explicitly.

### 6.6 Loop time budget

Verify that the new PWM and safety code do not increase the main loop
execution time above the v1.0 baseline + 10 µs. Measured via CYCCNT.

**Pass**: worst-case loop time in v1.1 is ≤ v1.0 worst case + 10 µs.

### 6.7 CRC and framing

Run the existing CRC/framing test suite. All v1.0 tests must still pass.
New tests for the v1.1 field parsing must also pass.

### 6.8 Firmware version advertisement

Verify that the firmware version field in the status packet reads
`0x0101` after the v1.1 flash.

### 6.9 Backward-compat host test

Run a v1.0 host against v1.1 firmware. Verify that all v1.0 behavior is
preserved; PWM is off, status flags extension is zero, no behavior
regression.

### 6.10 Forward-compat host test

Run a v1.1 host (the analyzer Python code) against v1.0 firmware. Verify
that the host detects `firmware_version = 0x0100` and disables all
laser-dependent modes with a clear error message.

---

## 7. Firmware file changes summary

New files:

- `ATSAM3X8E_FW/phyextension/pwm.h`
- `ATSAM3X8E_FW/phyextension/pwm.c`
- `ATSAM3X8E_FW/phyextension/safety.h`
- `ATSAM3X8E_FW/phyextension/safety.c`

Modified files:

- `ATSAM3X8E_FW/phyextension/main.c` — main loop integration per §5
- `ATSAM3X8E_FW/phyextension/protocol.h` — add new fields, flags
- `ATSAM3X8E_FW/phyextension/protocol.c` — parse new fields
- `ATSAM3X8E_FW/phyextension/dout.c` — add force-clear/force-set masks
- `ATSAM3X8E_FW/phyextension/version.h` — bump to `0x0101`

Unchanged:

- CRC code
- USB bulk / CDC serial transport
- DMA ADC buffering
- DAC output code

Expected development time: 2 – 5 days for an engineer familiar with
the SAM3X peripheral library, including the acceptance tests.

---

## 8. Out of scope (for v1.1)

These items are **not** required for the analyzer base configuration
and are explicitly deferred to v1.2:

- I²C peripheral exposure (needed for module identification EEPROMs)
- SPI peripheral exposure (needed for external DAC expansion, SPI
  sensors)
- TC (Timer/Counter) quadrature decode (needed for turret encoder)
- Additional PWM channels beyond 2 (current design uses 2 only)
- Stepper motion controller (host handles stepping via GPIO)
- Hardware watchdog on the safety path (the current firmware-level
  check is sufficient for Class 3R)
- USART bridge
- CAN peripheral

The analyzer Phase 1 build (absorbance head, beer QC, multi-angle
nephelometer with software-toggled DOUT for the extra lasers) works
entirely with v1.1.

---

## 9. Related documents

- [`../../MULTIFUNCTION_ANALYZER.md`](../../MULTIFUNCTION_ANALYZER.md) §11 — broader firmware roadmap (v1.2, v1.3)
- [`../../BEER_ANALYZER.md`](../../BEER_ANALYZER.md) §7 — the same spec, condensed, embedded in the beer doc
- [`../../../technical/PROTOCOL.md`](../../../technical/PROTOCOL.md) — the full PhyCMD-64 protocol reference
- [`../../../firmware/FIRMWARE_UPDATES.md`](../../../firmware/FIRMWARE_UPDATES.md) — general firmware update tracking

---

*End of firmware spec v1.1. Questions or implementation notes → issues
on the phycommander repository.*
