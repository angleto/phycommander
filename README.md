# PhyCMD - Physical Commander

**Real-time hardware control system for test machines and precision mechanical devices**

## Overview

PhyCMD is a complete hardware control framework providing hard real-time communication between high-level software and microcontroller hardware. It's designed for applications requiring precise timing, reliable data transfer, and flexible control interfaces.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Application Layer                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │   Web    │  │  Custom  │  │  Test    │              │
│  │  Browser │  │  Rust    │  │  Scripts │              │
│  └─────┬────┘  └────┬─────┘  └────┬─────┘              │
│        │            │             │                     │
│     WebSocket    IPC (SHM)     REST API                 │
└────────┼────────────┼─────────────┼─────────────────────┘
         │            │             │
┌────────┴────────────┴─────────────┴─────────────────────┐
│                    physerver (Rust)                     │
│  • Real-time serial communication (5kHz)                │
│  • Protocol encoding/decoding (CRC-16)                  │
│  • Multi-interface server                              │
│  • Telemetry and monitoring                            │
└──────────────────────┬──────────────────────────────────┘
                       │
                  USB CDC Serial
                  (921600+ baud)
                       │
┌──────────────────────┴──────────────────────────────────┐
│           phyextension (ATSAM3X8E firmware)             │
│  • 16 digital I/O                                       │
│  • 8-channel ADC (DMA buffered)                         │
│  • 2-channel DAC (12-bit)                               │
│  • Fixed 64-byte protocol                               │
└─────────────────────────────────────────────────────────┘
```

## Components

### 1. physerver (Rust)
High-performance server for real-time communication with the microcontroller.

**Features**:
- 🚀 Real-time serial I/O with microsecond precision
- 🔒 Memory-safe Rust implementation
- 🌐 Built-in web server with REST API
- 📡 WebSocket support for live updates
- 💾 Shared memory IPC for ultra-low latency
- ⚡ Lock-free data structures
- 🔄 Automatic device detection

**Location**: `physerver/`
**Documentation**: [physerver/README.md](physerver/README.md)

### 2. phyextension (Firmware)
Microcontroller firmware for ATSAM3X8E (Arduino Due compatible).

**Features**:
- ⚡ DMA-based ADC sampling (minimal CPU overhead)
- 🔌 16 digital inputs + 16 digital outputs
- 📊 8-channel 12-bit ADC
- 🎛️ 2-channel 12-bit DAC
- 📦 Fixed 64-byte protocol
- 🔁 USB CDC virtual serial port

**Location**: `ATSAM3X8E_FW/`
**Updates needed**: [FIRMWARE_UPDATES.md](FIRMWARE_UPDATES.md)

### 3. phywebapp (Web Interface)
Modern web dashboard for monitoring and control.

**Features**:
- 🎮 Real-time GPIO control
- 📈 Live ADC readings
- 🎚️ DAC output sliders
- 📊 System telemetry
- 📱 Responsive design
- ⚡ WebSocket live updates

**Location**: `physerver/static/`
**Access**: http://localhost:8080 (when physerver is running)

## Quick Start

### Prerequisites

**Linux (Ubuntu/Debian)**:
```bash
# Install dependencies
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libudev-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add user to dialout group for serial access
sudo usermod -a -G dialout $USER
# Log out and back in
```

**macOS**:
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install pkg-config (optional)
brew install pkg-config
```

### Build & Run

```bash
# 1. Build physerver
cd physerver
cargo build --release

# 2. Connect Arduino Due via USB

# 3. Run physerver (auto-detect device)
./target/release/physerver --auto-detect

# 4. Open web interface
# Visit http://localhost:8080 in your browser
```

### Test with Example Client

```bash
# Terminal 1: Run physerver
./target/release/physerver --auto-detect

# Terminal 2: Run example client
cargo run --example simple_client
```

## Documentation

| Document | Description |
|----------|-------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | System design, components, technology stack |
| [PROTOCOL.md](PROTOCOL.md) | PhyCMD-64 protocol specification |
| [SETUP.md](SETUP.md) | Detailed setup instructions |
| [FIRMWARE_UPDATES.md](FIRMWARE_UPDATES.md) | Required firmware updates |
| [physerver/README.md](physerver/README.md) | Server API documentation |

## Features

### Current (Implemented)

- ✅ Real-time serial communication
- ✅ 64-byte fixed protocol
- ✅ Digital I/O (16 in + 16 out)
- ✅ 8-channel ADC
- ✅ 2-channel DAC
- ✅ REST API
- ✅ WebSocket streaming
- ✅ Shared memory IPC
- ✅ Web dashboard
- ✅ Example client library

### Planned (Firmware Updates)

- ⏳ CRC-16 error detection
- ⏳ Sequence numbering
- ⏳ PWM outputs (2 channels)
- ⏳ Loop time telemetry
- ⏳ Communications watchdog
- ⏳ Error counters

## Performance Targets

| Metric | Target | Status |
|--------|--------|--------|
| Update rate | 5 kHz | ⏳ Testing needed |
| Jitter (stddev) | < 10 µs | ⏳ Testing needed |
| Round-trip latency | < 500 µs | ⏳ Testing needed |
| ADC sampling rate | 5 kHz | ✅ Capable |
| GPIO toggle freq | 2.5 kHz | ✅ Capable |

## Use Cases

- **Test Machines**: Automated testing of mechanical/electrical devices
- **Data Acquisition**: High-speed sensor monitoring
- **Motion Control**: Precise control of actuators and motors
- **Laboratory Automation**: Remote control of experimental setups
- **Industrial IoT**: Edge device monitoring and control
- **Education**: Teaching real-time systems and embedded programming

## API Examples

### REST API

```bash
# Get status
curl http://localhost:8080/api/status

# Set GPIO pin 3 high
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 3, "value": true}'

# Set DAC to mid-scale
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 2047}'
```

### Rust Client (IPC)

```rust
use physerver::{IpcClient, Command, CommandFlags};

fn main() -> anyhow::Result<()> {
    let client = IpcClient::connect()?;

    // Set outputs
    let mut cmd = Command::default();
    cmd.digital_out = 0x00FF;  // Pins 0-7 high
    cmd.dac[0] = 2047;         // Mid-scale
    cmd.flags = CommandFlags {
        adc_enable: true,
        dac_enable: true,
        ..Default::default()
    };

    client.write_command(&cmd);

    // Read inputs
    let status = client.read_status();
    println!("ADC 0: {}", status.adc[0]);

    Ok(())
}
```

### WebSocket (JavaScript)

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onmessage = (event) => {
    const status = JSON.parse(event.data);
    console.log('ADC values:', status.adc);
};

// Send command
const cmd = {
    digital_out: 0x0001,  // Pin 0 high
    dac: [2047, 4095],
    pwm: [0, 0],
    flags: { adc_enable: true, dac_enable: true },
    seq_num: 0
};
ws.send(JSON.stringify(cmd));
```

## Development Status

### Completed ✅

1. **Architecture & Design**
   - System architecture document
   - Protocol specification
   - Setup instructions

2. **Physerver (Rust)**
   - Serial communication module
   - Protocol encoder/decoder with CRC
   - REST API server
   - WebSocket support
   - Shared memory IPC
   - Real-time scheduling
   - Web interface
   - Example client

3. **Documentation**
   - Comprehensive guides
   - API documentation
   - Code examples

### In Progress ⏳

1. **Firmware Updates**
   - Add CRC validation
   - Implement sequence numbering
   - Add PWM support
   - Telemetry (loop time, uptime, errors)
   - Communications watchdog

2. **Testing**
   - Hardware integration tests
   - Performance benchmarks
   - Stress testing

### Future 🔮

1. **Features**
   - Configuration file support (TOML)
   - Systemd service unit
   - Multiple device support
   - Data logging to file
   - Scripting interface (Lua/Python)

2. **Optimizations**
   - Zero-copy protocol parsing
   - SIMD optimizations
   - Custom allocator

## Contributing

This project is designed for embedded systems and real-time applications. Contributions are welcome!

## Hardware

### Recommended

- **Microcontroller**: ATSAM3X8E (Arduino Due)
- **Connection**: USB 2.0 (Full Speed)
- **Power**: USB powered or external 7-12V

### Pin Mapping

See firmware configuration files for detailed pin assignments:
- Digital I/O: Configured in `conf_board.h`
- ADC: Channels 0-7
- DAC: Channels 0-1 (DACC0, DACC1)
- PWM: TBD (TC0 channels)

## Troubleshooting

See [SETUP.md](SETUP.md) for detailed troubleshooting.

## License

MIT

---

**Status**: ✅ Core implementation complete, ready for hardware testing
**Version**: 1.0.0
**Last Updated**: 2025-11-22
