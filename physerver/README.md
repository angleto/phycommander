# PhyServer - Physical Commander Server

High-performance, real-time server for controlling ATSAM3X8E microcontroller hardware.

## Features

- **Real-time Communication**: Precise timing control for 5kHz+ update rates
- **Multiple Interfaces**: Serial port, IPC (shared memory), REST API, WebSocket
- **Web UI**: Built-in web interface for monitoring and control
- **Zero-copy IPC**: Lock-free shared memory for local clients
- **Cross-platform**: Linux and macOS support
- **Type-safe Protocol**: Rust implementation with CRC validation

## Quick Start

### Build

```bash
cargo build --release
```

### Run

Auto-detect device and start server:

```bash
sudo ./target/release/physerver --auto-detect --rt
```

Specify port manually:

```bash
./target/release/physerver --port /dev/ttyACM0 --baud 921600
```

### Web Interface

Once running, open your browser to:

```
http://localhost:8080
```

## Command-Line Options

```
Options:
  -p, --port <PORT>            Serial port device (e.g., /dev/ttyACM0)
  -b, --baud <BAUD>            Baud rate [default: 921600]
  -w, --web-port <WEB_PORT>    Web server port [default: 8080]
  -r, --rate <RATE>            Update rate in Hz [default: 1000]
      --rt                     Enable real-time scheduling
      --rt-priority <N>        Real-time priority (1-99) [default: 80]
      --no-ipc                 Disable IPC (shared memory)
      --no-web                 Disable web server
      --auto-detect            Auto-detect PhyCMD device
  -h, --help                   Print help
  -V, --version                Print version
```

## Real-Time Mode

For best performance, run with real-time scheduling:

```bash
# Grant capabilities (one-time setup)
sudo setcap cap_sys_nice,cap_ipc_lock=eip target/release/physerver

# Run with RT enabled
./target/release/physerver --auto-detect --rt --rate 5000
```

## REST API

### Get Status

```bash
curl http://localhost:8080/api/status
```

Returns JSON:
```json
{
  "digital_in": 0,
  "digital_out": 15,
  "adc": [2048, 1024, 0, 0, 0, 0, 0, 0],
  "flags": {...},
  "seq_num": 42,
  "loop_time_us": 150,
  "uptime_ms": 123456,
  "error_count": 0
}
```

### Set GPIO

```bash
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 3, "value": true}'
```

### Set DAC

```bash
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 2047}'
```

### Read ADC

```bash
curl http://localhost:8080/api/adc/read
```

## WebSocket API

Connect to `ws://localhost:8080/ws` for real-time bidirectional communication.

**Receive** (Status updates):
```json
{
  "digital_in": 0,
  "adc": [2048, ...],
  "loop_time_us": 150,
  ...
}
```

**Send** (Commands):
```json
{
  "digital_out": 15,
  "dac": [2047, 4095],
  "pwm": [32768, 65535],
  "flags": {...},
  "seq_num": 0
}
```

## IPC (Shared Memory)

For low-latency local communication, use the shared memory interface:

```rust
use physerver::ipc::IpcClient;
use physerver::protocol::Command;

let client = IpcClient::connect()?;

// Set outputs
let mut cmd = Command::default();
cmd.digital_out = 0x00FF;
cmd.dac[0] = 2047;
client.write_command(&cmd);

// Read inputs
let status = client.read_status();
println!("ADC 0: {}", status.adc[0]);
```

## Architecture

```
┌─────────────────────────────────────────┐
│           physerver                     │
├─────────────────────────────────────────┤
│  ┌───────────────────────────────────┐  │
│  │  Serial I/O Thread (RT Priority)  │  │
│  │  • Send commands at fixed rate    │  │
│  │  • Receive status                 │  │
│  │  • Protocol encode/decode         │  │
│  └───────────────────────────────────┘  │
│                  │                       │
│     ┌────────────┼────────────┐          │
│     ▼            ▼            ▼          │
│  ┌──────┐  ┌──────────┐  ┌────────┐     │
│  │ IPC  │  │   Web    │  │  REST  │     │
│  │ SHM  │  │ Socket   │  │  API   │     │
│  └──────┘  └──────────┘  └────────┘     │
└─────────────────────────────────────────┘
        │            │            │
        ▼            ▼            ▼
   Local Apps   WebSocket     HTTP
                 Clients     Clients
```

## Performance

Target metrics:
- Update rate: 5000 Hz (200 µs period)
- Jitter: < 10 µs (with RT mode)
- Round-trip latency: < 500 µs
- CPU usage: < 50% (single core)

## Logging

Set log level with `RUST_LOG` environment variable:

```bash
RUST_LOG=debug ./target/release/physerver --auto-detect
```

Levels: `error`, `warn`, `info`, `debug`, `trace`

## Troubleshooting

### Serial port permission denied

```bash
sudo usermod -a -G dialout $USER
# Log out and back in
```

### Real-time scheduling fails

```bash
# Check limits
ulimit -r

# Grant capabilities
sudo setcap cap_sys_nice=eip target/release/physerver
```

### Device not found

```bash
# List available ports
ls /dev/ttyACM* /dev/ttyUSB*

# Check dmesg for device
dmesg | grep tty
```

## Development

Run tests:

```bash
cargo test
```

Build with optimizations:

```bash
cargo build --release
```

Format code:

```bash
cargo fmt
```

Lint:

```bash
cargo clippy
```

## License

AGPL-3.0-or-later. See the top-level `LICENSE` and `LICENSE-AGPL-3.0`
at the repository root for the full text and the rationale behind
the choice.
