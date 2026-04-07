## Firmware Upload Guide

Complete guide for uploading firmware to Arduino Due using the Programming Port.

## Overview

The Arduino Due has **two USB ports**:

1. **Programming Port** (closest to DC jack)
   - Native USB port (SAM3X's USB peripheral)
   - Used for uploading firmware via BOSSA
   - Also used for high-speed bulk transfers (physerver with USB transport)
   - **This is the port you use for firmware upload**

2. **Native USB Port** (closest to reset button)
   - Can also be used for communication
   - Not typically used for programming

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

## Method 1: Upload Pre-compiled Binary (Recommended)

### Step 1: Put Arduino Due in Programming Mode

1. **Connect** the Arduino Due's **Programming Port** to your computer via USB
2. **Press and release** the **Erase button** (small button near the ATSAM chip)
3. Wait 1 second
4. The board is now in programming mode (LED should pulse)

**Note**: On Linux, the device appears as `/dev/ttyACM0` or similar

### Step 2: Upload Firmware

```bash
# Linux
bossac -e -w -v -b -R firmware.bin

# macOS (may need different port)
bossac --port=/dev/cu.usbmodem* -e -w -v -b -R firmware.bin

# Windows
bossac.exe -e -w -v -b -R -p COM3 firmware.bin
```

**Flags explained**:
- `-e` = Erase flash
- `-w` = Write firmware
- `-v` = Verify after writing
- `-b` = Boot from flash after upload
- `-R` = Reset after upload
- `-p` = Port (Windows only, auto-detected on Linux/macOS)

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

See `FIRMWARE_UPDATES.md` for detailed firmware modifications.

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
3. **Run benchmarks**: See `docs/PERFORMANCE.md`
4. **Configure for production**: Edit `config.toml`

## Support

For firmware upload issues:
- Check BOSSA documentation: https://www.shumatech.com/web/products/bossa
- Arduino Due guide: https://www.arduino.cc/en/Guide/ArduinoDue
- Create issue with error message and system details
