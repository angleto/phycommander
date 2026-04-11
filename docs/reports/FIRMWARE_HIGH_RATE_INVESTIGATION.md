# Firmware High-Rate Stability Investigation

**Date:** 2026-04-11
**Status:** Investigation in progress, kHz-rate target NOT YET achieved.
**Target:** USB CDC communication at maximum supported rate (5 kHz via CDC serial transport, or up to 10 kHz via direct USB bulk), sustained 24/7 with <0.01% error rate.

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

## The three paths forward

**Ranked in order of likelihood to actually deliver 5 kHz 24/7.**

### Path A — Fix the CDC path (cheapest, lowest ceiling)

1. Re-apply the dc42 non-blocking `udi_cdc_write_buf` patch.
2. Keep our byte-by-byte `main.c` framing (which already avoids Bug 1).
3. Remove any `WFI` from the idle path.
4. Re-test at 1 kHz, 2 kHz, 5 kHz for **5+ minutes each** with `usbmon` capture running so we can see the first sign of misbehaviour instead of guessing from `seq_num` drift.
5. Measure and document the actual achievable rate.

Estimated ceiling: **~2-3 kHz sustained**, realistically. USB CDC ACM has protocol overhead (line state notifications on the interrupt endpoint) and Linux `cdc_acm` is not tuned for extremely tight request/response. Duet3D runs well into the multi-kHz range over CDC, so 5 kHz may be reachable with the right patches.

### Path B — Direct USB bulk class (highest ceiling, proper fix)

The README already calls this out as the intended high-performance path:

> Transport & Communication: **Modular transport system** (USB Bulk / USB CDC Serial)
> **Direct USB bulk transfer** (10 kHz, 120 µs latency)
> Status: ⏳ Direct USB bulk endpoint support in firmware

`physerver` already has a `usb` transport (`physerver/src/transport/usb.rs`) that uses `rusb` to do direct BULK IN/OUT to VID `0x2341` PID `0x003e`. The firmware currently doesn't implement this — it only exposes CDC. To enable it:

1. Rewrite `conf_usb.h` to describe a **Vendor-specific class interface** with just two bulk endpoints (BULK IN `0x82`, BULK OUT `0x02`), no CDC descriptors at all.
2. Replace `udi_cdc.c` dependency with a minimal descriptor + the raw `udd_ep_run()` API from `udd.h`. Arm an OUT transfer, handle `udi_cdc_data_received`-style callback, echo back via BULK IN. This is ~200 lines of C.
3. Change VID/PID to match what `transport::usb::UsbTransport::find_device()` looks for (`0x2341:0x003e`) — or change `find_device()` to accept our `0x03eb:0x2404`.
4. Add a `udev` rule `SUBSYSTEM=="usb", ATTRS{idVendor}=="...", ATTRS{idProduct}=="...", MODE="0666"` so `rusb` can claim the interface without root.

This **completely bypasses** `cdc_acm`, `tcdrain`, `serialport-rs`, CDC class overhead, line coding negotiation, and every other source of the bugs above. It's what the README promises and what the existing `physerver` transport was built for.

Estimated ceiling: **10 kHz sustained** (the value in the README).

Estimated effort: 1-2 focused days.

### Path C — Use Arduino core USB (escape hatch)

If both ASF paths keep hitting walls, the nuclear option is to drop ASF UDI_CDC entirely and adopt the Arduino core USB stack from `github.com/arduino/ArduinoCore-sam`. This is the same CDC code that runs on every Arduino Due today, is maintained, and is tested by thousands of users. It would require porting our digital-I/O + ADC + protocol code on top of it, and re-organising the project so Arduino IDE or `arduino-cli` can build it.

Estimated effort: 2-3 days, high chance of success but invasive.

## Checklist for the next session

Before attacking the high-rate problem again, these preconditions should be met so iteration doesn't get stuck on Bugs 4 & 5:

- [ ] **Physical access to the Arduino Due is available**, so that USB can be unplugged/replugged if the SAM3X enters the "zombie" state that AIRCR reset cannot clear.
- [ ] **A full host reboot of `192.168.0.22` has just happened**, so `cdc_acm` is in a fresh state. First iteration of a test run is almost always clean; the fifth iteration is almost never clean.
- [ ] `usbmon` is enabled and a capture script is ready: `cat /sys/kernel/debug/usb/usbmon/2u > /tmp/usbmon.log &` before starting `physerver`, so we can grep NAK/STALL/short-packet/URB-unlink events on failure.
- [ ] `physerver` is only started ONCE in each test (no `restart` loops while hunting bugs — use foreground runs).
- [ ] The test target is explicit and numerical: *"5 kHz sustained for 5+ minutes with 0 errors"*, not *"runs OK for a bit"*. Anything less is not production.
- [ ] The session starts with a clear choice of Path A vs Path B vs Path C, **not** an exploratory iteration.

## Recommendation

Take **Path B**. It is the architecture the project README already promises, it completely sidesteps every CDC bug catalogued here, and it is what every other hobbyist/industrial SAM3X project that needs >1 kHz already does.

Path A should only be attempted if Path B is somehow blocked and there is a hard requirement to ship over `cdc_acm`.

## References

- Duet3D CoreNG `udi_cdc.c` (dc42 non-blocking write_buf patch): <https://github.com/Duet3D/CoreNG/blob/master/asf/common/services/usb/class/cdc/device/udi_cdc.c>
- ArduinoCore-sam: <https://github.com/arduino/ArduinoCore-sam>
- Tewarid, *"Measuring data rate of ASF USB Device CDC example"* (documents the `read_buf` busy-wait bug): <https://tewarid.github.io/2014/03/16/measuring-data-rate-of-asf-usb-device-cdc-example.html>
- Atmel AT09332, *"USB Device Interface (UDI) for Communication Class Device (CDC) Application Note"*: <https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-42337-USB-Device-Interface-UDI-for-Communication-Class-Device-CDC_ApplicationNote_AT09332.pdf>
- SAM3X/A datasheet (UOTGHS, RSTC, PMC sections): <https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-11057-32-bit-Cortex-M3-Microcontroller-SAM3X-SAM3A_Datasheet.pdf>
