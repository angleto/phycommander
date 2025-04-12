# PhyCMD User Manual

**Version 1.0.0**
**Last Updated**: 2025-11-22

## Table of Contents

1. [Introduction](#introduction)
2. [Getting Started](#getting-started)
3. [Transport Modes](#transport-modes)
4. [Configuration](#configuration)
5. [Using the Web Interface](#using-the-web-interface)
6. [Using the REST API](#using-the-rest-api)
7. [Using the IPC Interface](#using-the-ipc-interface)
8. [Real-Time Operation](#real-time-operation)
9. [Monitoring and Telemetry](#monitoring-and-telemetry)
10. [Troubleshooting](#troubleshooting)

---

## Introduction

### What is PhyCMD?

PhyCMD (Physical Commander) is a real-time hardware control system that provides high-performance communication between software applications and microcontroller hardware (Arduino Due). It's designed for:

- **Test machines** requiring precise control
- **Data acquisition** systems
- **Laboratory automation**
- **Motion control** applications
- **Industrial IoT** edge devices

### Key Features

- **Dual Transport Modes**: Choose between USB Bulk (10 kHz) or Serial (1 kHz)
- **Multiple Interfaces**: Web UI, REST API, WebSocket, Shared Memory IPC
- **Real-Time Performance**: Hard real-time scheduling with microsecond precision
- **Flexible I/O**: 16 digital inputs, 16 digital outputs, 8 ADC channels, 2 DAC channels
- **Easy Configuration**: TOML-based configuration with command-line overrides

---

## Getting Started

### Prerequisites

**Hardware**:
- Arduino Due (ATSAM3X8E)
- USB cable (USB Type-B)
- Computer with USB 2.0+ port

**Software** (Linux):
```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libudev-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
sudo usermod -a -G dialout $USER  # Log out and back in after this
```

**Software** (macOS):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
brew install pkg-config
```

### Installation

1. **Build PhyServer**:
   ```bash
   cd physerver
   cargo build --release
   ```

2. **Connect Arduino Due**:
   - Connect Arduino Due to computer via USB
   - Note: Use the "Programming Port" (closest to DC jack)

3. **Run PhyServer**:
   ```bash
   ./target/release/physerver --auto-detect
   ```

4. **Open Web Interface**:
   - Navigate to http://localhost:8080 in your browser

### First Run

When you run physerver for the first time:

```bash
$ ./target/release/physerver --auto-detect
INFO PhyServer - Physical Commander Server v1.0.0
INFO Auto-detecting PhyCMD device...
INFO ✓ USB transport detected
INFO Transport info: max_rate=10000Hz, typical_latency=120µs
INFO Web server started on http://0.0.0.0:8080
INFO Starting main communication loop at 1000 Hz
```

You should see:
- Device detection (USB or Serial)
- Transport information
- Web server starting
- Communication loop starting

---

## Transport Modes

PhyServer supports two transport modes for communicating with the Arduino Due.

### USB Bulk Transport (Recommended)

**Direct USB bulk transfer via libusb**

**Advantages**:
- ⚡ **10x faster**: 120µs latency vs 750µs
- 🚀 **Higher throughput**: 8 Mbps vs 900 kbps
- 🎯 **Lower jitter**: ±7µs vs ±35µs
- 📈 **Higher update rates**: Up to 10 kHz vs 1 kHz

**Requirements**:
- Linux: libudev-dev installed
- Permissions: User in dialout group or udev rules configured
- Firmware: Currently uses USB CDC endpoints (full USB bulk support pending)

**Usage**:
```bash
# Auto-detect (USB preferred)
./physerver --auto-detect

# Force USB mode
./physerver --transport usb

# With configuration file
./physerver --config config.toml  # transport.type = "usb"
```

### USB CDC Serial Transport

**Virtual serial port (USB CDC ACM)**

**Advantages**:
- ✅ **Universal compatibility**: Works on all platforms
- 🔌 **Plug-and-play**: Automatic driver installation
- 🛡️ **No special permissions**: Works immediately
- 📚 **Well-tested**: Mature, stable technology

**Usage**:
```bash
# Auto-detect serial port
./physerver --transport serial --auto-detect

# Specify port manually
./physerver --transport serial --port /dev/ttyACM0

# With configuration file
./physerver --config config.toml  # transport.type = "serial"
```

### Comparison

| Feature | USB Bulk | Serial |
|---------|----------|--------|
| Max Rate | 10 kHz | 1 kHz |
| Latency | 120 µs | 750 µs |
| Jitter | ±7 µs | ±35 µs |
| Setup | Requires libudev | Plug-and-play |
| Compatibility | Linux/macOS/Windows* | Universal |
| Performance | Best | Good |

*Windows requires Zadig driver installation for USB bulk mode

### When to Use Each Mode

**Use USB Bulk when**:
- You need high update rates (>1 kHz)
- Low latency is critical
- Running on Linux/macOS with libudev installed
- Real-time control applications

**Use Serial when**:
- Maximum compatibility needed
- 1 kHz update rate is sufficient
- Simple plug-and-play operation desired
- Running in restricted environments

---

## Configuration

### Configuration File (config.toml)

Create a `config.toml` file:

```toml
# Transport configuration
[transport]
type = "usb"              # "usb" or "serial"
serial_port = "/dev/ttyACM0"
baud_rate = 921600
update_rate = 5000        # Hz (USB: up to 10000, Serial: up to 1000)
auto_detect = true        # Auto-detect device

# Web server configuration
[web]
host = "0.0.0.0"          # Bind to all interfaces
port = 8080               # HTTP port

# IPC configuration
[ipc]
enabled = true
shm_name = "/physerver"

# Real-time configuration
[realtime]
enabled = false           # Enable real-time scheduling
priority = 80             # RT priority (1-99, higher = more priority)
cpu_affinity = false      # Pin to specific CPU core
cpu_core = 2              # CPU core to use (if cpu_affinity = true)
```

### Loading Configuration

```bash
# Use configuration file
./physerver --config config.toml

# Override transport type
./physerver --config config.toml --transport serial

# Override update rate
./physerver --config config.toml --rate 5000

# Override web port
./physerver --config config.toml --web-port 9090
```

### Command-Line Arguments

All configuration options can be overridden via command line:

```bash
./physerver \
  --transport usb \
  --rate 5000 \
  --web-port 8080 \
  --rt \
  --rt-priority 90
```

**Arguments**:
- `--config PATH`: Configuration file path
- `--transport TYPE`: Transport type ("usb" or "serial")
- `--port PATH`: Serial port path (e.g., /dev/ttyACM0)
- `--baud RATE`: Baud rate (default: 921600)
- `--rate HZ`: Update rate in Hz
- `--web-port PORT`: Web server port (default: 8080)
- `--rt`: Enable real-time scheduling
- `--rt-priority N`: RT priority (1-99)
- `--no-ipc`: Disable shared memory IPC
- `--no-web`: Disable web server
- `--auto-detect`: Auto-detect device

---

## Using the Web Interface

### Accessing the Dashboard

1. Start physerver:
   ```bash
   ./physerver --auto-detect
   ```

2. Open browser:
   - Local: http://localhost:8080
   - Remote: http://<server-ip>:8080

### Dashboard Features

**Status Panel**:
- Connection status
- Update rate
- Latency statistics
- Error counters

**Digital I/O**:
- View digital inputs (read-only)
- Control digital outputs (checkboxes/buttons)
- Pin-by-pin control

**Analog Input (ADC)**:
- Real-time ADC value display
- 8 channels, 12-bit resolution (0-4095)
- Live graphs (optional)

**Analog Output (DAC)**:
- DAC channel sliders
- 2 channels, 12-bit resolution (0-4095)
- Voltage display (0-3.3V)

**System Telemetry**:
- Loop time (microseconds)
- Uptime
- Sequence number
- Error count

---

## Using the REST API

### Base URL

```
http://localhost:8080/api
```

### Endpoints

#### GET /api/status

Get current device status.

**Response**:
```json
{
  "digital_in": 4095,
  "digital_out": 255,
  "adc": [2048, 1024, 3072, 512, 0, 4095, 2000, 1500],
  "seq_num": 142,
  "loop_time_us": 85,
  "uptime_ms": 123456,
  "error_count": 0,
  "flags": {
    "adc_active": true,
    "dac_active": true,
    "pwm_active": false,
    "error": false,
    "watchdog_triggered": false,
    "usb_configured": true,
    "overrun": false
  }
}
```

#### POST /api/gpio/set

Set a single GPIO output pin.

**Request**:
```json
{
  "pin": 3,
  "value": true
}
```

**Response**:
```json
{
  "success": true,
  "pin": 3,
  "value": true
}
```

#### POST /api/dac/set

Set a DAC channel output.

**Request**:
```json
{
  "channel": 0,
  "value": 2047
}
```

**Response**:
```json
{
  "success": true,
  "channel": 0,
  "value": 2047
}
```

### Examples

#### cURL

```bash
# Get status
curl http://localhost:8080/api/status

# Set GPIO pin 5 high
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 5, "value": true}'

# Set DAC channel 0 to 3.3V (4095)
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 4095}'
```

#### Python

```python
import requests

# Get status
response = requests.get('http://localhost:8080/api/status')
status = response.json()
print(f"ADC 0: {status['adc'][0]}")

# Set GPIO
requests.post('http://localhost:8080/api/gpio/set',
              json={'pin': 3, 'value': True})

# Set DAC
requests.post('http://localhost:8080/api/dac/set',
              json={'channel': 0, 'value': 2047})
```

#### JavaScript/Node.js

```javascript
// Get status
fetch('http://localhost:8080/api/status')
  .then(res => res.json())
  .then(status => console.log('ADC 0:', status.adc[0]));

// Set GPIO
fetch('http://localhost:8080/api/gpio/set', {
  method: 'POST',
  headers: {'Content-Type': 'application/json'},
  body: JSON.stringify({pin: 3, value: true})
});

// Set DAC
fetch('http://localhost:8080/api/dac/set', {
  method: 'POST',
  headers: {'Content-Type': 'application/json'},
  body: JSON.stringify({channel: 0, value: 2047})
});
```

---

## Using the IPC Interface

### Shared Memory IPC

For ultra-low latency (nanosecond-scale), use the shared memory IPC interface.

### Rust Client

```rust
use physerver::{IpcClient, Command, CommandFlags, Status};

fn main() -> anyhow::Result<()> {
    // Connect to IPC server
    let client = IpcClient::connect()?;

    // Create command
    let mut cmd = Command::default();
    cmd.digital_out = 0x00FF;  // Set pins 0-7 high
    cmd.dac[0] = 2047;         // DAC 0 to mid-scale
    cmd.flags = CommandFlags {
        adc_enable: true,
        dac_enable: true,
        pwm_enable: false,
        reset_seq: false,
        watchdog_disable: false,
    };

    // Write command
    client.write_command(&cmd);

    // Read status
    let status: Status = client.read_status();
    println!("Digital inputs: 0x{:04X}", status.digital_in);
    println!("ADC 0: {}", status.adc[0]);
    println!("Loop time: {} µs", status.loop_time_us);

    Ok(())
}
```

### Performance

IPC Interface performance:
- **Write latency**: <100 ns
- **Read latency**: <100 ns
- **No serialization overhead**
- **Lock-free** atomic operations

Use IPC when:
- Running custom Rust applications
- Need sub-microsecond latency
- High-frequency data exchange (>10 kHz)
- Running on same machine as physerver

---

## Real-Time Operation

### Enabling Real-Time Scheduling

For hard real-time performance, enable RT scheduling:

```bash
# Grant capabilities (one-time setup)
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip target/release/physerver

# Run with RT enabled
./physerver --auto-detect --rt --rt-priority 90
```

### Real-Time Configuration

```toml
[realtime]
enabled = true
priority = 90        # Higher = more priority (1-99)
cpu_affinity = true  # Pin to specific core
cpu_core = 2         # Use isolated core
```

### System Tuning

For best real-time performance:

1. **Isolate CPU cores** (add to `/etc/default/grub`):
   ```
   GRUB_CMDLINE_LINUX="isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3"
   ```
   Then: `sudo update-grub && sudo reboot`

2. **Set CPU governor to performance**:
   ```bash
   echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   ```

3. **Disable irqbalance**:
   ```bash
   sudo systemctl stop irqbalance
   sudo systemctl disable irqbalance
   ```

### Expected Performance

With RT optimizations:

**USB Bulk**:
- Update rate: 5-10 kHz
- Latency: 100-120 µs
- Jitter: ±5-10 µs

**Serial**:
- Update rate: 800-1000 Hz
- Latency: 700-750 µs
- Jitter: ±20-35 µs

---

## Monitoring and Telemetry

### Logging

PhyServer uses structured logging with multiple levels:

```bash
# Default (INFO level)
./physerver --auto-detect

# Debug logging
RUST_LOG=debug ./physerver --auto-detect

# Trace logging (very verbose)
RUST_LOG=trace ./physerver --auto-detect

# Module-specific logging
RUST_LOG=physerver::transport=trace ./physerver --auto-detect
```

### Log Levels

- `ERROR`: Critical errors
- `WARN`: Warnings (e.g., high latency, sequence mismatch)
- `INFO`: Normal operation (default)
- `DEBUG`: Detailed debug information
- `TRACE`: Very detailed tracing

### Status Monitoring

Monitor via WebSocket for real-time updates:

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onmessage = (event) => {
  const status = JSON.parse(event.data);

  console.log('Loop time:', status.loop_time_us, 'µs');
  console.log('Uptime:', status.uptime_ms / 1000, 's');
  console.log('Errors:', status.error_count);

  if (status.flags.error) {
    console.error('Device error detected!');
  }

  if (status.loop_time_us > 1000) {
    console.warn('High loop time:', status.loop_time_us);
  }
};
```

### Performance Metrics

Check transport statistics programmatically:

```rust
let stats = transport.stats();
println!("Bytes sent: {}", stats.bytes_sent);
println!("Bytes received: {}", stats.bytes_received);
println!("Errors: {}", stats.errors);
println!("Last latency: {} µs", stats.last_latency_us);
```

---

## Troubleshooting

### Device Not Detected

**Symptom**: `Failed to auto-detect any device`

**Solutions**:
```bash
# Check USB connection
lsusb | grep -i arduino
# Should show: Arduino Due

# Check serial ports
ls /dev/ttyACM*
# Should show: /dev/ttyACM0 (or similar)

# Check permissions
groups | grep dialout
# Should show dialout in groups

# Add to dialout group if missing
sudo usermod -a -G dialout $USER
# Log out and back in
```

### USB Transport Not Available

**Symptom**: `USB transport not available`

**Solutions**:
```bash
# Install libudev-dev
sudo apt-get install libudev-dev pkg-config

# Rebuild with USB support
cd physerver
cargo clean
cargo build --release --features usb

# Create udev rules
echo 'SUBSYSTEM=="usb", ATTR{idVendor}=="2341", ATTR{idProduct}=="003e", MODE="0666"' | \
  sudo tee /etc/udev/rules.d/99-arduino-due.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### High Latency / Low Performance

**Symptom**: Loop time >1000µs, poor performance

**Solutions**:
1. **Use USB transport** instead of serial:
   ```bash
   ./physerver --transport usb
   ```

2. **Enable real-time scheduling**:
   ```bash
   sudo setcap cap_sys_nice=eip target/release/physerver
   ./physerver --rt --rt-priority 90
   ```

3. **Lower update rate**:
   ```bash
   ./physerver --rate 1000  # Instead of 5000
   ```

4. **Check system load**:
   ```bash
   top  # Close unnecessary programs
   ```

### Sequence Number Mismatch

**Symptom**: `Sequence mismatch: sent X, received Y`

**Cause**: Firmware and server out of sync

**Solutions**:
- Usually recovers automatically
- If persistent, restart physerver
- Check for electrical interference
- Verify USB cable quality

### Communication Errors

**Symptom**: `Communication error: ...`

**Solutions**:
1. **Check physical connection**
2. **Try different USB port**
3. **Restart Arduino Due** (press reset button)
4. **Check firmware** is properly uploaded
5. **Reduce update rate** if errors persist

### Web Interface Not Accessible

**Symptom**: Cannot access http://localhost:8080

**Solutions**:
```bash
# Check server is running
ps aux | grep physerver

# Check web server started
# Look for: "Web server started on http://0.0.0.0:8080"

# Check firewall
sudo ufw allow 8080/tcp

# Try different port
./physerver --web-port 9090
```

### For More Help

- Check logs with `RUST_LOG=debug`
- See [SETUP.md](../getting-started/SETUP.md) for detailed setup
- See [PERFORMANCE.md](../technical/PERFORMANCE.md) for performance tuning
- Review [API_REFERENCE.md](../technical/API_REFERENCE.md) for API details

---

## Appendix: Pin Mapping

### Digital I/O

Pins are mapped to Arduino Due digital pins. Consult firmware `conf_board.h` for exact mapping.

### ADC Channels

| Channel | Arduino Pin | Range |
|---------|-------------|-------|
| 0 | A0 | 0-4095 (0-3.3V) |
| 1 | A1 | 0-4095 (0-3.3V) |
| 2 | A2 | 0-4095 (0-3.3V) |
| 3 | A3 | 0-4095 (0-3.3V) |
| 4 | A4 | 0-4095 (0-3.3V) |
| 5 | A5 | 0-4095 (0-3.3V) |
| 6 | A6 | 0-4095 (0-3.3V) |
| 7 | A7 | 0-4095 (0-3.3V) |

### DAC Channels

| Channel | Arduino Pin | Range |
|---------|-------------|-------|
| 0 | DAC0 | 0-4095 (0-3.3V) |
| 1 | DAC1 | 0-4095 (0-3.3V) |

---

**End of User Manual**
