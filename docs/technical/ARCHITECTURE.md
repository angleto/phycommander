# Physical Commander - System Architecture

## Overview

Physical Commander (PhyCMD) is a real-time hardware control system designed for test machines and precision mechanical device control. It provides hard real-time communication between high-level software and microcontroller hardware.

## System Components

### 1. phyextension (Microcontroller Firmware)
- **Platform**: ATSAM3X8E (Arduino Due)
- **Language**: C (using Atmel Software Framework)
- **Communication**: USB CDC (Virtual Serial Port)
- **Why C**:
  - Rust on ARM Cortex-M3 is possible but immature for ATSAM3X8E
  - Existing optimized C code with DMA and low-latency USB
  - Direct ASF integration for hardware peripherals
  - **Decision**: Keep C implementation, focus optimization efforts here

**Capabilities**:
- 16 Digital Inputs (16-bit)
- 16 Digital Outputs (16-bit)
- 8-channel ADC with DMA circular buffering
- 2-channel 12-bit DAC
- Fixed 64-byte protocol for deterministic timing
- USB CDC at high baud rates (460800+)

### 2. physerver (Server Service)
- **Platform**: Linux/macOS
- **Language**: **Rust** (migrating from C++)
- **Why Rust**:
  - Memory safety critical for real-time systems
  - Zero-cost abstractions for performance
  - Excellent async/await for concurrent I/O
  - Superior error handling
  - Native serialport and tokio libraries
  - Better maintainability than C++

**Responsibilities**:
- Serial port communication with phyextension
- Real-time scheduling and timing guarantees
- IPC mechanism for client applications
- Telemetry and monitoring
- Configuration management

**Architecture Layers**:
```
┌─────────────────────────────────────┐
│     physerver (Rust)                │
├─────────────────────────────────────┤
│  • Serial I/O Thread (RT Priority)  │
│  • IPC Server (Shared Memory)       │
│  • Web API Server (HTTP/WebSocket)  │
│  • Business Logic Interface         │
│  • Telemetry & Logging              │
└─────────────────────────────────────┘
```

### 3. phywebapp (Web Interface)
- **Platform**: Web Browser
- **Technology Stack**:
  - **Backend**: Integrated into physerver (Rust)
    - Axum or Actix-web for HTTP/REST
    - WebSocket for real-time updates
  - **Frontend**:
    - HTML5/CSS3/JavaScript (or TypeScript)
    - Option: htmx for simplicity, or React/Svelte for richer UI
    - WebSocket client for real-time data

**Features**:
- Monitor GPIO states in real-time
- Configure ADC/DAC parameters
- View telemetry and performance metrics
- Control test sequences
- System configuration

### 4. Test Business Logic Interface
- **Language**: Rust (primary), C/C++ bindings available
- **Interface Types**:
  - Shared Memory (low latency, local)
  - TCP/UDP sockets (network capable)
  - Rust API (native library)

## Communication Protocol

### Protocol Specification: PhyCMD-64

**Fixed Size**: 64 bytes (chosen for USB packet efficiency and cache alignment)

#### Host → Device (Command Message)
```
┌──────┬─────────┬─────────────────┬─────────────┬──────────┐
│ Byte │  Field  │   Type          │   Purpose   │  Range   │
├──────┼─────────┼─────────────────┼─────────────┼──────────┤
│ 0-1  │ Header  │ uint16_t        │ Magic/Sync  │ 0xAA55   │
│ 2-3  │ DigOut  │ uint16_t        │ GPIO Output │ 16 bits  │
│ 4-5  │ DAC0    │ uint16_t        │ DAC Ch0     │ 0-4095   │
│ 6-7  │ DAC1    │ uint16_t        │ DAC Ch1     │ 0-4095   │
│ 8-9  │ PWM0    │ uint16_t        │ PWM Ch0     │ 0-65535  │
│ 10-11│ PWM1    │ uint16_t        │ PWM Ch1     │ 0-65535  │
│ 12   │ Flags   │ uint8_t         │ Control     │ Bitfield │
│ 13   │ SeqNum  │ uint8_t         │ Sequence    │ 0-255    │
│ 14-15│ CRC     │ uint16_t        │ Checksum    │ CRC-16   │
│ 16-63│ Padding │ uint8_t[48]     │ Reserved    │ 0x00     │
└──────┴─────────┴─────────────────┴─────────────┴──────────┘
```

#### Device → Host (Status Message)
```
┌──────┬─────────┬─────────────────┬─────────────┬──────────┐
│ Byte │  Field  │   Type          │   Purpose   │  Range   │
├──────┼─────────┼─────────────────┼─────────────┼──────────┤
│ 0-1  │ Header  │ uint16_t        │ Magic/Sync  │ 0x55AA   │
│ 2-3  │ DigIn   │ uint16_t        │ GPIO Input  │ 16 bits  │
│ 4-5  │ DigOut  │ uint16_t        │ GPIO Echo   │ 16 bits  │
│ 6-21 │ ADC[8]  │ uint16_t[8]     │ ADC Chs 0-7 │ 0-4095   │
│ 22   │ Flags   │ uint8_t         │ Status      │ Bitfield │
│ 23   │ SeqNum  │ uint8_t         │ Sequence    │ 0-255    │
│ 24-25│ CRC     │ uint16_t        │ Checksum    │ CRC-16   │
│ 26-27│ Timing  │ uint16_t        │ Loop µs     │ 0-65535  │
│ 28-63│ Padding │ uint8_t[36]     │ Reserved    │ 0x00     │
└──────┴─────────┴─────────────────┴─────────────┴──────────┘
```

**Flags Bitfield**:
- Bit 0: ADC Enable
- Bit 1: DAC Enable
- Bit 2: PWM Enable
- Bit 3: Error Flag
- Bit 4-7: Reserved

**Timing Requirements**:
- Target: 5 kHz update rate (200 µs period)
- Serial baud: 921600 or 1000000 bps
  - 64 bytes × 10 bits/byte = 640 bits
  - At 1 Mbps: 640 µs per message (allows ~1.5 kHz bidirectional)
  - Solution: USB Full Speed (12 Mbps) supports this easily
- Jitter: < 10 µs (0.5% of 200 µs period)

## Real-Time Performance Strategy

### Microcontroller (phyextension)
1. **DMA for ADC**: Continuous circular buffering, no CPU overhead
2. **USB CDC optimization**: Minimize interrupt latency
3. **Fixed timing loop**: Main loop at exact intervals
4. **Priority configuration**: USB and timers at highest priority

### Server (physerver)
1. **Real-time thread**:
   - Linux: `SCHED_FIFO` with high priority
   - macOS: Thread time constraints API
2. **Memory locking**: `mlockall()` to prevent paging
3. **CPU affinity**: Pin to dedicated core
4. **DMA latency**: Set `/dev/cpu_dma_latency` to 0 on Linux
5. **Lock-free queues**: For inter-thread communication

### IPC Design
**Shared Memory Layout** (for low-latency local clients):
```rust
#[repr(C)]
struct PhyCmdSharedState {
    // Command from client → server → device
    command: AtomicU64,        // Sequence number + dirty flag
    digital_out: AtomicU16,
    dac: [AtomicU16; 2],
    pwm: [AtomicU16; 2],
    flags: AtomicU8,

    // Status from device → server → client
    status: AtomicU64,         // Sequence number + dirty flag
    digital_in: AtomicU16,
    adc: [AtomicU16; 8],
    error_flags: AtomicU8,
    timing_us: AtomicU16,
}
```

**Alternative: Unix Domain Sockets** (more flexible, slightly higher latency)

## Directory Structure (Proposed)

```
phycmd/
├── phyextension/           # Microcontroller firmware
│   ├── src/
│   │   ├── main.c
│   │   ├── protocol.h
│   │   ├── usb_handler.c
│   │   ├── adc_dma.c
│   │   └── gpio.c
│   ├── Makefile
│   └── platformio.ini      # Optional: PlatformIO support
│
├── physerver/              # Rust server
│   ├── src/
│   │   ├── main.rs
│   │   ├── serial/         # Serial communication
│   │   ├── protocol/       # Protocol encoding/decoding
│   │   ├── ipc/            # Shared memory IPC
│   │   ├── web/            # Web server (Axum)
│   │   ├── rt/             # Real-time scheduling
│   │   └── telemetry/      # Logging and metrics
│   ├── Cargo.toml
│   └── README.md
│
├── phywebapp/              # Web interface
│   ├── static/
│   │   ├── index.html
│   │   ├── app.js
│   │   └── style.css
│   └── README.md
│
├── phyclient-rs/           # Rust client library
│   ├── src/
│   │   └── lib.rs
│   ├── examples/
│   │   └── simple_test.rs
│   └── Cargo.toml
│
├── protocol/               # Protocol specification
│   ├── PROTOCOL.md
│   └── protocol.h          # C header for firmware
│
├── docs/
│   ├── ARCHITECTURE.md     # This file
│   ├── API.md
│   └── PERFORMANCE.md
│
└── tests/                  # Integration tests
    ├── rt_benchmark/
    └── hardware_tests/
```

## Technology Stack Summary

| Component     | Language | Key Libraries/Tools              |
|---------------|----------|----------------------------------|
| phyextension  | C        | ASF, ARM CMSIS                   |
| physerver     | Rust     | tokio, serialport, axum, nix     |
| phywebapp     | JS/HTML  | WebSocket API, htmx or React     |
| phyclient-rs  | Rust     | shared-memory, tokio             |
| Build         | -        | cargo, make, platformio (opt)    |

## Migration Strategy

### Phase 1: Protocol & Documentation ✓
- [x] Define protocol specification
- [x] Create architecture docs

### Phase 2: Physerver Core (Rust)
- [ ] Serial port communication module
- [ ] Protocol encoder/decoder
- [ ] Real-time scheduling setup
- [ ] Basic IPC (shared memory)

### Phase 3: Firmware Updates
- [ ] Update to new protocol with CRC and sequence numbers
- [ ] Optimize timing loop
- [ ] Add PWM support

### Phase 4: Web Interface
- [ ] REST API in physerver
- [ ] WebSocket for real-time updates
- [ ] Basic HTML/JS frontend
- [ ] Real-time dashboard

### Phase 5: Client Library & Examples
- [ ] Rust client library (phyclient-rs)
- [ ] Example test programs
- [ ] Performance benchmarks

### Phase 6: Production Hardening
- [ ] Comprehensive error handling
- [ ] Logging and diagnostics
- [ ] Configuration file support
- [ ] Systemd service
- [ ] Documentation

## Performance Targets

| Metric                    | Target      | Measured |
|---------------------------|-------------|----------|
| Update rate               | 5 kHz       | TBD      |
| Jitter (stddev)           | < 10 µs     | TBD      |
| Round-trip latency        | < 500 µs    | TBD      |
| ADC sampling rate         | 5 kHz       | TBD      |
| GPIO toggle freq (stable) | 2.5 kHz     | TBD      |
| CPU usage (physerver)     | < 50%       | TBD      |

## Design Decisions Log

1. **Keep C for firmware**: Rust on ATSAM3X8E is immature; existing C code is optimized
2. **Rust for physerver**: Memory safety + performance + maintainability
3. **64-byte fixed protocol**: Cache-aligned, USB-efficient, deterministic
4. **Shared memory IPC**: Lowest latency for local communication
5. **Integrated web server**: Simpler deployment than separate services
6. **CRC-16 for integrity**: Lightweight error detection
7. **Sequence numbers**: Detect dropped packets

## Open Questions

1. Do we need PWM outputs or can we use DAC + external circuits?
2. Should we support multiple devices simultaneously?
3. Network protocol for remote access (beyond web UI)?
4. Configuration: TOML file or database?

## References

- USB CDC: https://www.usb.org/document-library/class-definitions-communication-devices-12
- Real-time Linux: https://wiki.linuxfoundation.org/realtime/start
- Rust embedded: https://docs.rust-embedded.org/
