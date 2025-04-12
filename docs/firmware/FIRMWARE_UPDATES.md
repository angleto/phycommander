# PhyExtension Firmware Updates Required

## Current State

The existing ATSAM3X8E firmware (`ATSAM3X8E_FW/ATSAM3X8E_FW/src/main.c`) implements a basic 64-byte protocol with:

- ✅ 64-byte fixed message size
- ✅ 16 digital inputs (bytes 0-1 in output)
- ✅ 16 digital outputs (bytes 2-3 in input)
- ✅ 2 DAC outputs (bytes 4-7 in input)
- ✅ 8 ADC channels (bytes 4-19 in output)
- ✅ DMA circular buffering for ADC
- ✅ USB CDC communication

## Required Updates for New Protocol

To match the PhyCMD-64 protocol specification in [PROTOCOL.md](../technical/PROTOCOL.md), the firmware needs:

### 1. Protocol Headers
- **Command messages**: Add 0xAA55 header check (bytes 0-1)
- **Status messages**: Add 0x55AA header (bytes 0-1)
- Validates message type and helps with synchronization

### 2. CRC-16-CCITT Validation
- Compute CRC over bytes 0-13 of received command
- Validate CRC matches byte 14-15
- Discard invalid messages
- Increment error counter

### 3. Sequence Numbering
- Echo sequence number from command (byte 13)
- Allows host to detect dropped/reordered packets
- Initial value 0xFF if no command received

### 4. Flags Implementation

**Command flags (byte 12 input)**:
- Bit 0: ADC_ENABLE - Enable/disable ADC sampling
- Bit 1: DAC_ENABLE - Enable/disable DAC outputs
- Bit 2: PWM_ENABLE - Enable/disable PWM outputs
- Bit 3: RESET_SEQ - Reset sequence counter
- Bit 4: WATCHDOG_DISABLE - Disable communications watchdog

**Status flags (byte 22 output)**:
- Bit 0: ADC_ACTIVE - ADC currently enabled
- Bit 1: DAC_ACTIVE - DAC currently enabled
- Bit 2: PWM_ACTIVE - PWM currently enabled
- Bit 3: ERROR_FLAG - Error condition present
- Bit 4: WATCHDOG_TRIGGERED - Watchdog timeout occurred
- Bit 5: USB_CONFIGURED - USB enumeration complete
- Bit 6: OVERRUN - Data buffer overrun

### 5. PWM Support
- Implement 2 PWM channels (bytes 8-11 in command)
- 16-bit duty cycle (0-65535 = 0-100%)
- Configurable frequency (recommend 1-100 kHz)
- Use TC (Timer Counter) peripherals

### 6. Timing Information

**Loop time measurement (bytes 26-27 output)**:
- Measure main loop iteration time in microseconds
- Use SysTick or DWT CYCCNT counter
- Update every cycle

**Uptime counter (bytes 28-31 output)**:
- Milliseconds since boot
- Use SysTick interrupt
- 32-bit wraps at ~49 days

**Error counter (bytes 32-33 output)**:
- Increment on:
  - CRC errors
  - Invalid headers
  - Buffer overruns
  - Watchdog timeouts
- Persistent, wraps at 65535

### 7. Communications Watchdog
- Optional timeout detection (100ms default)
- Enters safe state if no valid command received
- Safe state: all outputs off, error flag set
- Cleared when valid command arrives

### 8. Optimized Message Layout

**Updated output packet structure** (device → host):
```c
typedef struct {
    uint16_t header;           // 0x55AA
    uint16_t digital_in;       // GPIO inputs
    uint16_t digital_out;      // GPIO outputs echo
    uint16_t adc[8];           // ADC channels 0-7
    uint8_t  status_flags;     // Status bits
    uint8_t  seq_num;          // Sequence echo
    uint16_t crc;              // CRC-16
    uint16_t loop_time_us;     // Loop iteration time
    uint32_t uptime_ms;        // System uptime
    uint16_t error_count;      // Total errors
    uint8_t  reserved[30];     // Future use
} __attribute__((packed)) status_msg_t;
```

**Updated input packet structure** (host → device):
```c
typedef struct {
    uint16_t header;           // 0xAA55
    uint16_t digital_out;      // GPIO outputs
    uint16_t dac0;             // DAC channel 0
    uint16_t dac1;             // DAC channel 1
    uint16_t pwm0;             // PWM channel 0
    uint16_t pwm1;             // PWM channel 1
    uint8_t  flags;            // Control flags
    uint8_t  seq_num;          // Sequence number
    uint16_t crc;              // CRC-16
    uint8_t  reserved[48];     // Future use
} __attribute__((packed)) command_msg_t;
```

## Implementation Notes

### CRC-16-CCITT Function
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

### Timing Measurement
```c
// Initialize DWT cycle counter
CoreDebug->DEMCR |= CoreDebug_DEMCR_TRCENA_Msk;
DWT->CYCCNT = 0;
DWT->CTRL |= DWT_CTRL_CYCCNTENA_Msk;

// Measure loop time
uint32_t start_cycles = DWT->CYCCNT;
// ... do work ...
uint32_t cycles = DWT->CYCCNT - start_cycles;
uint16_t time_us = cycles / (SystemCoreClock / 1000000);
```

### PWM Configuration
```c
// Use TC0 Channel 0 for PWM0, TC0 Channel 1 for PWM1
pmc_enable_periph_clk(ID_TC0);
tc_init(TC0, 0, TC_CMR_TCCLKS_TIMER_CLOCK1 | TC_CMR_WAVE | TC_CMR_WAVSEL_UP_RC);
tc_write_rc(TC0, 0, PWM_PERIOD); // Set frequency
tc_write_ra(TC0, 0, duty_cycle); // Set duty cycle
tc_start(TC0, 0);
```

## Backward Compatibility

The current firmware uses:
- Bytes 0-1: Reserved (can become header without breaking)
- Bytes 2-3: Digital outputs (unchanged position)
- Bytes 4-7: DAC values (unchanged position)

To maintain compatibility during transition:
1. Physerver can auto-detect old vs new protocol by checking headers
2. Old firmware gets 0x0000 in header bytes (reserved)
3. New firmware gets 0xAA55 in header bytes
4. Physerver adapts behavior based on detected version

## Testing Strategy

1. **Protocol validation**: Test CRC, headers, sequence numbers
2. **Timing verification**: Measure loop time < 200 µs at 5 kHz
3. **Stress testing**: Continuous operation for 24+ hours
4. **Error injection**: Corrupt messages, check error handling
5. **Performance**: Verify 5 kHz sustained rate with jitter < 10 µs

## Development Approach

### Phase 1: Add Protocol Infrastructure
- Add CRC function
- Implement header checking
- Add sequence numbering
- Basic flags support

### Phase 2: Telemetry
- Loop time measurement
- Uptime counter
- Error counter

### Phase 3: PWM
- Initialize Timer/Counter peripherals
- Implement duty cycle control
- Test frequency stability

### Phase 4: Watchdog & Polish
- Communications watchdog
- Optimize performance
- Documentation

## Files to Update

- `ATSAM3X8E_FW/ATSAM3X8E_FW/src/main.c` - Main logic
- `ATSAM3X8E_FW/ATSAM3X8E_FW/src/config/conf_board.h` - Configuration (if adding PWM pins)
- Add `protocol.h` - Protocol structures and constants

## Estimated Effort

- Protocol infrastructure: 4-6 hours
- Telemetry: 2-3 hours
- PWM: 3-4 hours
- Watchdog: 1-2 hours
- Testing & debugging: 4-6 hours

**Total**: 14-21 hours of development + testing

## Next Steps

1. Create updated `main.c` with protocol changes
2. Add `protocol.h` shared header
3. Build and flash to device
4. Test with physerver
5. Benchmark performance
6. Document any deviations from spec

## Notes

- Current firmware is working and functional
- Updates are enhancements, not bug fixes
- Can deploy physerver with existing firmware for basic testing
- Protocol updates enable full feature set and robustness
