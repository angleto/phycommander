# PhyCMD Quick Reference

One-page reference for common operations.

## Quick Start

```bash
# Build
cd physerver && cargo build --release

# Run (auto-detect USB or Serial)
./target/release/physerver --auto-detect

# Access web interface
open http://localhost:8080
```

## Command-Line Usage

```bash
# Auto-detect (USB preferred)
./physerver --auto-detect

# Force USB mode (10 kHz capable)
./physerver --transport usb

# Force serial mode
./physerver --transport serial --port /dev/ttyACM0

# High-performance mode
./physerver --transport usb --rate 5000 --rt --rt-priority 90

# With configuration file
./physerver --config config.toml

# Override configuration
./physerver --config config.toml --transport usb --rate 5000
```

## Configuration File (config.toml)

```toml
[transport]
type = "usb"              # "usb" or "serial"
serial_port = "/dev/ttyACM0"
baud_rate = 921600
update_rate = 5000        # Hz
auto_detect = true

[web]
host = "0.0.0.0"
port = 8080

[ipc]
enabled = true
shm_name = "/physerver"

[realtime]
enabled = false
priority = 80
cpu_affinity = false
cpu_core = 2
```

## REST API

```bash
# Get status
curl http://localhost:8080/api/status

# Set GPIO pin 5 high
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 5, "value": true}'

# Set DAC channel 0 to 1.65V (mid-scale)
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 2047}'
```

## WebSocket (JavaScript)

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onmessage = (event) => {
  const status = JSON.parse(event.data);
  console.log('ADC 0:', status.adc[0]);
};

// Send command
ws.send(JSON.stringify({
  digital_out: 0xFF,
  dac: [2047, 4095],
  pwm: [0, 0],
  flags: {adc_enable: true, dac_enable: true},
  seq_num: 0
}));
```

## IPC (Rust)

```rust
use physerver::{IpcClient, Command, CommandFlags};

let client = IpcClient::connect()?;

// Write command
let mut cmd = Command::default();
cmd.digital_out = 0x00FF;
cmd.dac[0] = 2047;
cmd.flags = CommandFlags {
    adc_enable: true,
    dac_enable: true,
    ..Default::default()
};
client.write_command(&cmd);

// Read status
let status = client.read_status();
println!("ADC 0: {}", status.adc[0]);
```

## Transport Comparison

| Feature | USB Bulk | Serial |
|---------|----------|--------|
| Max Rate | 10 kHz | 1 kHz |
| Latency | 120 µs | 750 µs |
| Setup | Requires libudev | Plug-and-play |
| Command | `--transport usb` | `--transport serial` |

## Common Operations

### Set Multiple GPIO Pins

```bash
# Python
import requests
requests.post('http://localhost:8080/api/command', json={
    'digital_out': 0x00FF,  # Pins 0-7 high
    'dac': [2047, 0],
    'pwm': [0, 0],
    'flags': {'adc_enable': True, 'dac_enable': True}
})
```

### Read ADC Values

```bash
# Curl + jq
curl -s http://localhost:8080/api/status | jq '.adc'

# Python
status = requests.get('http://localhost:8080/api/status').json()
print(f"ADC 0: {status['adc'][0]}")
```

### Monitor Loop Time

```bash
# Watch command (Linux/macOS)
watch -n 0.1 'curl -s http://localhost:8080/api/status | jq .loop_time_us'
```

## Data Ranges

| Type | Range | Resolution |
|------|-------|------------|
| Digital I/O | 0-1 | 1 bit |
| ADC | 0-4095 | 12-bit (0-3.3V) |
| DAC | 0-4095 | 12-bit (0-3.3V) |
| PWM | 0-65535 | 16-bit |

## Voltage Conversion

```
ADC: voltage = (value / 4095) * 3.3V
DAC: value = (voltage / 3.3V) * 4095
```

## Troubleshooting

```bash
# Device not detected
lsusb | grep -i arduino
ls /dev/ttyACM*
sudo usermod -a -G dialout $USER  # Log out/in

# USB transport not available
sudo apt-get install libudev-dev pkg-config
cargo build --release --features usb

# Enable permissions
sudo setcap cap_sys_nice,cap_ipc_lock=eip target/release/physerver

# Create udev rules
echo 'SUBSYSTEM=="usb", ATTR{idVendor}=="2341", ATTR{idProduct}=="003e", MODE="0666"' | \
  sudo tee /etc/udev/rules.d/99-arduino-due.rules
sudo udevadm control --reload-rules
sudo udevadm trigger

# Check logs
RUST_LOG=debug ./physerver --auto-detect
```

## Real-Time Setup

```bash
# Grant capabilities
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip target/release/physerver

# Run with RT
./physerver --auto-detect --rt --rt-priority 90

# System tuning
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

## Firmware Upload

```bash
# Install BOSSA
sudo apt-get install bossa-cli

# Press Erase button on Arduino Due, wait 1 second

# Upload firmware
bossac -e -w -v -b -R firmware.bin
```

## File Locations

| File | Location |
|------|----------|
| Binary | `physerver/target/release/physerver` |
| Config | `physerver/config.toml` (or specify with --config) |
| Static files | `physerver/static/` |
| Examples | `physerver/examples/` |

## Environment Variables

```bash
# Logging level
RUST_LOG=debug ./physerver --auto-detect
RUST_LOG=trace ./physerver --auto-detect

# Module-specific logging
RUST_LOG=physerver::transport=trace ./physerver --auto-detect
```

## Performance Targets

**USB Bulk Mode**:
- Update rate: 5-10 kHz
- Latency: 100-120 µs
- Jitter: ±5-10 µs

**Serial Mode**:
- Update rate: 800-1000 Hz
- Latency: 700-750 µs
- Jitter: ±20-35 µs

## Pin Mapping

**ADC**: A0-A7 (0-3.3V, 12-bit)
**DAC**: DAC0, DAC1 (0-3.3V, 12-bit)
**Digital I/O**: Configured in firmware `conf_board.h`

## URLs

- Web UI: http://localhost:8080
- REST API: http://localhost:8080/api
- WebSocket: ws://localhost:8080/ws
- Status: http://localhost:8080/api/status

## Documentation

- [USER_MANUAL.md](USER_MANUAL.md) - Complete user guide
- [API_REFERENCE.md](API_REFERENCE.md) - Detailed API docs
- [CONFIGURATION.md](CONFIGURATION.md) - Configuration guide
- [DEPLOYMENT.md](DEPLOYMENT.md) - Production deployment
- [PERFORMANCE.md](PERFORMANCE.md) - Performance benchmarks
- [BUILDING.md](BUILDING.md) - Build instructions

---

**Tip**: Use `./physerver --help` for full command-line options.
