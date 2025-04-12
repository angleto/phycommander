# PhyCMD Setup Guide

## System Requirements

### Hardware
- ATSAM3X8E microcontroller (Arduino Due) with PhyCMD firmware
- USB cable for serial communication
- Linux or macOS host computer

### Software Dependencies

#### Linux (Ubuntu/Debian)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install system dependencies
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libudev-dev \
    libssl-dev

# Add user to dialout group for serial port access
sudo usermod -a -G dialout $USER
# Log out and back in for group membership to take effect
```

#### macOS

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install homebrew if not already installed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies (minimal, most deps are included)
brew install pkg-config
```

## Building the Project

### Build physerver

```bash
cd physerver
cargo build --release
```

The binary will be located at `target/release/physerver`.

### Build firmware (phyextension)

The existing ATSAM3X8E firmware is located in `ATSAM3X8E_FW/`. To update it:

**Option 1: Using Atmel Studio (Windows)**
1. Open `ATSAM3X8E_FW/ATSAM3X8E_FW/ATSAM3X8E_FW.cproj`
2. Build the project
3. Flash to device using J-Link or similar

**Option 2: Using Arduino IDE**
1. The firmware can be adapted to work with Arduino IDE
2. Install Arduino Due board support
3. Compile and upload

**Option 3: Using BOSSA (Linux/Mac)**
```bash
# Install bossac
sudo apt-get install bossa-cli  # Linux
brew install bossa              # macOS

# Flash firmware
bossac -e -w -v -b firmware.bin
```

## Running the System

### 1. Flash the Firmware

First, ensure your ATSAM3X8E is programmed with the phyextension firmware.

### 2. Connect the Device

Connect the Arduino Due to your computer via USB. The device should appear as a serial port:
- Linux: `/dev/ttyACM0` or `/dev/ttyUSB0`
- macOS: `/dev/cu.usbmodem*` or `/dev/tty.usbmodem*`

Verify connection:
```bash
# Linux
ls /dev/ttyACM*

# macOS
ls /dev/cu.usbmodem*
```

### 3. Start physerver

**Auto-detect device (recommended):**
```bash
./target/release/physerver --auto-detect
```

**Specify port manually:**
```bash
./target/release/physerver --port /dev/ttyACM0 --baud 921600
```

**Enable real-time mode (Linux only, requires capabilities):**
```bash
# Grant capabilities (one-time)
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip target/release/physerver

# Run with RT enabled
./target/release/physerver --auto-detect --rt --rate 5000
```

### 4. Access the Web Interface

Open your browser to:
```
http://localhost:8080
```

You should see the PhyCMD dashboard with:
- Real-time GPIO input/output controls
- DAC sliders
- ADC readings
- System telemetry

## Testing

### Run a test client

```bash
# Terminal 1: Start physerver
./target/release/physerver --auto-detect

# Terminal 2: Run example client
cargo run --example simple_client
```

The example client will:
- Connect to physerver via shared memory IPC
- Blink GPIO pins in a rotating pattern
- Ramp DAC outputs
- Display ADC readings and telemetry

### REST API Testing

```bash
# Get current status
curl http://localhost:8080/api/status | jq

# Set GPIO pin 3 high
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 3, "value": true}'

# Set DAC channel 0 to mid-scale
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 2047}'
```

## Troubleshooting

### Serial Port Permission Denied

```bash
# Add user to dialout group
sudo usermod -a -G dialout $USER

# Or run with sudo (not recommended for production)
sudo ./target/release/physerver --auto-detect
```

### Device Not Found

```bash
# List all serial devices
ls /dev/tty* | grep -E '(ACM|USB|usbmodem)'

# Check dmesg for device enumeration
dmesg | tail -20

# Verify USB connection
lsusb | grep -i "arduino\|atmel"
```

### Build Fails - libudev not found

```bash
# Install libudev development package
sudo apt-get install libudev-dev pkg-config
```

### Real-time Scheduling Fails

```bash
# Check if you have RT capabilities
ulimit -r

# Grant capabilities to binary
sudo setcap cap_sys_nice,cap_ipc_lock=eip target/release/physerver

# Verify capabilities
getcap target/release/physerver
```

### Shared Memory Access Denied

```bash
# Check existing shared memory
ls /dev/shm/

# Clean up stale shared memory
rm /dev/shm/phycmd_state

# Restart physerver
```

## Performance Tuning

### For Best Real-time Performance (Linux)

```bash
# 1. Disable CPU frequency scaling
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# 2. Disable CPU idle states
sudo cpupower idle-set -D 0

# 3. Isolate CPU cores for real-time (add to kernel cmdline)
# Edit /etc/default/grub:
# GRUB_CMDLINE_LINUX="isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3"
# Then: sudo update-grub && reboot

# 4. Run physerver with high priority on isolated core
sudo ./target/release/physerver --auto-detect --rt --rt-priority 90 --cpu-core 2
```

### Verify Performance

Check real-time metrics:
- Loop time should be < 200 µs at 5kHz
- Jitter should be < 10 µs
- CPU usage should be < 50% on assigned core

Monitor via web interface or:
```bash
curl http://localhost:8080/api/status | jq '.loop_time_us, .uptime_ms'
```

## Next Steps

1. Read `ARCHITECTURE.md` for system design details - see [ARCHITECTURE.md](../technical/ARCHITECTURE.md)
2. Read `PROTOCOL.md` for communication protocol specification - see [PROTOCOL.md](../technical/PROTOCOL.md)
3. Review `physerver/README.md` for server API documentation
4. Write custom test logic using the IPC or REST API
5. Customize the web interface in `physerver/static/index.html`

## Support

For issues and questions:
- Check the documentation in the `docs/` directory
- Review existing issues in the repository
- Create a new issue with:
  - System information (OS, hardware)
  - physerver logs (run with `RUST_LOG=debug`)
  - Firmware version
  - Steps to reproduce

## License

MIT
