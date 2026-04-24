# PhyCommander — Arduino Due pin assignment

Authoritative map of which SAM3X8E pin does what in the PhyCommander
firmware. When you physically wire the front panel to the Due, use
the **Arduino D-label** (the number silkscreened on the top layer of
the board); internal references in the firmware use the SAM3X PIO
symbol (e.g. `PIO_PC21_IDX`), which the Arduino label table below
translates.

![Arduino Due pinout reference](arduino-due-pinout.png)

See [`arduino-due-pinout.png`](arduino-due-pinout.png) for the
full-resolution reference (the Arduino.cc pinout diagram). Every D-
and A-label mentioned in the sections below is the one shown on
that image and silkscreened on the top layer of the board.

> **Conventions**
>
> - "Due label" = the number silkscreened on the Arduino Due PCB
>   (e.g. `D22`, `A0`, `DAC0`). This is what you read when you plug
>   in a jumper.
> - "SAM3X pin" = the chip-level port + bit (e.g. `PA14`, `PB27`).
> - "Protocol index" = the array slot in the phycmd wire frame
>   (`adc[3]`, `dout[7]`, …). These are what the REST / WebSocket
>   clients see.
> - All 8 ADC channels and both DACs are **live on every release**.
>   PWM, DIN, DOUT, SPI, I2C, CAN availability depends on firmware
>   version — see the "status" column.

---

## 1. Analog outputs (DAC)

| Wire field | Due label | SAM3X pin | Output range | Status |
|---|---|---|---|---|
| `dac[0]` | `DAC0` | PB15 | ~0.55–2.75 V | ✅ Active |
| `dac[1]` | `DAC1` | PB16 | ~0.55–2.75 V | ✅ Active |

> **Range caveat.** The Arduino Due's on-chip DAC is not rail-to-rail.
> Code `0` produces ~0.55 V, code `4095` produces ~2.75 V (ref:
> Atmel SAM3X datasheet §41.6.1). If you need 0–3.3 V swing you must
> add a buffer/opamp + gain stage off-board.

---

## 2. Analog inputs (ADC)

Arduino Due labels `A0`–`A11` (12 pins) but the firmware wires only 8
channels. **Important:** Arduino's A-label is *reversed* relative to
the SAM3X AD channel number — `A0` on the silkscreen is **SAM3X AD7**
internally, `A7` is **AD0**, etc.

Each analog pin also has a D-number alias (`A0`=`D54`, `A1`=`D55`, …,
`A11`=`D65`) and most Due pinout reference images tag the same pin
with a third label `ADC<n>` where `<n>` tracks the A-number, *not* the
SAM3X AD channel. So the pinout image says `A2 / ADC2 / D56` for a pin
the SAM3X calls AD5. Keep the three name spaces straight:

  * `A<n>` / `D(54+n)` / `ADC<n>` — what the Due PCB and pinout image
    agree on (n = 0..11).
  * SAM3X AD channel — what the chip datasheet and register layout
    use. 7 minus `n` for pins on the main analog header.
  * `adc[i]` in our wire protocol — indexed by SAM3X AD channel, so
    `adc[i]` is the sample from `A(7-i)`.

| Due label | Alias | SAM3X pin | SAM3X AD | `status.adc[]` | Status |
|---|---|---|---|---|---|
| `A0` | `D54` / `ADC0` | PA16 | AD7 | `adc[7]` | ✅ Active |
| `A1` | `D55` / `ADC1` | PA24 | AD6 | `adc[6]` | ✅ Active |
| `A2` | `D56` / `ADC2` | PA23 | AD5 | `adc[5]` | ✅ Active |
| `A3` | `D57` / `ADC3` | PA22 | AD4 | `adc[4]` | ✅ Active |
| `A4` | `D58` / `ADC4` | PA6  | AD3 | `adc[3]` | ✅ Active |
| `A5` | `D59` / `ADC5` | PA4  | AD2 | `adc[2]` | ✅ Active |
| `A6` | `D60` / `ADC6` | PA3  | AD1 | `adc[1]` | ✅ Active |
| `A7` | `D61` / `ADC7` | PA2  | AD0 | `adc[0]` | ✅ Active |
| `A8` | `D62` / `ADC8` | PB17 | AD10 | — | ⚠️ Reserved (firmware reads 8 channels only) |
| `A9` | `D63` / `ADC9` | PB18 | AD11 | — | ⚠️ Reserved |
| `A10` | `D64` / `ADC10` | PB19 | AD12 | — | ⚠️ Reserved |
| `A11` | `D65` / `ADC11` | PB20 | AD13 | — | ⚠️ Reserved |

Sampling is **SOF-synchronous** (125 µs per cycle at HS, one conversion
cycle per USB microframe). See `main.c::user_callback_sof_action`.

---

## 3. PWM

### 3.1 Currently active (4 channels)

Hardware: SAM3X PWM peripheral, PWMH4–PWMH7. CPOL=1, carrier 1 kHz
for manual streaming, user-selectable frequency for `fngen` mode.
Duty = 0 → pin LOW, duty = max → pin HIGH (standard convention).

| Wire field | Due label | Silkscreen label | SAM3X pin | Peripheral | Status |
|---|---|---|---|---|---|
| `pwm[0]` (streaming) / fngen `pwm0` | `D9` | `PWM9` | PC21 | PWMH4 | ✅ Active |
| `pwm[1]` (streaming) / fngen `pwm1` | `D8` | `PWM8` | PC22 | PWMH5 | ✅ Active |
| fngen `pwm2` | `D7` | `PWM7` | PC23 | PWMH6 | ✅ Active |
| fngen `pwm3` | `D6` | `PWM6` | PC24 | PWMH7 | ✅ Active |

> The 64-byte wire frame carries two PWM slots (`pwm0`, `pwm1`) for
> streaming manual duty. `pwm2` and `pwm3` are generator-only —
> drive them via `POST /api/fngen/play_builtin/pwm{2,3}`.
>
> ⚠️ **Naming gotcha.** The Arduino Due silkscreens each PWM pin with
> the D-pin number (`PWM2`..`PWM13` for `D2`..`D13`), NOT with a
> sequential 0..3 index. Firmware `pwm[0]` therefore lands on the pin
> the board calls `PWM9`, not `PWM2`. When someone says "PWM0" on
> hardware they usually mean silkscreen `PWM2` on `D2` — map back to
> our index space before wiring.

### 3.2 Planned extension to 8 channels (D2–D9)

Target: **8 PWMs** (`pwm0`..`pwm7`) covering pins `D2`..`D9`. The
first 4 use the SAM3X PWM peripheral (already active, §3.1); the
new 4 use the **TC (Timer Counter) peripheral** via peripheral-B
pin muxing. TC channels need their own clock setup and the
per-channel duty updates go through `TC_RA`/`TC_RB` rather than
`PWM_CDTYUPD`, so `waveform.c::pwm_hw_play` will dispatch on the
channel index.

| Planned index | Due label | SAM3X pin | Peripheral | Status |
|---|---|---|---|---|
| `pwm4` | `D5` | PC25 | TC2 ch0 TIOA (TC6) | ⏳ Planned |
| `pwm5` | `D4` | PC26 | TC2 ch0 TIOB (TC6) | ⏳ Planned |
| `pwm6` | `D3` | PC28 | TC2 ch1 TIOA (TC7) | ⏳ Planned |
| `pwm7` | `D2` | PB25 | TC0 ch0 TIOA (TC0) | ⏳ Planned |

`D10`..`D12` are left free for future expansion (additional PWMs or
alternate uses). `D13` (`PB27`) is reserved as the firmware
heartbeat LED — see §5.

Implementation outline:
1. Extend `WAVE_NUM_PWM_ACTIVE` from 4 to 8.
2. Add a `s_pwm[idx].peripheral_kind` tag (`PWM_HW` or `TC_HW`) and
   per-kind `_play`/`_stop` helpers. TC setup picks a prescaler to
   fit the requested frequency, writes `TC_CMR` + `TC_RC` (period)
   + `TC_RA` (duty), enables the channel via `TC_CCR`.
3. Mux the pins: PC25/26 release to peripheral B; PC28 and PB25
   similarly.
4. Wire the `fngen play_builtin/pwm{4..7}` endpoints — no wire-
   protocol change needed because the on-chip function generator
   already reaches all channels by index through the vendor SETUP
   plane.

---

## 4. Digital I/O

Firmware convention (pre-2.0 and kept): **odd D-number = output,
even D-number = input**. Wire as loopback pairs D(2k)↔D(2k+1) for
self-test. All DIN pins have internal pull-ups enabled.

### 4.1 Digital outputs (DOUT, 16 channels)

| Wire bit | Due label | SAM3X pin |
|---|---|---|
| `digital_out` bit 0 | `D23` | PA14 |
| bit 1 | `D25` | PD0 |
| bit 2 | `D27` | PD2 |
| bit 3 | `D29` | PD6 |
| bit 4 | `D31` | PA7 |
| bit 5 | `D33` | PC1 |
| bit 6 | `D35` | PC3 |
| bit 7 | `D37` | PC5 |
| bit 8 | `D39` | PC7 |
| bit 9 | `D41` | PC9 |
| bit 10 | `D43` | PA20 |
| bit 11 | `D45` | PC18 |
| bit 12 | `D47` | PC16 |
| bit 13 | `D49` | PC14 |
| bit 14 | `D51` | PC12 |
| bit 15 | `D53` | PB14 |

### 4.2 Digital inputs (DIN, 16 channels)

| Wire bit | Due label | SAM3X pin |
|---|---|---|
| `digital_in` bit 0 | `D22` | PB26 |
| bit 1 | `D24` | PA15 |
| bit 2 | `D26` | PD1 |
| bit 3 | `D28` | PD3 |
| bit 4 | `D30` | PD9 |
| bit 5 | `D32` | PD10 |
| bit 6 | `D34` | PC2 |
| bit 7 | `D36` | PC4 |
| bit 8 | `D38` | PC6 |
| bit 9 | `D40` | PC8 |
| bit 10 | `D42` | PA19 |
| bit 11 | `D44` | PC19 |
| bit 12 | `D46` | PC17 |
| bit 13 | `D48` | PC15 |
| bit 14 | `D50` | PC13 |
| bit 15 | `D52` | PB21 |

---

## 5. Dedicated / status pins

| Due label | SAM3X pin | Role | Notes |
|---|---|---|---|
| `D13` | PB27 | **Firmware heartbeat LED** | On-board "L" LED. Firmware blinks 1 Hz when iso transport is healthy, double-blink on auto-reconnect, solid on hang. See `main.c::SysTick_Handler` for the state machine. NOT user-controllable — do not wire anything to it. |

---

## 6. Peripheral buses (reserved, available to the firmware for future use)

### 6.1 UART

| Bus | TX Due label | RX Due label | SAM3X pins | Status |
|---|---|---|---|---|
| Serial (UART0, programming port) | `D1` | `D0` | PA9 / PA8 | 🔒 **Don't touch** — the ATmega16U2 USB-UART bridge and the `flash_firmware.sh` 1200-baud trick both live on this bus. Using D0/D1 as GPIO breaks the flash flow. |
| Serial1 (USART0) | `D18` | `D19` | PA11 / PA10 | ✅ Free — can be repurposed as extra DIN/DOUT or as external UART |
| Serial2 (USART1) | `D16` | `D17` | PA13 / PA12 | ✅ Free |
| Serial3 (USART3) | `D14` | `D15` | PD4 / PD5 | ✅ Free |

### 6.2 I²C

| Bus | Due label | SAM3X pin | Status |
|---|---|---|---|
| Wire (I²C0, TWI0) | `D20` (SDA) | PB12 | ⚠️ Reserved — if you add I²C sensors (BME280 environmental, SI7021 humidity, etc.), wire them here. |
| | `D21` (SCL) | PB13 | |
| Wire1 (I²C1, TWI1) | `D70` = `SDA1` | PA17 | ⚠️ Reserved — on the analog header after `A11` |
| | `D71` = `SCL1` | PA18 | |

### 6.3 SPI

| Signal | Due label | SAM3X pin | Notes |
|---|---|---|---|
| MISO | ICSP header pin 1 (also `D74`) | PA25 | |
| MOSI | ICSP header pin 4 (also `D75`) | PA26 | |
| SCK | ICSP header pin 3 (also `D76`) | PA27 | |
| SS0 (CS canonical) | `D10` | PA28 | conflicts with a potential `pwm8` in future extension |
| SS1 | `D4` | PC26 | conflicts with a potential `pwm6` |
| SS2 | `D52` | PB21 | **conflicts with `din[15]`** — if we add SPI slaves, `din[15]` moves |

The ICSP header MISO/MOSI/SCK lines are shared with the SWD/JTAG
debug interface. Do not use them as GPIO.

### 6.4 CAN bus (your DB9 on the back panel)

SAM3X has two independent CAN controllers; **only CAN0 is easy to
wire on the Arduino Due**, CAN1 conflicts with existing signals.

| Bus | Signal | SAM3X pin | Accessible on Due? | Status |
|---|---|---|---|---|
| CAN0 | CANRX0 | PA1 | Brought out to pad `CANRX` (near `D0`/`D1`, dedicated CAN holes on some Due revs; **not** on the main digital header) | ✅ Free — route to rear DB9 |
| CAN0 | CANTX0 | PA0 | Brought out to pad `CANTX` | ✅ Free |
| CAN1 | CANRX1 | PB14 | **same pin as `dout[15]`** — conflict | 🔒 Blocked until `dout[15]` is remapped |
| CAN1 | CANTX1 | PB15 | **same pin as `DAC0`** — conflict | 🔒 Blocked |

For your use case (single CAN bus → rear DB9 connector): **use CAN0
(PA0 / PA1)**, wire directly to a CAN transceiver chip (MCP2551,
TJA1050, etc.), then to the DB9. The Arduino Due doesn't have a CAN
transceiver on-board — you need to add one.

---

## 7. Do-not-touch list (hardware-critical)

| Pin | Why |
|---|---|
| `D0` / `D1` (UART0) | Programming-port USB-serial bridge. Needed for the flash flow. |
| `AREF` | Analog reference input. Leaving it floating lets the internal 3.3 V reference dominate; driving it as GPIO shorts the reference. |
| ICSP header (MISO/MOSI/SCK/RESET) | JTAG/SWD debug fallback + factory programming; also SPI. |
| `D13` (`PB27`) | On-board LED (firmware-managed status indicator). Wiring a load to it fights the LED. |
| `RESET` button | Only hardware path to recover from a firmware hang. Keep accessible. |
| `ERASE` button | Factory-erase fallback when the `flash_firmware.sh` 1200-baud trick fails. |

---

## 8. Available-but-unused pins (for future expansion)

After accounting for all of the above, these SAM3X pins are not
claimed by any firmware role and are free for future features:

- `D14`–`D19` (USART1/2/3 pairs — 6 pins) — if you don't need
  extra serial ports, reusable as DOUT/DIN or further PWM candidates.
- `D68`, `D69` (CAN pads, after CAN0 routing) — technically reusable.
- Analog `A8`–`A11` (PB17–PB20) — reserved for future 12-channel ADC
  support.
- The "TWI0" (I²C0) and "TWI1" (I²C1) pins if you don't plan to
  use I²C.

---

## 9. Quick sanity-check procedure

After any firmware change that touches pin assignments:

1. Flash the new firmware (`./scripts/flash_firmware.sh`).
2. Run the DOUT/DIN loopback script (`/tmp/_gpio_loopback.py` or
   the selftest) — all 16/16 pairs must pass.
3. Run the DAC→ADC loopback: set `dac0=4095`, `dac1=0` via
   `/api/command` and confirm `adc[7]` ~3400 and `adc[6]` ~680.
4. Run the PWM→ADC loopback on `pwm0`..`pwm3` via
   `/api/fngen/play_builtin/pwmN` with `shape=square`.
5. `curl /api/health` must return 200 with `iso_in_rate_hz` ≈ 8000.
