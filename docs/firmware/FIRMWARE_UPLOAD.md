## Firmware Upload Guide

Complete guide for uploading firmware to Arduino Due.

## TL;DR — scripted flash

For the current PhyCommander codebase, use the repo's one-shot script
(see [Scripted flash (recommended)](#scripted-flash-recommended) for
the full walkthrough):

```bash
# build + flash + soft-reset, all in one
sudo ./scripts/flash_firmware.sh
```

The script encapsulates four non-obvious quirks that tripped us up
in the past; if you're writing your own flash flow, read
[Why the scripted flow exists](#why-the-scripted-flow-exists) first.

If `physerver` is running, the script triggers SAM-BA via the new
`POST /api/firmware/enter-bootloader` endpoint (no J-Link, no
1200-baud / DTR-drop dance). It transparently falls back to the
1200-baud trick on the programming port when physerver isn't
reachable. See [JTAG-free entry path](#jtag-free-entry-path) below.

## Overview

The Arduino Due has **two USB ports**:

1. **Programming Port** (closest to DC jack)
   - Wired to the ATmega16U2 serving as USB-UART bridge to the SAM3X
     first UART, and to the SAM3X's ERASE/RESET pins (pulsed on a
     1200-baud open/close).
   - Used for uploading firmware via BOSSA.
   - `udevadm` reports `ID_MODEL_ID=003d`.

2. **Native USB Port** (closest to reset button)
   - Wired to the SAM3X's own USB peripheral. Appears as
     `ID_MODEL_ID=003e` (phycmd vendor-class firmware) or `003e`
     with different strings depending on the flashed firmware.
   - Used by physerver iso transport at 8 kHz microframe rate.
   - Never used for firmware upload.

## Prerequisites

### Install BOSSA (Upload Tool)

**Linux (Ubuntu/Debian)**:
```bash
sudo apt-get install bossa-cli
```

**macOS**:
```bash
brew install bossa
```

**Windows**:
Download from: https://www.shumatech.com/web/products/bossa

**Verify Installation**:
```bash
bossac --version
# Should show: Basic Open Source SAM-BA Application (BOSSA) Version 1.x
```

### Install Build Tools (if compiling firmware)

**Linux**:
```bash
sudo apt-get install gcc-arm-none-eabi
```

**macOS**:
```bash
brew install arm-none-eabi-gcc
```

## Scripted flash (recommended)

`scripts/flash_firmware.sh` builds and flashes in one invocation:

```bash
# from the repo root, on a host with the Due plugged in:
sudo ./scripts/flash_firmware.sh

# flash an already-built .bin without re-running make:
sudo ./scripts/flash_firmware.sh --bin path/to/phycmd_fw.bin

# build only (no flash, no sudo needed):
./scripts/flash_firmware.sh --build-only

# flash a specific port (auto-detection picks ID_MODEL_ID=003d):
sudo ./scripts/flash_firmware.sh --port /dev/ttyACM0

# force the legacy 1200-baud entry (skip the API call):
sudo ./scripts/flash_firmware.sh --entry 1200baud

# require the in-firmware HTTP entry (fail if physerver is down):
sudo ./scripts/flash_firmware.sh --entry fngen
```

What it does, in order:

1. `make -C ATSAM3X8E_FW/ATSAM3X8E_FW -j$(nproc)` — produces
   `build/phycmd_fw.bin`.
2. **Trigger SAM-BA mode** (two paths, tried in order):
   - **(a) JTAG-free / API path.** If physerver answers `GET /api/health`
     under `$PHYSERVER_URL` (default `http://127.0.0.1:8080`), POST to
     `/api/firmware/enter-bootloader`. The firmware handler clears
     `GPNVM1` (EEFC `CGPB`) and writes
     `RSTC_CR = KEY(0xA5) | PROCRST | PERRST | EXTRST`. The chip resets
     mid-status-stage; ROM SAM-BA takes over. This is the default
     once the firmware on the chip carries the
     `VREQ_FW_ENTER_BOOTLOADER 0x40` handler.
   - **(b) Legacy 1200-baud fallback.** If physerver isn't reachable
     (or `--entry 1200baud` is forced), open the programming port at
     1200 baud and close it. The ATmega16U2 detects the sequence
     and pulses ERASE+RESET on the SAM3X.
3. Stops `physerver.service` if active, so the Due's programming-port
   CDC is released to bossac.
4. Waits for the SAM-BA CDC to re-enumerate as `/dev/ttyACM0`.
5. `bossac -e -w -v -b "$BIN"` — erase + write + verify + set
   GPNVM1 so the SAM3X boots from flash on next reset. **No -R flag**
   (see below).
6. Opens the port at 115200 raw and writes the SAM-BA text command
   `W400E1A00,A500000D#` — this pokes the SAM3X's RSTC_CR register
   with `KEY(0xA5) | PROCRST | PERRST | EXTRST`, issuing a full CPU +
   peripheral + USB reset. The SAM3X boots out of SAM-BA into the
   freshly written application.
7. Waits for the application firmware to re-enumerate, restarts
   `physerver.service`. The stale `/dev/shm/phycmd_state` segment
   is unlinked automatically by `IpcServer::new()` on startup.

### JTAG-free entry path

The 1200-baud trick relies on the ATmega16U2's CDC stack staying
healthy: it has to detect the magic baud rate + DTR drop sequence and
then drive ERASE+RESET on the SAM3X. On long-running benches we have
seen the ATmega16U2 wedge after a CRC drift / EP0 timeout storm, at
which point the only recovery was a J-Link SWD flash.

To remove that single point of failure, the firmware exposes a
vendor SETUP request `VREQ_FW_ENTER_BOOTLOADER (0x40)` on the native
USB port (`physerver`'s normal control plane). The handler does the
same thing the 1200-baud trick does, but driven by the SAM3X itself:

1. `EEFC_FCR ← (FKEY=0x5A << 24) | (FARG=GPNVM1=1 << 8) | FCMD=CGPB(0x0C)`,
   spin on `EEFC_FSR.FRDY` until clear. After this `GPNVM1=0` and the
   chip will boot ROM SAM-BA on the next reset.
2. `RSTC->RSTC_CR ← KEY(0xA5) | PROCRST | PERRST | EXTRST` — full
   chip reset including the USB peripheral.

`physerver` exposes the request as `POST /api/firmware/enter-bootloader`.
Manually:

```bash
curl -X POST http://127.0.0.1:8080/api/firmware/enter-bootloader
# physerver returns 200 OK once the libusb timeout elapses; the
# Due is now in ROM SAM-BA on the programming port.
```

The native USB port disappears (the SAM3X reset wiped UDP); the
ATmega16U2 path stays up because it's a separate chip, and ROM
SAM-BA's USART handler picks up the same `/dev/ttyACM0` that bossac
expects. From here the rest of the flash flow (steps 5–7 above) is
identical to the 1200-baud path.

`EXTRST` matters here: openocd's `reset run` issues only Cortex-M
`SYSRESETREQ`, which leaves the USB peripheral bound to the host's
old enumeration. Writing `RSTC_CR` with `PROCRST | PERRST | EXTRST`
forces the D+ pull-up to release and the host to re-enumerate, the
same way a real RESET button press does.

### Why the scripted flow exists

The obvious "press ERASE + bossac -e -w -v -b -R firmware.bin" flow
common in Arduino documentation does not work reliably here:

- **`bossac -R` does not reset the SAM3X** in this configuration.
  The `-R` flag issues a SAM-BA `G <appstart>#` command, which jumps
  to the app but without a peripheral reset — the Due's USB stack,
  DMA controllers, and configured clock tree stay in the SAM-BA
  state, and the application often misbehaves or hangs. Use the
  `RSTC_CR` write instead: clean full reset.

- **Triggering the 1200-baud trick a second time (e.g. to "cycle"
  the board after flashing) wipes the just-written flash.** The
  ATmega16U2 pulses the ERASE pin every time it sees 1200-baud
  open/close, and ERASE means "erase flash". So the sequence must
  be 1200-baud *once*, write, reset-via-RSTC_CR, and never 1200-baud
  again until the next upload.

- **Pressing the ERASE button works interactively** but cannot be
  done remotely. The 1200-baud trick is the equivalent driven
  entirely over USB from software.

- **`bossac -R` silently hangs on some Due + host combinations**
  (observed on the DN2800MT reference host) even when it appears
  to complete successfully from bossac's output. Always verify by
  watching `lsusb` / `dmesg` re-enumerate into the app firmware
  after flash before declaring success.

### Recovery if flash aborts

- **Between steps 3 and 5**: the Due is in SAM-BA mode waiting for
  input. Just re-run the script — the second invocation sees the
  SAM-BA device, skips to bossac, and completes.
- **After step 4 but before step 5**: new firmware is already
  written but the Due hasn't jumped to it. Press the board's
  physical RESET button (not ERASE!) and the Due boots into the
  new app.
- **Corrupt/broken app that prevents re-enum of `003e`**: press the
  physical ERASE button on the board to force SAM-BA, then re-run
  the script to flash a known-good binary.

## Method 1: Upload Pre-compiled Binary (manual / reference)

> **Note.** The instructions in this section are a manual
> reference for someone debugging the flash flow or working
> without the repo script. For day-to-day updates on the current
> PhyCommander firmware tree, use
> [`scripts/flash_firmware.sh`](#scripted-flash-recommended) — it
> handles the 1200-baud trigger, avoids `-R`, and issues a clean
> `RSTC_CR` soft-reset after bossac returns.

### Step 1: Put Arduino Due in Programming Mode

Two equivalent options:

- **Remote / scripted**: toggle the programming port at 1200 baud:
  ```bash
  stty -F /dev/ttyACM0 1200
  # the Due re-enumerates into SAM-BA within ~1 s
  ```
- **Physical**: press and release the **ERASE button** (the smaller
  of the two buttons, next to the RESET button). Do NOT press
  RESET instead — RESET just reboots the application, it does not
  enter SAM-BA.

On Linux the device appears as `/dev/ttyACM0` in both cases.

### Step 2: Upload Firmware

```bash
# Linux — NB: no -R flag, see below
bossac --port=ttyACM0 -e -w -v -b firmware.bin

# macOS (may need different port)
bossac --port=cu.usbmodem* -e -w -v -b firmware.bin

# Windows
bossac.exe -p COM3 -e -w -v -b firmware.bin
```

**Flags explained**:
- `-e` = Erase flash
- `-w` = Write firmware
- `-v` = Verify after writing
- `-b` = Boot from flash after upload (sets GPNVM1)
- `-p` = Port (Windows only, auto-detected on Linux/macOS)

**Why no `-R`**: the `-R` reset path is unreliable on SAM3X in this
configuration (it sometimes hangs; it sometimes jumps to the app
without resetting peripherals, leaving the USB stack in a broken
state). Use the `RSTC_CR` soft-reset in Step 3 instead.

### Step 3: Soft-reset into the new firmware

After bossac returns, the Due is still in SAM-BA mode. Exit cleanly
by writing `RSTC_CR = 0xA500000D` through the SAM-BA text protocol:

```bash
stty -F /dev/ttyACM0 115200 raw -echo -echoe -echok -echoctl -echoke
printf 'W400E1A00,A500000D#' > /dev/ttyACM0
# the Due resets mid-write, so we expect no reply
```

The SAM3X re-enumerates into the newly written application within
~1-2 seconds. Do **not** trigger a second 1200-baud toggle as a
"refresh" — that would erase the flash you just wrote.

### Step 3: Verify

After upload, the board will reset and run the new firmware. Verify:

```bash
# Check USB device
lsusb | grep -i arduino
# Should show: Arduino Due (Programming Port)

# Test with physerver
./target/release/physerver --auto-detect
```

## Method 2: Automated Upload Script

Create a script for easy firmware upload:

```bash
#!/bin/bash
# upload_firmware.sh

FIRMWARE="ATSAM3X8E_FW/ATSAM3X8E_FW/Debug/firmware.bin"
PORT="/dev/ttyACM0"

echo "Putting Arduino Due in programming mode..."
echo "Press the ERASE button now, then press Enter"
read

echo "Uploading firmware..."
bossac -e -w -v -b -R ${FIRMWARE}

if [ $? -eq 0 ]; then
    echo "✓ Firmware uploaded successfully!"
    sleep 2
    echo "Testing connection..."
    ./target/release/physerver --auto-detect --transport usb
else
    echo "✗ Firmware upload failed!"
    exit 1
fi
```

Make executable:
```bash
chmod +x upload_firmware.sh
./upload_firmware.sh
```

## Method 3: Upload via Arduino IDE

1. **Install Arduino IDE**:
   ```bash
   # Linux
   sudo apt-get install arduino

   # macOS
   brew install --cask arduino

   # Or download from: https://www.arduino.cc/en/software
   ```

2. **Configure Arduino IDE**:
   - Open Arduino IDE
   - Go to **Tools > Board > Boards Manager**
   - Search for "Arduino SAM Boards"
   - Install "Arduino SAM Boards (32-bits ARM Cortex-M3)"

3. **Select Board and Port**:
   - **Tools > Board > Arduino Due (Programming Port)**
   - **Tools > Port > /dev/ttyACM0** (or your port)

4. **Upload**:
   - Open your firmware .ino file
   - Click **Upload** button
   - IDE will compile and upload automatically

## Compiling Firmware from Source

### Using Atmel Studio (Windows)

1. **Install Atmel Studio 7**: https://www.microchip.com/en-us/tools-resources/develop/microchip-studio
2. **Open project**: `ATSAM3X8E_FW/ATSAM3X8E_FW/ATSAM3X8E_FW.cproj`
3. **Build**:
   - Build > Build Solution (F7)
   - Output: `Debug/ATSAM3X8E_FW.bin`

4. **Upload** using BOSSA (see Method 1)

### Using Makefile (Linux/macOS)

Create `Makefile` in firmware directory:

```makefile
# Makefile for ATSAM3X8E firmware

CC = arm-none-eabi-gcc
OBJCOPY = arm-none-eabi-objcopy
SIZE = arm-none-eabi-size

MCU = cortex-m3
ARCH = armv7-m

# Source files
SRCS = src/main.c

# Compiler flags
CFLAGS = -mcpu=$(MCU) -mthumb -O2 -g
CFLAGS += -Wall -Wextra
CFLAGS += -DBOARD=ARDUINO_DUE_X

# Linker flags
LDFLAGS = -T sam3x8e_flash.ld

TARGET = firmware

all: $(TARGET).bin

$(TARGET).elf: $(SRCS)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $^
	$(SIZE) $@

$(TARGET).bin: $(TARGET).elf
	$(OBJCOPY) -O binary $< $@

upload: $(TARGET).bin
	bossac -e -w -v -b -R $<

clean:
	rm -f $(TARGET).elf $(TARGET).bin

.PHONY: all upload clean
```

Build and upload:
```bash
make
make upload
```

## Troubleshooting

### Device not found

```bash
# Linux: Check USB devices
lsusb | grep -i arduino

# Check serial devices
ls /dev/ttyACM*

# Check permissions
sudo chmod 666 /dev/ttyACM0

# Add user to dialout group
sudo usermod -a -G dialout $USER
# Log out and back in
```

### Upload fails with "SAM-BA operation failed"

1. **Press Erase button** again
2. **Try different port**:
   ```bash
   ls /dev/ttyACM*
   bossac --port=/dev/ttyACM1 -e -w -v -b -R firmware.bin
   ```

3. **Reset the board** with the reset button, then try again

### Board doesn't respond after upload

1. **Press Reset button** on the board
2. **Check firmware is valid**:
   ```bash
   file firmware.bin
   # Should show: firmware.bin: data
   ```

3. **Re-upload** firmware in programming mode

### Permission denied on Linux

```bash
# Temporary fix
sudo bossac -e -w -v -b -R firmware.bin

# Permanent fix
sudo usermod -a -G dialout $USER
# Log out and back in
```

### macOS: Port not found

```bash
# List ports
ls /dev/cu.usbmodem*

# Use specific port
bossac --port=/dev/cu.usbmodem14201 -e -w -v -b -R firmware.bin
```

### Windows: COM port issues

1. **Open Device Manager**
2. Check **Ports (COM & LPT)**
3. Note the COM port number (e.g., COM3)
4. Use in bossac:
   ```
   bossac.exe -e -w -v -b -R -p COM3 firmware.bin
   ```

## USB Firmware Update Requirements

When using direct USB transport (not serial), the firmware must:

1. **Configure USB endpoints** properly:
   - Endpoint 0x02: Bulk OUT (host → device)
   - Endpoint 0x82: Bulk IN (device → host)

2. **Handle bulk transfers**:
   - 64-byte fixed packet size
   - No USB CDC layer
   - Direct endpoint I/O

3. **Update firmware code** (`main.c`):
   ```c
   // Replace USB CDC calls:
   // udi_cdc_read_buf() → USB_Read_EP()
   // udi_cdc_write_buf() → USB_Write_EP()
   ```

See [FIRMWARE_UPDATES.md](FIRMWARE_UPDATES.md) for detailed firmware modifications.

## Firmware Backup

**Save current firmware**:
```bash
# Read flash (NOT RECOMMENDED - complex)
# Better: Keep source code and compiled .bin file backed up

# Backup source
tar -czf firmware_backup_$(date +%Y%m%d).tar.gz ATSAM3X8E_FW/

# Backup binary
cp firmware.bin firmware_backup_$(date +%Y%m%d).bin
```

## Version Management

Add version info to firmware:

```c
// In main.c
#define FIRMWARE_VERSION "1.0.0"
#define BUILD_DATE __DATE__
#define BUILD_TIME __TIME__

// Include in status message
```

Check version:
```bash
./physerver --auto-detect
# Should show firmware version in logs
```

## Automated Firmware Build & Upload

Complete script:

```bash
#!/bin/bash
# build_and_upload.sh - Complete firmware update workflow

set -e  # Exit on error

FIRMWARE_DIR="ATSAM3X8E_FW/ATSAM3X8E_FW"
FIRMWARE_BIN="${FIRMWARE_DIR}/Debug/firmware.bin"

echo "=== PhyCMD Firmware Build & Upload ==="
echo

# Step 1: Build firmware (if using Makefile)
echo "[1/3] Building firmware..."
cd ${FIRMWARE_DIR}
make clean
make
cd -

# Verify binary exists
if [ ! -f "${FIRMWARE_BIN}" ]; then
    echo "ERROR: Firmware binary not found: ${FIRMWARE_BIN}"
    exit 1
fi

echo "✓ Firmware built: $(stat -f%z ${FIRMWARE_BIN} 2>/dev/null || stat -c%s ${FIRMWARE_BIN}) bytes"
echo

# Step 2: Upload firmware
echo "[2/3] Uploading firmware..."
echo "→ Press ERASE button on Arduino Due"
echo "→ Wait 1 second"
echo "→ Press Enter to continue"
read

bossac -e -w -v -b -R ${FIRMWARE_BIN}

if [ $? -eq 0 ]; then
    echo "✓ Firmware uploaded successfully!"
else
    echo "✗ Firmware upload failed!"
    exit 1
fi

echo

# Step 3: Test
echo "[3/3] Testing connection..."
sleep 2

./target/release/physerver --auto-detect --transport usb &
SERVER_PID=$!

sleep 3

# Test REST API
RESPONSE=$(curl -s http://localhost:8080/api/status)

if [ -n "$RESPONSE" ]; then
    echo "✓ Connection successful!"
    echo "Response: $RESPONSE"
else
    echo "✗ Connection test failed!"
fi

kill $SERVER_PID 2>/dev/null

echo
echo "=== Firmware Update Complete ==="
```

Usage:
```bash
chmod +x build_and_upload.sh
./build_and_upload.sh
```

## Quick Reference

| Task | Command |
|------|---------|
| Upload firmware | `bossac -e -w -v -b -R firmware.bin` |
| List ports (Linux) | `ls /dev/ttyACM*` |
| List ports (macOS) | `ls /dev/cu.usbmodem*` |
| Test connection | `./physerver --auto-detect` |
| Switch to USB | `./physerver --transport usb` |
| Switch to Serial | `./physerver --transport serial --port /dev/ttyACM0` |

## Safety Notes

⚠️ **Important**:
- Always backup firmware before updates
- Don't disconnect during upload
- Verify firmware before uploading
- Use Programming Port, not Native USB Port
- Press Erase button to enter programming mode

## Next Steps

After firmware upload:
1. **Test with serial transport**: `./physerver --transport serial`
2. **Test with USB transport**: `./physerver --transport usb`
3. **Run benchmarks**: See [PERFORMANCE.md](../technical/PERFORMANCE.md)
4. **Configure for production**: Edit `config.toml`

## Support

For firmware upload issues:
- Check BOSSA documentation: https://www.shumatech.com/web/products/bossa
- Arduino Due guide: https://www.arduino.cc/en/Guide/ArduinoDue
- Create issue with error message and system details
