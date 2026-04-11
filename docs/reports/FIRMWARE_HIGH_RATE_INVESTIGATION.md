# Firmware High-Rate Stability Investigation

**Date:** 2026-04-11
**Status:** Investigation done, architecture for the fix decided. Implementation deferred to the next focused session.
**Target:** Sustained 24/7 operation at 5 kHz (minimum acceptable) up to 10 kHz (ceiling) with ≤0.01 % error rate.
**Chosen path:** Replace ASF UDI_CDC with a **custom USB Vendor Class bulk** firmware layer written in bare-metal C, on top of ASF's UDD + CMSIS + peripheral drivers, hitting the host through the `rusb`-based transport that already exists in `physerver/src/transport/usb.rs`. No Arduino SDK, no CDC, no `cdc_acm`, no `tcdrain`. See **"Chosen path — Vendor Class bulk (L1)"** below.

## Current state of the tree

The following files are changed vs. the stock ATSAM3X8E_FW checkpoint:

| File | Status | Reason |
|---|---|---|
| `ATSAM3X8E_FW/ATSAM3X8E_FW/Makefile` | NEW | GCC ARM build recipe extracted from `ATSAM3X8E_FW.cproj` (Atmel Studio) so the firmware can be built on Linux/macOS with `gcc-arm-none-eabi`. |
| `ATSAM3X8E_FW/ATSAM3X8E_FW/src/ASF/sam/utils/compiler.h` | MODIFIED | One-line fix: `optimize(s)` → `optimize("s")`. Upstream ASF has an un-quoted string identifier that modern GCC (13.x) rejects; the original project only built because Atmel Studio used an old GCC that silently tolerated it. |
| `ATSAM3X8E_FW/ATSAM3X8E_FW/src/main.c` | MODIFIED | Full rewrite of the user application layer. Adds the PhyCMD-64 protocol (header `0xAA55` / `0x55AA`, CRC-16-CCITT, sequence number) that the Rust `physerver` expects. The previous `main.c` was a legacy 64-byte loopback with no framing, which could not talk to the current `physerver`. |

The Duet3D/dc42 non-blocking-write patch to `ASF/.../udi_cdc.c` was tried during the investigation and then **rolled back** — see "Patches tried & reverted" below.

### What works today

- Firmware builds on Linux with `gcc-arm-none-eabi` 13.2 via the new `Makefile` (13.1 KB flash, 11 KB RAM).
- `bossac` flashes it correctly over `/dev/arduino_due_prog` (i.e. the ATmega16U2 Programming Port) after a 1200-baud SAM-BA trigger.
- The chip enumerates over USB HS (480 Mbps) as `03eb:2404 Ephemeralbit PhysicalCommander`, and presents a `cdc_acm` character device that `udev` aliases to `/dev/phycommander` via `99-phycommander.rules`.
- `physerver` on the Linux target (Ubuntu 24.04 PREEMPT-RT on `192.168.0.22`) opens the port, runs its command-response loop, and the **full PhyCMD-64 protocol round-trip is verified bit-for-bit**: `physerver`'s encode_command bytes match what the firmware decodes, the 0x1021 CRC matches on both sides, the sequence number is echoed correctly, and `/api/status` shows valid data.
- Sustained operation at **`update_rate = 10` Hz** was observed stable for >2 minutes (1200+ frames) with 0 errors in a clean test run.

### What does NOT work

At any `update_rate ≥ ~100 Hz` the firmware eventually stops responding. The failure pattern is:

1. First N successful exchanges (observed N values: 55, 121, 133, 250 — not deterministic).
2. The firmware stops acknowledging USB OUT transactions. `physerver` hangs in `tcdrain(2)` (the `.flush()` call after each `write_all`) because `cdc_acm` is still waiting for the kernel TX buffer to drain to the device.
3. After the kernel's internal timeout fires, `physerver` logs `"Communication error: Failed to read status from serial port"` and the `exchange()` call returns `Err`.
4. On the **next** `physerver` start attempt, re-opening the port goes slow: the host cdc-acm issues a `SET_CONTROL_LINE_STATE` CDC class request over control endpoint 0 (bmRequestType `0x21`, bRequest `0x22`, wValue `0x0003` for DTR+RTS high) and the SAM3X is **completely silent on the bus for ~5.5 seconds** — no ACK, no NAK, nothing — until the host cancels the URB with `ENOENT`.

Both symptoms point to the same root cause: the SAM3X USB stack ends up in a state where the main loop is either stuck or not servicing the UDI_CDC state machine often enough for `udd_ep_run()` to keep the RX/TX endpoints armed, and control transfers start missing their 5 s timeout window.

## Bugs identified (with evidence)

### Bug 1 — `udi_cdc_read_buf` busy-waits across buffer boundaries

`src/ASF/common/services/usb/class/cdc/device/udi_cdc.c` (ASF 3.x):

```c
iram_size_t udi_cdc_multi_read_buf(uint8_t port, void* buf, iram_size_t size)
{
    ...
udi_cdc_read_buf_loop_wait:
    flags = cpu_irq_save();
    pos = udi_cdc_rx_pos[port];
    buf_sel = udi_cdc_rx_buf_sel[port];
    cpu_irq_restore(flags);
    while (pos >= udi_cdc_rx_buf_nb[port][buf_sel]) {
        if (!udi_cdc_data_running) {
            return size;
        }
        goto udi_cdc_read_buf_loop_wait;   /* <-- spins forever */
    }
    ...
    if (size) {
        goto udi_cdc_read_buf_loop_wait;
    }
```

If the caller asks for more bytes than the **currently selected** RX buffer holds, the function spins waiting for those bytes to appear — but the double-buffer swap only happens inside `udi_cdc_rx_start()`, which is only called on a successful partial read. If the other buffer is full and the current one is empty, the spin can deadlock for extended periods.

**Confirmed** by an independent third-party writeup: see *"Measuring data rate of ASF USB Device CDC example"* (tewarid, 2014): *"a slight design flaw. If data is not a multiple of 10 bytes, the code will be stuck in the call to `udi_cdc_read_buf` towards the end of the data."*

**Workaround in `main.c`:** never call `read_buf()` with a size larger than `udi_cdc_get_nb_received_data()`. This keeps the call off the wait path entirely. Our current byte-by-byte framing loop already does this.

### Bug 2 — `udi_cdc_write_buf` busy-waits when the TX FIFO is full

Same file, `udi_cdc_multi_write_buf()`:

```c
udi_cdc_write_buf_loop_wait:
    if (!udi_cdc_multi_is_tx_ready(port)) {
        if (!udi_cdc_data_running) {
            return size;
        }
        goto udi_cdc_write_buf_loop_wait;   /* <-- spins forever */
    }
```

Under high rate, the device-side TX buffer (double-buffered, `UDI_CDC_TX_BUFFERS` bytes each = 512 for HS) fills faster than `cdc_acm` on the host pulls it over the BULK IN endpoint. The spin blocks the main loop — and therefore also blocks `udi_cdc_rx_start()` from ever being called again to re-arm the BULK OUT endpoint — so the host's next write is NAKed forever. This produces exactly the `tcdrain()` hang we see from `physerver`.

**Known fix (Duet3D CoreNG, commit by dc42):**

```c
if (!udi_cdc_multi_is_tx_ready(port)) {
#if 1   // dc42 change to make this function non-blocking
    return size;
#else
    if (!udi_cdc_data_running) {
        return size;
    }
    goto udi_cdc_write_buf_loop_wait;
#endif
}
```

Source: <https://github.com/Duet3D/CoreNG/blob/master/asf/common/services/usb/class/cdc/device/udi_cdc.c>

**Status in this tree:** patch was applied during investigation and **rolled back** to bisect the boot crash that appeared in the same window. To be re-applied in the next session.

### Bug 3 — `__WFI()` in the idle path breaks control-endpoint handling

`main.c` originally had `__asm__ volatile("wfi")` in the loop that waits for RX data. On SAM3X, when the main loop sleeps via WFI while the CDC interface is idle, the USB peripheral can stop servicing control transfers reliably — `usbmon` capture showed `SET_CONTROL_LINE_STATE` going unanswered for 5.5 s (until the host cancelled the URB with `ENOENT`). Removing `WFI` in favour of a tight spin made control transfers complete in <1 ms in the same test, but that alone did not fix the high-rate bulk transfer hang (Bug 2 dominated).

**Takeaway:** do not use `WFI` with this firmware/ASF combination until the UDI_CDC internal state machine is properly serviced from main context.

### Bug 4 — `cdc_acm` host driver state corrupts after rapid reconnects

After ~4-5 cycles of `physerver start/stop` in quick succession, the host `cdc_acm` driver enters a state where every subsequent open takes >5 s and no BULK transfers succeed, even if the firmware is perfectly healthy. `rmmod cdc_acm && modprobe cdc_acm` clears it; a full reboot also clears it. This is a host-side issue, not a firmware issue, but it complicates iterating on the firmware because every broken test leaves the environment in a worse state for the next test.

**Not a production concern**: in real 24/7 operation `physerver` starts once and stays open. It only bites during development iteration.

### Bug 5 — SAM3X USB peripheral "zombie" state after repeated flash cycles

Several times during this session the SAM3X entered a state where:
- The Programming Port (`2341:003d`, ATmega16U2 bridge) is still visible on `lsusb`.
- The SAM3X Native USB port (`03eb:2404` or the `6124` SAM-BA) is **completely gone** from the bus.
- The `1200 baud` trick on the Programming Port no longer re-triggers SAM-BA.
- A `bossac` via the Programming Port still works, but the flashed firmware does not come up.
- A **full system reboot** of the host (or presumably a physical USB unplug of the Arduino Due) is the only reliable recovery.

This looks like a UOTGHS/PHY state that `AIRCR.SYSRESETREQ` does not actually clear, even though the Cortex-M3 datasheet says it should propagate through RSTC to all peripherals. Possibly a known Atmel erratum; needs investigation.

**Not a production concern either**: again, this only bites during development iteration where the firmware is being re-flashed many times. A shipped firmware runs once and stays put.

## Patches tried & reverted

| Patch | Where | Outcome |
|---|---|---|
| `udi_cdc_write_buf` → non-blocking (dc42 pattern) | `ASF/.../udi_cdc.c` | Builds clean. Boot OK on first attempt, but in the same session revealed state-corruption issue (Bug 5) and I could not finish the test. **Not merged in this commit** but ready to re-apply — the diff is documented above. |
| `udi_cdc_read_buf` → non-blocking | `ASF/.../udi_cdc.c` | Same as above, reverted. Our byte-by-byte `main.c` loop makes this optional, so not high priority. |
| Remove `__WFI()` from idle spin | `main.c` | Fixed the 5.5 s `SET_CONTROL_LINE_STATE` timeout, made control transfers respond <1 ms. Did not fix the high-rate TX stall. Currently re-applied in this tree. |
| Disable WDT via `WDT->WDT_MR = WDT_MR_WDDIS` | `main.c` | Kept. SAM3X `WDT_MR` is write-once, so re-arming it at a smaller timeout after boot fires immediately — this single write is the safest thing to do. |

## Chosen path — Vendor Class bulk (L1)

The fix is to **throw away UDI_CDC and talk directly to UDD with two bulk endpoints in a Vendor-specific class**. No `/dev/ttyACM*`, no kernel `cdc_acm` driver, no `tcdrain`, no line coding, no DTR/RTS dance. The host side is already built: `physerver/src/transport/usb.rs` uses `rusb` (libusb) to do BULK IN/OUT directly on a target VID/PID. It just needs a firmware that answers on those endpoints with our PhyCMD-64 frames.

### How much of ASF do we keep?

Picking the right level of "bare-metal" matters a lot. Going too minimal wastes weeks rewriting code that already works; staying too high leaves the buggy layer in place. The right cut is **L1**: keep everything in ASF *except* `UDI_CDC`.

| Level | What's kept from ASF | What we write ourselves | Effort | Stability |
|---|---|---|---|---|
| L0 — current state (CDC) | Everything: UDI_CDC, UDC, UDD, UOTGHS driver, peripheral drivers, startup | Protocol loop + framing | — | ≤ 2 kHz, fragile (5 bugs) |
| **L1 — chosen** | **UDC (class registrar), UDD (USB device driver), UOTGHS driver, `sysclk`, `pmc`, `pio`, `adc`, `dacc`, startup, linker script** | **Vendor Class descriptors + 2 bulk endpoint handlers via `udd_ep_run()` + protocol loop** | **1-2 gg** | **5-10 kHz, robust** |
| L2 — UDC/UDD bypass | `sysclk`, `pmc`, `pio`, `adc`, `dacc`, startup, linker script, CMSIS headers | Full USB device driver for UOTGHS (endpoint config, SETUP handler, bulk DMA) + descriptors + everything else | 1-2 weeks | Same ceiling as L1, higher risk |
| L3 — full custom | Only CMSIS register definitions (`sam3x8e.h`) | Clock init, USB stack, all peripheral drivers, startup, linker script | 3-4 weeks | Maximum control, reinvents the wheel |

**Why L1 and not L2/L3.** The bugs catalogued above (1-5) all live inside `UDI_CDC`. The layer below it — `UDC` and especially `UDD` — is a thin wrapper over the UOTGHS peripheral. `udd_ep_run()` arms a bulk transfer with the chip's hardware FIFO + DMA and calls a user callback when it completes; that's it. No spin loops, no class protocol, no state machine we can trip on. It's ~1000 lines of driver that's been in production on thousands of SAM-based products for a decade. Rewriting it from scratch (L2/L3) would produce code with the **same reliability as the current UDD** because we'd be rediscovering the same edge cases Atmel already debugged.

The same reasoning applies to `sysclk`, `pmc`, `pio`, `adc`, `dacc`: these are leaf drivers that program peripheral registers. They do one job and have no bug pattern we've observed. Keeping them costs nothing and removes weeks of re-implementation.

### What the new firmware looks like at the code level

Concretely, L1 means these files change (all in `ATSAM3X8E_FW/ATSAM3X8E_FW/src/`):

| File | Change |
|---|---|
| `config/conf_usb.h` | Remove every `UDI_CDC_*` define. Set `USB_DEVICE_NB_INTERFACE = 1`. Set the class via `UDC_DESC_STORAGE` + our own descriptor table (device + configuration + interface + two endpoint descriptors). `USB_DEVICE_CLASS = 0xFF` (vendor-specific). `USB_DEVICE_VENDOR_ID = 0x2341`, `USB_DEVICE_PRODUCT_ID = 0x003E` to match `physerver/src/transport/usb.rs`. |
| `main.c` | Replace the `udi_cdc_*` loop with a pair of bulk endpoint callbacks plus a tight main loop. Sketch below. |
| `Makefile` | Drop the four `src/ASF/common/services/usb/class/cdc/device/udi_cdc*.c` entries from `SRCS`. Everything else stays. |

The main.c skeleton is ~200 lines, most of which are descriptor tables:

```c
/* -- 1. USB descriptors (static ROM data, ~80 lines) -- */
COMPILER_WORD_ALIGNED
static const uint8_t device_descriptor[] = {
    0x12,                   /* bLength              */
    USB_DT_DEVICE,          /* bDescriptorType      */
    0x00, 0x02,             /* bcdUSB 2.00          */
    0xFF,                   /* bDeviceClass         — vendor-specific */
    0x00,                   /* bDeviceSubClass      */
    0x00,                   /* bDeviceProtocol      */
    64,                     /* bMaxPacketSize0      */
    0x41, 0x23,             /* idVendor 0x2341      */
    0x3E, 0x00,             /* idProduct 0x003E     */
    0x00, 0x01,             /* bcdDevice 1.00       */
    0x01, 0x02, 0x03,       /* iManufacturer, iProduct, iSerial */
    0x01,                   /* bNumConfigurations   */
};

static const uint8_t config_descriptor[] = {
    /* configuration */
    0x09, USB_DT_CONFIGURATION, 0x20, 0x00, 0x01, 0x01, 0x00, 0x80, 0xFA,
    /* interface 0, class=0xFF, 2 endpoints */
    0x09, USB_DT_INTERFACE, 0x00, 0x00, 0x02, 0xFF, 0x00, 0x00, 0x00,
    /* EP 0x82, bulk IN, 64 bytes, interval 0 */
    0x07, USB_DT_ENDPOINT, 0x82, 0x02, 0x40, 0x00, 0x00,
    /* EP 0x02, bulk OUT, 64 bytes, interval 0 */
    0x07, USB_DT_ENDPOINT, 0x02, 0x02, 0x40, 0x00, 0x00,
};

/* string descriptors (vendor, product, serial) — same as current */

/* -- 2. Class hooks registered with UDC, ~30 lines -- */
static bool my_class_enable(void) {
    /* Host completed SET_CONFIGURATION. Arm the first bulk OUT. */
    arm_out_transfer();
    return true;
}
static void my_class_disable(void) { /* nothing */ }
static bool my_class_setup(void)   { /* no vendor-specific requests */ return false; }

UDC_DESC_STORAGE udi_api_t my_class_api = {
    .enable    = my_class_enable,
    .disable   = my_class_disable,
    .setup     = my_class_setup,
    .getsetting= NULL,
};

/* UDC configuration: one interface, our class API */
UDC_DESC_STORAGE udc_config_speed_t udc_config_fshs[1] = { {
    .desc = (usb_conf_desc_t *)config_descriptor,
    .udi_apis = (udi_api_t *[]) { &my_class_api },
} };

UDC_DESC_STORAGE udc_config_t udc_config = {
    .confdev_lsfs = (usb_dev_desc_t *)device_descriptor,
    .conf_fs_nb   = 1, .conf_fs = udc_config_fshs,
    .conf_hs_nb   = 1, .conf_hs = udc_config_fshs,
    .conf_bos     = NULL,
};

/* -- 3. Bulk endpoint DMA handlers, ~40 lines -- */
static volatile bool g_cmd_ready = false;
static COMPILER_WORD_ALIGNED uint8_t rx_buf[64];
static COMPILER_WORD_ALIGNED uint8_t tx_buf[64];

static void bulk_out_cb(udd_ep_status_t status, iram_size_t n, udd_ep_id_t ep) {
    (void)ep;
    if (status == UDD_EP_TRANSFER_OK && n == 64) {
        memcpy(s_in, rx_buf, 64);   /* s_in is the packed command_msg_t* */
        g_cmd_ready = true;
    }
    /* Immediately re-arm for the next frame.  No spin, no flow control,
     * the host is gated by the bulk IN we'll send right after. */
    udd_ep_run(UDI_EP_OUT, false, rx_buf, 64, bulk_out_cb);
}

static void bulk_in_cb(udd_ep_status_t status, iram_size_t n, udd_ep_id_t ep) {
    (void)status; (void)n; (void)ep;
    /* Transfer complete, nothing to do here. */
}

/* -- 4. Main loop, ~15 lines -- */
int main(void) {
    sysclk_init();
    irq_initialize_vectors();
    cpu_irq_enable();
    board_init();
    adc_setup();   /* PDC free-running, its own rate — no longer bound to the USB loop */
    dac_setup();
    init_digital_io();
    udc_start();

    for (;;) {
        if (!g_cmd_ready) continue;
        g_cmd_ready = false;

        process_command();                /* validate CRC, apply outputs */
        build_status_into(tx_buf);        /* compute CRC, fill 64 B */
        udd_ep_run(UDI_EP_IN, false, tx_buf, 64, bulk_in_cb);
    }
}
```

The entire hot path is: hardware DMA fills `rx_buf`, ISR sets `g_cmd_ready` and re-arms OUT, main loop picks it up, processes, fires a single `udd_ep_run()` to send `tx_buf`, loop. **Zero spin loops. Zero flow control in software. Zero dependency on ASF's CDC state machine.**

ADC can now run continuously at its own rate via PDC (free-running, ~70 kHz buffer fills on 8 channels) and the main loop just reads the latest snapshot from `g_adc_buf` when it builds the status frame. DAC writes are applied instantly when a valid command arrives. This is finally what the README describes as *"USB Bulk (10 kHz) — direct libusb — 120 µs latency"*.

### Paths not taken (for the record)

Two other approaches were considered and rejected:

- **Fix CDC in place** (re-apply Duet3D's `write_buf` non-blocking patch + misc ASF patches). Ceiling stops at ~2-3 kHz realistically; still depends on `cdc_acm` kernel driver which has its own corruption issues (Bug 4). Not worth the patch maintenance burden when L1 is the proper architecture anyway.
- **Port to Arduino core (`ArduinoCore-sam`)**. Would drag in Arduino.h, `setup()`/`loop()`, and re-organise the tree around `arduino-cli`. Contrary to the project stance of writing plain C on bare-metal, and still uses CDC underneath. Only worth considering as a last resort.

## Implementation plan for the next session

Ordered so that each step is independently testable — if the chip doesn't enumerate after step 3, you know the descriptors are wrong, not the main loop.

### Pre-conditions (environment)

- [ ] **Physical access to the Arduino Due.** Power-cycling via an accessible USB port is required if the SAM3X enters the "zombie" state (Bug 5). A switched USB hub works fine.
- [ ] **Full host reboot of `192.168.0.22`** immediately before starting, so kernel USB state is clean.
- [ ] **`usbmon` capture ready**: `sudo cat /sys/kernel/debug/usb/usbmon/2u > /tmp/usbmon.log &` *before* the first `physerver` run. After each failure, inspect the capture for NAK, STALL, short packet, URB unlink (`ENOENT`).
- [ ] `physerver` started **once, in foreground**, while tuning. `systemctl` restart loops mask symptoms behind cdc_acm corruption.

### Code work (firmware)

- [ ] **Step 1 — descriptor-only build.** Rewrite `src/config/conf_usb.h`: remove all `UDI_CDC_*` defines, declare Vendor Class interface with 2 bulk endpoints. Drop the `udi_cdc.c`, `udi_cdc_desc.c` entries from `Makefile` `SRCS`. Write a stub `main.c` that just calls `udc_start()` and loops. Build, flash, confirm device enumerates with the expected VID/PID (`0x2341:0x003E`), expected device/config/interface/endpoint descriptors (check with `lsusb -v`). No data transfer yet.
- [ ] **Step 2 — bulk echo.** Add `bulk_out_cb` / `bulk_in_cb` handlers that raw-echo (OUT payload → IN payload). Test with a short Python script using `pyusb` that writes 64 bytes and reads 64 bytes in a tight loop. Measure the achievable rate. Expect **≥5 kHz** at this step; if lower, the problem is in the endpoint configuration (packet size, interval, double buffering).
- [ ] **Step 3 — PhyCMD-64 protocol wiring.** Copy `crc16_ccitt`, the packed command/status structs, and the digital I/O helpers from the current `main.c`. `bulk_out_cb` drops the frame into `s_cmd`, main loop validates + processes + fills `s_stat`, fires `udd_ep_run()` on BULK IN. Same protocol layer we already have today — just a different transport.
- [ ] **Step 4 — re-enable ADC (PDC free-running) and DAC (one-shot writes).** These run at their own rates, decoupled from the USB loop. Main loop just reads the latest snapshot from `g_adc_buf` when building a status frame.
- [ ] **Step 5 — physerver wiring.** `/etc/physerver/config.toml`: change `type = "serial"` to `type = "usb"`. Confirm `physerver/src/transport/usb.rs` looks for the same VID/PID; if not, either update it or update the firmware descriptors to match (prefer aligning firmware to host). Add a udev rule `SUBSYSTEM=="usb", ATTRS{idVendor}=="2341", ATTRS{idProduct}=="003e", MODE="0666"` so physerver can `libusb_open()` as the `angelo` user.

### Gate of "done"

- [ ] `physerver` runs at `update_rate = 5000` for **≥5 minutes** continuously with **0** communication errors logged.
- [ ] Round-trip latency measured via `stats.avg_latency_us` stays below 250 µs.
- [ ] `usbmon` capture shows a clean OUT/IN ping-pong with no NAK, no STALL, no URB unlink.
- [ ] Bonus: push to `update_rate = 10000` and measure where it starts dropping frames. That value is the practical ceiling of the hardware, document it in the README.

## References

- Duet3D CoreNG `udi_cdc.c` (dc42 non-blocking write_buf patch): <https://github.com/Duet3D/CoreNG/blob/master/asf/common/services/usb/class/cdc/device/udi_cdc.c>
- ArduinoCore-sam: <https://github.com/arduino/ArduinoCore-sam>
- Tewarid, *"Measuring data rate of ASF USB Device CDC example"* (documents the `read_buf` busy-wait bug): <https://tewarid.github.io/2014/03/16/measuring-data-rate-of-asf-usb-device-cdc-example.html>
- Atmel AT09332, *"USB Device Interface (UDI) for Communication Class Device (CDC) Application Note"*: <https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-42337-USB-Device-Interface-UDI-for-Communication-Class-Device-CDC_ApplicationNote_AT09332.pdf>
- SAM3X/A datasheet (UOTGHS, RSTC, PMC sections): <https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-11057-32-bit-Cortex-M3-Microcontroller-SAM3X-SAM3A_Datasheet.pdf>
