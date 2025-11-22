# PhyCMD-64 Protocol Specification v1.0

## Overview

PhyCMD-64 is a fixed-size, bidirectional serial protocol designed for hard real-time communication between a host computer and an ATSAM3X8E microcontroller. The protocol prioritizes deterministic timing, low latency, and reliable data transfer for precision hardware control applications.

## Design Principles

1. **Fixed Size**: All messages are exactly 64 bytes
   - Simplifies parsing and buffering
   - Deterministic processing time
   - Efficient for USB Full Speed endpoints (64-byte max packet)
   - Cache-line friendly (64 bytes = 1 cache line on many processors)

2. **Bidirectional**: Separate message formats for each direction
   - Host → Device: Command messages
   - Device → Host: Status messages

3. **Self-Describing**: Headers distinguish message direction
   - Command: `0xAA55`
   - Status: `0x55AA`

4. **Error Detection**: CRC-16-CCITT for data integrity

5. **Sequencing**: Detect dropped or reordered packets

## Message Format

### Common Fields

All messages share these conventions:
- **Byte Order**: Little-endian (LSB first)
- **Alignment**: Natural alignment for 16-bit values
- **Padding**: Unused bytes set to 0x00
- **Version**: Implied by header magic number

### Command Message (Host → Device)

Total: 64 bytes

```c
typedef struct __attribute__((packed)) {
    uint16_t header;        // Offset 0-1:   Magic 0xAA55
    uint16_t digital_out;   // Offset 2-3:   GPIO outputs (16 bits)
    uint16_t dac0;          // Offset 4-5:   DAC channel 0 (12-bit, 0-4095)
    uint16_t dac1;          // Offset 6-7:   DAC channel 1 (12-bit, 0-4095)
    uint16_t pwm0;          // Offset 8-9:   PWM channel 0 duty (0-65535)
    uint16_t pwm1;          // Offset 10-11: PWM channel 1 duty (0-65535)
    uint8_t  flags;         // Offset 12:    Control flags (see below)
    uint8_t  seq_num;       // Offset 13:    Sequence number (wraps at 255)
    uint16_t crc;           // Offset 14-15: CRC-16-CCITT of bytes 0-13
    uint8_t  reserved[48];  // Offset 16-63: Reserved for future use
} phycmd_command_t;
```

#### Field Descriptions

**header** (offset 0-1):
- Value: `0xAA55` (fixed)
- Purpose: Message type identification and synchronization

**digital_out** (offset 2-3):
- Bits 0-15: Output state for GPIO pins 0-15
- 1 = High, 0 = Low
- Mapping: Bit 0 → GPIO0, Bit 1 → GPIO1, ..., Bit 15 → GPIO15

**dac0, dac1** (offset 4-7):
- Range: 0-4095 (12-bit DAC resolution)
- Values > 4095 are clamped to 4095
- Output voltage: `(value / 4095) × Vref`
- Vref typically 3.3V → 0.806 mV resolution

**pwm0, pwm1** (offset 8-11):
- Range: 0-65535 (16-bit duty cycle)
- Duty cycle: `(value / 65535) × 100%`
- Frequency: Configured separately (TBD, likely 1-100 kHz)

**flags** (offset 12):
- Bit 0: `ADC_ENABLE` - Enable ADC sampling
- Bit 1: `DAC_ENABLE` - Enable DAC outputs
- Bit 2: `PWM_ENABLE` - Enable PWM outputs
- Bit 3: `RESET_SEQ` - Reset sequence number to 0
- Bit 4: `WATCHDOG_DISABLE` - Disable communications watchdog
- Bit 5-7: Reserved (must be 0)

**seq_num** (offset 13):
- Increments with each command sent
- Wraps from 255 to 0
- Used to detect dropped or duplicate packets
- Special value: 0xFF can indicate "don't care" mode

**crc** (offset 14-15):
- CRC-16-CCITT (polynomial 0x1021, init 0xFFFF)
- Computed over bytes 0-13 (header through seq_num)
- Invalid CRC causes message to be discarded

**reserved** (offset 16-63):
- 48 bytes reserved for future protocol extensions
- Must be set to 0x00 by sender
- Receiver should ignore these bytes

### Status Message (Device → Host)

Total: 64 bytes

```c
typedef struct __attribute__((packed)) {
    uint16_t header;        // Offset 0-1:   Magic 0x55AA
    uint16_t digital_in;    // Offset 2-3:   GPIO inputs (16 bits)
    uint16_t digital_out;   // Offset 4-5:   GPIO outputs echo
    uint16_t adc[8];        // Offset 6-21:  ADC channels 0-7 (12-bit)
    uint8_t  status_flags;  // Offset 22:    Status flags (see below)
    uint8_t  seq_num;       // Offset 23:    Sequence number echo
    uint16_t crc;           // Offset 24-25: CRC-16-CCITT of bytes 0-23
    uint16_t loop_time_us;  // Offset 26-27: Main loop iteration time (µs)
    uint32_t uptime_ms;     // Offset 28-31: System uptime (milliseconds)
    uint16_t error_count;   // Offset 32-33: Total error count
    uint8_t  reserved[30];  // Offset 34-63: Reserved
} phycmd_status_t;
```

#### Field Descriptions

**header** (offset 0-1):
- Value: `0x55AA` (fixed)
- Purpose: Message type identification

**digital_in** (offset 2-3):
- Bits 0-15: Input state for GPIO pins 0-15
- 1 = High, 0 = Low
- Sampled at the time of message construction

**digital_out** (offset 4-5):
- Echo of current output state
- Allows host to verify commands were applied
- May differ from commanded value during transients

**adc[8]** (offset 6-21):
- 8 channels, each 16-bit (but only 12 bits used)
- Range: 0-4095 (12-bit ADC resolution)
- Voltage: `(value / 4095) × Vref`
- Sampling: Latest value from DMA circular buffer
- Channels not in use return 0

**status_flags** (offset 22):
- Bit 0: `ADC_ACTIVE` - ADC is currently enabled
- Bit 1: `DAC_ACTIVE` - DAC is currently enabled
- Bit 2: `PWM_ACTIVE` - PWM is currently enabled
- Bit 3: `ERROR_FLAG` - Error condition (see error_count)
- Bit 4: `WATCHDOG_TRIGGERED` - Communications watchdog triggered
- Bit 5: `USB_CONFIGURED` - USB enumeration complete
- Bit 6: `OVERRUN` - Data buffer overrun occurred
- Bit 7: Reserved

**seq_num** (offset 23):
- Echo of the most recently received command sequence number
- Special value 0xFF if no command received yet
- Allows host to verify round-trip communication

**crc** (offset 24-25):
- CRC-16-CCITT (polynomial 0x1021, init 0xFFFF)
- Computed over bytes 0-23 (header through seq_num)

**loop_time_us** (offset 26-27):
- Time taken for last main loop iteration in microseconds
- Used for performance monitoring
- Values > 10000 µs may indicate problems

**uptime_ms** (offset 28-31):
- Milliseconds since device boot/reset
- Wraps after ~49.7 days
- Useful for synchronization and diagnostics

**error_count** (offset 32-33):
- Total number of errors since boot
- Includes: CRC errors, framing errors, buffer overruns
- Does not reset; wraps at 65535

**reserved** (offset 34-63):
- 30 bytes reserved for future use
- Set to 0x00 by device

## CRC-16-CCITT Calculation

**Parameters**:
- Polynomial: 0x1021 (x^16 + x^12 + x^5 + 1)
- Initial value: 0xFFFF
- Final XOR: 0x0000 (no XOR out)
- Bit order: MSB first

**C Implementation**:
```c
uint16_t crc16_ccitt(const uint8_t *data, size_t length) {
    uint16_t crc = 0xFFFF;
    for (size_t i = 0; i < length; i++) {
        crc ^= (uint16_t)data[i] << 8;
        for (uint8_t j = 0; j < 8; j++) {
            if (crc & 0x8000) {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    return crc;
}
```

**Usage**:
```c
phycmd_command_t cmd;
// Fill fields...
cmd.crc = crc16_ccitt((uint8_t*)&cmd, 14); // Bytes 0-13
```

## Communication Flow

### Normal Operation

```
Host                                Device
  |                                    |
  |-- Command (seq=0) --------------->|
  |                                    | Process command
  |<-------------- Status (seq=0) ----|
  |                                    |
  |-- Command (seq=1) --------------->|
  |                                    | Process command
  |<-------------- Status (seq=1) ----|
  |                                    |
  ... (continues at fixed rate) ...
```

### Error Handling

**CRC Error**:
```
Host                                Device
  |                                    |
  |-- Command (BAD CRC) ------------->|
  |                                    | Discard, increment error_count
  |<-------------- Status (seq=old) --| (seq_num unchanged)
  |                                    |
  |-- Command (seq=retransmit) ------>| (Host detects by seq mismatch)
  |<-------------- Status (seq=new) --|
```

**Sequence Number Mismatch**:
- Host sends seq N
- Status returns seq M where M ≠ N
- Possible causes:
  - Packet loss
  - Host/device out of sync
- Recovery: Host can reset sequence with `RESET_SEQ` flag

**Communications Watchdog**:
- If enabled, device expects commands at regular intervals
- If no valid command received for >100ms, device:
  - Sets `WATCHDOG_TRIGGERED` flag
  - Optional: Enters safe state (all outputs off)
- Cleared when valid command received

## Timing Requirements

### Target Performance

- **Update Rate**: 5 kHz (200 µs period)
- **Jitter**: < 10 µs standard deviation
- **Round-trip Latency**: < 500 µs

### USB Serial Configuration

**Recommended Baud Rates**:
- 921600 bps (standard)
- 1000000 bps (1 Mbps, may require custom settings)
- 2000000 bps (2 Mbps, optimal for 5 kHz)

**USB CDC Considerations**:
- Actual baud rate ignored by USB CDC (USB uses 12 Mbps Full Speed)
- Baud rate setting mainly for serial driver compatibility
- USB Full Speed provides ~1.2 MB/s sustained throughput
- 64-byte messages at 5 kHz = 320 KB/s (well within limits)

**Packet Timing**:
- USB polls at 1 kHz (1ms frames)
- Multiple packets can be sent per frame
- Use libusb or native drivers for best latency

## Protocol Extensions (Future)

Reserved bytes allow for backward-compatible extensions:

### Potential Additions
1. **Variable frequency PWM** (bytes 16-19)
2. **Encoder inputs** (bytes 20-27)
3. **I2C/SPI bridge commands** (bytes 28-43)
4. **Timestamp synchronization** (bytes 44-51)
5. **Multi-device addressing** (byte 52)

### Version Negotiation (Future)
- Could use byte 16 for version number
- Device reports supported version in byte 34 of status
- Backward compatibility: v1.0 devices ignore reserved bytes

## Security Considerations

1. **Physical Access**: Assumes host has trusted physical access to device
2. **No Authentication**: Protocol has no authentication mechanism
3. **USB Enumeration**: Standard USB CDC security model
4. **Buffer Overflows**: Fixed-size messages prevent most overflow attacks
5. **Malformed Packets**: CRC validation rejects corrupted data

**Threat Model**: Designed for lab/industrial use, not hostile environments.

## Compliance

- USB 2.0 CDC ACM specification
- No specific safety certifications (depends on application)
- Should comply with:
  - CE for EMC (if properly designed PCB)
  - FCC Part 15 Class B (digital device)

## Test Vectors

### Command Message Example

```
Header:        0xAA 0x55
Digital Out:   0x0F 0x00  (GPIO 0-3 high, rest low)
DAC0:          0xFF 0x07  (2047, mid-scale)
DAC1:          0xFF 0x0F  (4095, full-scale)
PWM0:          0x00 0x80  (32768, 50% duty)
PWM1:          0xFF 0xFF  (65535, 100% duty)
Flags:         0x07        (ADC, DAC, PWM enabled)
Seq:           0x2A        (42)
CRC:           [calculated]
Reserved:      [48 bytes of 0x00]
```

### Status Message Example

```
Header:        0x55 0xAA
Digital In:    0xFF 0x00  (GPIO 0-7 high)
Digital Out:   0x0F 0x00  (echo)
ADC[0]:        0x00 0x08  (2048)
ADC[1-7]:      [similar]
Status:        0x67        (ADC/DAC/PWM active, USB configured)
Seq:           0x2A        (echo)
CRC:           [calculated]
Loop Time:     0xC8 0x00  (200 µs)
Uptime:        0x10 0x27 0x00 0x00  (10000 ms)
Error Count:   0x00 0x00
Reserved:      [30 bytes of 0x00]
```

## Changelog

- **v1.0 (2025-11-22)**: Initial specification
  - 64-byte fixed format
  - Bidirectional command/status messages
  - CRC-16 integrity checking
  - Sequence numbering
  - GPIO, ADC, DAC, PWM support

## References

1. USB CDC ACM: USB Class Definitions for Communication Devices v1.2
2. CRC-16-CCITT: ITU-T Recommendation V.41
3. ATSAM3X8E Datasheet: Atmel Doc 11057
