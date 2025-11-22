# PhyCMD Configuration Guide

Complete configuration reference for PhyServer.

## Table of Contents

1. [Configuration Overview](#configuration-overview)
2. [Transport Configuration](#transport-configuration)
3. [Web Server Configuration](#web-server-configuration)
4. [IPC Configuration](#ipc-configuration)
5. [Real-Time Configuration](#real-time-configuration)
6. [Command-Line Overrides](#command-line-overrides)
7. [Environment Variables](#environment-variables)
8. [Configuration Examples](#configuration-examples)

---

## Configuration Overview

PhyServer can be configured via:
1. **TOML configuration file** (recommended for production)
2. **Command-line arguments** (overrides config file)
3. **Environment variables** (logging only)

### Configuration File Location

```bash
# Specify config file
./physerver --config /path/to/config.toml

# Default search paths (if no --config specified)
./config.toml
./physerver.toml
~/.config/physerver/config.toml
/etc/physerver/config.toml
```

### Creating Configuration File

```bash
# Copy example configuration
cp physerver/config.example.toml config.toml

# Edit configuration
nano config.toml
```

---

## Transport Configuration

Controls communication with the Arduino Due.

### Complete Transport Section

```toml
[transport]
# Transport type: "usb" or "serial"
type = "usb"

# Serial port path (used when type = "serial")
serial_port = "/dev/ttyACM0"

# Serial baud rate (used when type = "serial")
baud_rate = 921600

# Update rate in Hz
# USB: recommended 1000-10000 Hz
# Serial: recommended 100-1000 Hz
update_rate = 5000

# Auto-detect device (overrides type if true)
auto_detect = true
```

### Transport Type

**USB Bulk** (`type = "usb"`):
```toml
[transport]
type = "usb"
update_rate = 5000
```

**Advantages**:
- 10x lower latency (120 µs vs 750 µs)
- Higher update rates (up to 10 kHz)
- Better jitter (±7 µs vs ±35 µs)

**Requirements**:
- Linux: libudev-dev installed
- Build with `--features usb` (default)
- May require udev rules or elevated permissions

**USB CDC Serial** (`type = "serial"`):
```toml
[transport]
type = "serial"
serial_port = "/dev/ttyACM0"
baud_rate = 921600
update_rate = 1000
```

**Advantages**:
- Universal compatibility
- Plug-and-play
- No special permissions
- Works on all platforms

### Auto-Detection

```toml
[transport]
auto_detect = true
```

When `auto_detect = true`:
1. Tries USB bulk transport first
2. Falls back to serial if USB not available
3. Auto-detects serial port if using serial

**Recommended**: Enable for development, disable for production

### Update Rate

```toml
[transport]
update_rate = 5000  # Hz
```

**Recommended rates**:
- **USB bulk**: 1000-10000 Hz (sustainable: 5000 Hz)
- **Serial**: 100-1000 Hz (sustainable: 800 Hz)

**Considerations**:
- Higher rates = lower latency, higher CPU usage
- Consider your application's requirements
- With real-time scheduling, higher rates are more stable

### Serial Port

```toml
[transport]
serial_port = "/dev/ttyACM0"
```

**Linux**:
- Usually: `/dev/ttyACM0`, `/dev/ttyACM1`, etc.
- Check with: `ls /dev/ttyACM*`
- Permissions: User must be in `dialout` group

**macOS**:
- Usually: `/dev/cu.usbmodem*`
- Check with: `ls /dev/cu.usbmodem*`

**Windows**:
- Usually: `COM3`, `COM4`, etc.
- Check in Device Manager

### Baud Rate

```toml
[transport]
baud_rate = 921600
```

**Recommended**: 921600 (ignored for USB bulk, applies to serial only)

**Note**: For USB CDC, the baud rate is mostly ignored as USB handles its own speed negotiation, but it's required for the serial port API.

---

## Web Server Configuration

Controls the HTTP/WebSocket server.

### Complete Web Section

```toml
[web]
# Bind address
# "0.0.0.0" = all interfaces (accessible remotely)
# "127.0.0.1" = localhost only (local access only)
host = "0.0.0.0"

# HTTP port
port = 8080
```

### Host Binding

**All Interfaces** (allow remote access):
```toml
[web]
host = "0.0.0.0"
```

Access from:
- Local: http://localhost:8080
- Remote: http://192.168.1.100:8080 (replace with actual IP)

**Localhost Only** (security):
```toml
[web]
host = "127.0.0.1"
```

Access from:
- Local: http://localhost:8080
- Remote: Not accessible (use SSH tunnel)

### Port Configuration

```toml
[web]
port = 8080
```

**Common ports**:
- `8080`: Default HTTP alternate
- `3000`: Common development port
- `80`: Standard HTTP (requires root/capabilities)
- `443`: HTTPS (requires reverse proxy)

**Using privileged ports** (<1024):
```bash
# Grant capability
sudo setcap cap_net_bind_service=+ep target/release/physerver

# Run on port 80
./physerver --web-port 80
```

---

## IPC Configuration

Controls shared memory IPC interface.

### Complete IPC Section

```toml
[ipc]
# Enable IPC server
enabled = true

# Shared memory name
shm_name = "/physerver"
```

### Enabling/Disabling IPC

**Enable IPC** (Rust clients can connect):
```toml
[ipc]
enabled = true
```

**Disable IPC** (web-only mode):
```toml
[ipc]
enabled = false
```

Or via command-line:
```bash
./physerver --no-ipc
```

### Shared Memory Name

```toml
[ipc]
shm_name = "/physerver"
```

**Multiple instances**:
```toml
# Instance 1
[ipc]
shm_name = "/physerver1"

# Instance 2
[ipc]
shm_name = "/physerver2"
```

Client must match:
```rust
let client = IpcClient::connect_named("/physerver1")?;
```

---

## Real-Time Configuration

Controls real-time scheduling and performance optimizations.

> **Note**: For comprehensive OS-level real-time configuration (RT kernel installation, CPU isolation, system tuning), see [REALTIME_OS_CONFIG.md](REALTIME_OS_CONFIG.md). This section covers PhyServer-specific RT settings.

### Complete Real-Time Section

```toml
[realtime]
# Enable real-time scheduling
enabled = false

# Real-time priority (1-99, higher = more priority)
# Typical: 80-90 for physerver
priority = 80

# Pin to specific CPU core
cpu_affinity = false

# CPU core number (if cpu_affinity = true)
cpu_core = 2
```

### Enabling Real-Time

```toml
[realtime]
enabled = true
priority = 90
```

**Requirements**:
```bash
# Grant capabilities
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip target/release/physerver

# Verify
getcap target/release/physerver
```

**Without capabilities**, RT will fail gracefully with a warning.

### Priority

```toml
[realtime]
priority = 90  # 1-99
```

**Guidelines**:
- `1-30`: Low priority RT
- `31-60`: Medium priority RT
- `61-90`: High priority RT
- `91-99`: Critical (kernel threads)

**Recommended**:
- USB transport: `80-90`
- Serial transport: `70-80`

**Too high**: May interfere with kernel threads
**Too low**: May not achieve desired latency

### CPU Affinity

```toml
[realtime]
cpu_affinity = true
cpu_core = 2
```

**Benefits**:
- Reduces cache misses
- Eliminates migration overhead
- More consistent performance

**Requirements**:

> **See Also**: For complete CPU isolation setup including RT kernel installation and system tuning, refer to [REALTIME_OS_CONFIG.md](REALTIME_OS_CONFIG.md).

1. **Isolate CPU core** in kernel parameters (`/etc/default/grub`):
   ```
   GRUB_CMDLINE_LINUX="isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3"
   ```

2. **Update grub and reboot**:
   ```bash
   sudo update-grub
   sudo reboot
   ```

3. **Verify isolation**:
   ```bash
   cat /proc/cmdline | grep isolcpus
   ```

**Without isolation**, affinity is less effective.

### Example: Maximum Performance

```toml
[realtime]
enabled = true
priority = 90
cpu_affinity = true
cpu_core = 2
```

**Expected results**:
- USB: 5-10 kHz sustained, <10 µs jitter
- Serial: 800-1000 Hz sustained, <35 µs jitter

---

## Command-Line Overrides

All configuration options can be overridden via command-line.

### Override Examples

```bash
# Override transport type
./physerver --config config.toml --transport usb

# Override update rate
./physerver --config config.toml --rate 10000

# Override web port
./physerver --config config.toml --web-port 9090

# Override multiple options
./physerver --config config.toml \
  --transport usb \
  --rate 5000 \
  --web-port 8080 \
  --rt \
  --rt-priority 90

# Disable IPC
./physerver --config config.toml --no-ipc

# Disable web server
./physerver --config config.toml --no-web
```

### Complete Command-Line Reference

```bash
./physerver [OPTIONS]

OPTIONS:
    -c, --config <FILE>          Configuration file path
    -t, --transport <TYPE>       Transport type ("usb" or "serial")
    -p, --port <PATH>            Serial port path
    -b, --baud <RATE>            Baud rate
    -w, --web-port <PORT>        Web server port
    -r, --rate <HZ>              Update rate in Hz
        --rt                     Enable real-time scheduling
        --rt-priority <N>        Real-time priority (1-99)
        --no-ipc                 Disable IPC
        --no-web                 Disable web server
        --auto-detect            Auto-detect device
    -h, --help                   Print help
    -V, --version                Print version
```

### Priority of Configuration

Highest to lowest priority:
1. Command-line arguments
2. Configuration file (--config)
3. Default values

Example:
```toml
# config.toml
[transport]
type = "serial"
update_rate = 1000
```

```bash
# Command overrides type and rate
./physerver --config config.toml --transport usb --rate 5000
# Result: USB transport at 5000 Hz
```

---

## Environment Variables

### Logging Configuration

```bash
# Set log level
export RUST_LOG=info        # Default
export RUST_LOG=debug       # Detailed
export RUST_LOG=trace       # Very detailed
export RUST_LOG=error       # Errors only

# Module-specific logging
export RUST_LOG=physerver::transport=trace,physerver::web=debug

# Then run
./physerver --auto-detect
```

**Log Levels**:
- `error`: Errors only
- `warn`: Warnings and errors
- `info`: Normal operation (default)
- `debug`: Detailed debug info
- `trace`: Very detailed tracing

**Module Filters**:
```bash
# Trace transport, debug everything else
RUST_LOG=physerver::transport=trace,debug ./physerver

# Trace USB, info for everything else
RUST_LOG=physerver::transport::usb=trace,info ./physerver
```

### Log Format

```bash
# JSON format (for log aggregation)
export RUST_LOG_FORMAT=json

# Pretty format (default)
export RUST_LOG_FORMAT=pretty
```

---

## Configuration Examples

### Example 1: Development (USB, Auto-detect)

```toml
[transport]
type = "usb"
update_rate = 1000
auto_detect = true

[web]
host = "127.0.0.1"
port = 8080

[ipc]
enabled = true
shm_name = "/physerver"

[realtime]
enabled = false
```

**Usage**:
```bash
./physerver --config config.toml
```

---

### Example 2: Production (USB, High Performance)

```toml
[transport]
type = "usb"
update_rate = 5000
auto_detect = false

[web]
host = "0.0.0.0"
port = 8080

[ipc]
enabled = true
shm_name = "/physerver"

[realtime]
enabled = true
priority = 90
cpu_affinity = true
cpu_core = 2
```

**Usage**:
```bash
# Grant capabilities first
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip target/release/physerver

# Run
./physerver --config config.toml
```

---

### Example 3: Maximum Compatibility (Serial)

```toml
[transport]
type = "serial"
serial_port = "/dev/ttyACM0"
baud_rate = 921600
update_rate = 1000
auto_detect = false

[web]
host = "0.0.0.0"
port = 8080

[ipc]
enabled = true
shm_name = "/physerver"

[realtime]
enabled = false
```

**Usage**:
```bash
./physerver --config config.toml
```

---

### Example 4: Web-Only (No IPC)

```toml
[transport]
type = "usb"
update_rate = 1000
auto_detect = true

[web]
host = "0.0.0.0"
port = 8080

[ipc]
enabled = false

[realtime]
enabled = false
```

**Usage**:
```bash
./physerver --config config.toml
```

---

### Example 5: Multiple Instances

**Instance 1** (config1.toml):
```toml
[transport]
type = "serial"
serial_port = "/dev/ttyACM0"
update_rate = 1000

[web]
port = 8080

[ipc]
shm_name = "/physerver1"
```

**Instance 2** (config2.toml):
```toml
[transport]
type = "serial"
serial_port = "/dev/ttyACM1"
update_rate = 1000

[web]
port = 8081

[ipc]
shm_name = "/physerver2"
```

**Usage**:
```bash
# Terminal 1
./physerver --config config1.toml

# Terminal 2
./physerver --config config2.toml
```

---

## Configuration Validation

PhyServer validates configuration on startup and reports errors:

```bash
$ ./physerver --config config.toml
ERROR Invalid transport type: "usbb". Use 'usb' or 'serial'
```

Common validation errors:
- Invalid transport type
- Invalid baud rate
- Invalid port number
- Invalid RT priority (must be 1-99)
- Invalid CPU core number

---

## Best Practices

1. **Use configuration files** for production
2. **Use command-line overrides** for testing/development
3. **Enable auto-detect** during development
4. **Disable auto-detect** in production
5. **Use USB transport** for performance
6. **Use serial transport** for compatibility
7. **Enable real-time** for high update rates
8. **Pin to isolated core** for best RT performance
9. **Bind to 127.0.0.1** if security is critical
10. **Keep update rate reasonable** for your CPU

---

## See Also

- [USER_MANUAL.md](USER_MANUAL.md) - Complete user guide
- [DEPLOYMENT.md](DEPLOYMENT.md) - Production deployment
- [PERFORMANCE.md](PERFORMANCE.md) - Performance tuning
- [BUILDING.md](BUILDING.md) - Build instructions

---

**End of Configuration Guide**
