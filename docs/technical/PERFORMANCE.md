# Performance Comparison: USB vs Serial Transport

## Overview

PhyServer supports two transport modes for communicating with the Arduino Due:

1. **Serial (USB CDC)** - USB Communications Device Class (virtual serial port)
2. **USB (Direct Bulk)** - Direct USB bulk transfers via libusb

This document compares their performance characteristics.

## Transport Comparison

| Characteristic | Serial (USB CDC) | USB (Direct Bulk) |
|---------------|------------------|-------------------|
| **Technology** | USB CDC ACM | USB Bulk Transfer |
| **Driver** | Built-in OS driver | rusb (libusb) |
| **Latency** | 500-1000 µs | 50-200 µs |
| **Max Rate** | ~1 kHz | ~10 kHz |
| **Throughput** | ~900 kbps | ~8 Mbps |
| **Jitter** | ±50-100 µs | ±5-20 µs |
| **Setup** | Automatic | Requires libusb |
| **Compatibility** | Universal | Requires permissions |
| **CPU Usage** | Low | Very Low |

## Detailed Analysis

### Latency

**Round-trip latency** (send command + receive status):

```
Serial Transport:
├─ USB Stack:      200-400 µs
├─ CDC Processing: 100-200 µs
├─ Serial Driver:  100-200 µs
└─ Protocol:       50-100 µs
   Total:          500-1000 µs

USB Transport:
├─ USB Stack:      30-80 µs
├─ Bulk Transfer:  10-50 µs
├─ Protocol:       50-100 µs
└─ rusb Overhead:  10-20 µs
   Total:          100-250 µs
```

**Winner**: USB (4-10x faster)

### Maximum Update Rate

**Theoretical limits**:

```
Serial (USB CDC):
- 64 bytes/packet × 10 bits/byte = 640 bits
- USB polls at 1ms intervals
- Max: ~1.5 kHz theoretical, ~1 kHz practical

USB (Bulk):
- 64 bytes/packet
- USB Full Speed: 12 Mbps
- No 1ms polling limitation
- Max: ~20 kHz theoretical, ~10 kHz practical
```

**Winner**: USB (10x faster)

### Throughput

**Sustained data rate**:

```
Serial:
- Effective baud: ~921600 (ignored, actually USB speed)
- Real throughput: ~900 kbps
- Overhead from CDC layer: ~30%

USB:
- Direct bulk transfer
- Sustained: ~8 Mbps
- Overhead: ~10%
```

**Winner**: USB (9x faster)

### Jitter (Timing Variation)

Standard deviation of round-trip time:

```
Serial:
- Without RT scheduling: ±50-100 µs
- With RT scheduling:    ±20-50 µs

USB:
- Without RT scheduling: ±10-30 µs
- With RT scheduling:    ±5-10 µs
```

**Winner**: USB (5-10x more consistent)

## Benchmark Results

### Test Setup

> These measurements were captured on **PhyServer v1.0.0** (Nov 2025).
> The iso transport and on-chip fn-gen added in v2.0.0 (Apr 2026)
> change the transport characteristics significantly — see
> `/api/rt_stats` on a running v2.0.0 instance for up-to-date numbers.

- **Hardware**: Arduino Due (ATSAM3X8E @ 84 MHz)
- **Computer**: Intel Atom (various models tested)
- **OS**: Linux 5.x with real-time kernel patches
- **PhyServer**: v1.0.0 with RT optimizations enabled

### Serial Transport Results

```
Configuration: Serial, 1 kHz update rate, RT priority 80

Metric               | Value
---------------------|-------------
Update rate          | 995 Hz (actual)
Avg latency          | 750 µs
Std dev (jitter)     | ±35 µs
P99 latency          | 980 µs
Dropped packets      | 0.1%
CPU usage            | 12%
Max sustained rate   | 1050 Hz
```

### USB Transport Results

```
Configuration: USB, 5 kHz update rate, RT priority 80

Metric               | Value
---------------------|-------------
Update rate          | 4995 Hz (actual)
Avg latency          | 120 µs
Std dev (jitter)     | ±8 µs
P99 latency          | 180 µs
Dropped packets      | 0.01%
CPU usage            | 15%
Max sustained rate   | 9500 Hz
```

### Comparison at Same Rate (1 kHz)

To isolate latency differences:

```
@ 1 kHz update rate, RT enabled:

Metric          | Serial    | USB       | Improvement
----------------|-----------|-----------|------------
Avg latency     | 750 µs    | 115 µs    | 6.5x
Std dev         | ±35 µs    | ±7 µs     | 5x
P99 latency     | 980 µs    | 165 µs    | 5.9x
CPU usage       | 12%       | 8%        | 1.5x lower
```

## Use Case Recommendations

### Choose Serial Transport When:

✅ **Compatibility is critical**
- Works on any OS without drivers
- No elevated permissions needed
- Automatic device enumeration

✅ **Update rate < 1 kHz is sufficient**
- Motor control at moderate speeds
- Data logging applications
- Manual testing and development

✅ **Simple deployment needed**
- Plug-and-play operation
- No libusb installation
- Works in restricted environments

### Choose USB Transport When:

✅ **Performance is critical**
- High-speed control loops (>1 kHz)
- Precision timing requirements
- Low-latency feedback needed

✅ **Maximum throughput needed**
- Multiple simultaneous tests
- High-rate data acquisition
- Real-time system requirements

✅ **Deterministic timing required**
- Hard real-time applications
- Precise mechanical control
- Test equipment automation

## Performance Tuning

### For Serial Transport

```toml
# config.toml
[transport]
type = "serial"
update_rate = 1000  # Don't exceed 1000 Hz

[realtime]
enabled = true
priority = 80
```

### For USB Transport

```toml
# config.toml
[transport]
type = "usb"
update_rate = 5000  # Can go up to 10000 Hz

[realtime]
enabled = true
priority = 90       # Higher priority for USB
cpu_affinity = true
cpu_core = 2        # Isolated core
```

### Real-Time Optimizations

Both transports benefit from RT optimizations:

```bash
# Grant capabilities
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip ./physerver

# Isolate CPU core (add to kernel cmdline)
isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3

# Disable CPU frequency scaling
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Run with RT and CPU pinning
./physerver --config config.toml
```

**Improvement with RT optimizations**:
- Serial: 2-3x lower jitter
- USB: 5-10x lower jitter

## Bottleneck Analysis

### Serial Transport Bottlenecks

1. **USB polling**: Limited to 1ms intervals (1 kHz)
2. **CDC overhead**: Extra protocol layer
3. **Kernel buffering**: Data passes through TTY layer

### USB Transport Bottlenecks

1. **USB Full Speed**: 12 Mbps max (USB 2.0 limitation of Arduino Due)
2. **Packet processing**: ~30µs per packet on Atom CPU
3. **Context switches**: ~10µs per transfer

### System Bottlenecks (Both)

1. **CPU speed**: Intel Atom is slower than desktop CPUs
2. **Cache misses**: ~100ns per miss on Atom
3. **Kernel latency**: ~50µs without RT patches

## Power Consumption

### Idle

```
Serial: ~2.5W (Arduino + USB)
USB:    ~2.5W (same hardware)
```

### Active (1 kHz)

```
Serial: ~3.0W
USB:    ~3.0W
```

### Active (5 kHz, USB only)

```
USB: ~3.5W (15% higher due to increased activity)
```

**Conclusion**: USB transport is more power-efficient per bit transferred.

## Switching Between Transports

### Runtime Configuration

```bash
# Use USB (default for best performance)
./physerver --config config.toml

# Override to serial
./physerver --transport serial --port /dev/ttyACM0

# Auto-detect best transport
./physerver --auto-detect
```

### Configuration File

```toml
# config.toml
[transport]
type = "usb"  # or "serial"
auto_detect = true  # Falls back if preferred unavailable
```

### Programmatic Switching

```rust
use physerver::{Transport, TransportType};

// Create USB transport
let transport = create_transport(
    TransportType::Usb,
    "", // ignored for USB
    0   // ignored for USB
)?;

// Or serial
let transport = create_transport(
    TransportType::Serial,
    "/dev/ttyACM0",
    921600
)?;
```

## Compatibility Matrix

| OS | Serial | USB | Notes |
|----|--------|-----|-------|
| Linux | ✅ | ✅ | USB requires libusb, udev rules |
| macOS | ✅ | ✅ | USB may need permissions |
| Windows | ✅ | ⚠️ | USB requires libusb drivers (zadig) |
| RT Linux | ✅ | ✅ | Both benefit from RT patches |

## Conclusion

**For maximum performance**: Use USB transport with RT optimizations

```bash
# Best configuration for high performance
./physerver --transport usb --rt --rate 5000 --cpu-core 2
```

**Expected results**:
- 5 kHz update rate
- <120 µs average latency
- <10 µs jitter (std dev)
- <0.01% packet loss

**For maximum compatibility**: Use serial transport

```bash
# Works everywhere, plug-and-play
./physerver --transport serial --port /dev/ttyACM0 --rate 1000
```

**Expected results**:
- 1 kHz update rate
- <750 µs average latency
- <35 µs jitter (std dev)
- <0.1% packet loss

## Future Improvements

Potential optimizations for even better performance:

1. **USB 3.0** (requires different hardware)
   - 5 Gbps vs 12 Mbps
   - 125µs polling vs 1ms
   - Estimated 50 kHz possible

2. **Zero-copy transfers**
   - Direct DMA to userspace
   - Estimated 30% latency reduction

3. **Batch transfers**
   - Multiple packets per USB frame
   - Estimated 2x throughput

4. **Custom USB descriptor**
   - Optimized endpoint configuration
   - Estimated 20% latency reduction

## References

- USB 2.0 Specification: https://www.usb.org/document-library
- Arduino Due Datasheet: ATSAM3X8E (Atmel Doc 11057)
- Linux USB Documentation: https://www.kernel.org/doc/html/latest/driver-api/usb/
- Real-time Linux: https://wiki.linuxfoundation.org/realtime/
