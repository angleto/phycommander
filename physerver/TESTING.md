# Testing Guide

## Prerequisites

To run tests on Linux, you need `libudev-dev`:

```bash
sudo apt-get install libudev-dev pkg-config
```

## Running Tests

### Unit Tests

```bash
# Run all unit tests
cargo test

# Run tests for specific module
cargo test protocol
cargo test serial
cargo test ipc

# Run with verbose output
cargo test -- --nocapture

# Run specific test
cargo test test_encode_command
```

### Integration Tests

```bash
# Run integration tests
cargo test --test integration_test

# Run all tests (unit + integration)
cargo test --all
```

### With Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html
```

## Test Organization

### Unit Tests

Located in each module file:
- `src/protocol/codec.rs` - Protocol encoding/decoding tests
- `src/protocol/crc.rs` - CRC calculation tests
- `src/protocol/types.rs` - Type conversion tests

### Integration Tests

Located in `tests/`:
- `tests/integration_test.rs` - Full system integration tests

## Test Categories

### Protocol Tests

Test the PhyCMD-64 protocol implementation:
- ✅ Command encoding
- ✅ Status decoding
- ✅ CRC validation
- ✅ Header validation
- ✅ Flags conversion
- ✅ DAC value clamping
- ✅ Roundtrip encoding/decoding

### CRC Tests

Test CRC-16-CCITT calculation:
- ✅ Empty data
- ✅ Known test vectors
- ✅ Table-based vs direct calculation

### Integration Tests

Test complete system behavior:
- ✅ End-to-end protocol flow
- ✅ Error detection
- ✅ Multiple message encoding
- ✅ Flag roundtrip conversion

## Manual Testing

### Serial Port (Requires Hardware)

```bash
# List available ports
cargo run --example simple_client

# Test with actual Arduino Due
./target/release/physerver --port /dev/ttyACM0
```

### Web Server

```bash
# Start server
./target/release/physerver --auto-detect

# Test REST API
curl http://localhost:8080/api/status

# Test GPIO control
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 3, "value": true}'
```

### WebSocket

Use browser console or websocat:

```bash
# Install websocat
cargo install websocat

# Connect to WebSocket
websocat ws://localhost:8080/ws
```

## Benchmark Tests

```bash
# Run benchmarks (if implemented)
cargo bench

# Profile performance
cargo build --release
perf record ./target/release/physerver --auto-detect
perf report
```

## Test Data

### Valid Command Message

```
Header:    0xAA55
DigOut:    0x00FF
DAC0:      2047 (0x07FF)
DAC1:      4095 (0x0FFF)
PWM0:      32768 (0x8000)
PWM1:      65535 (0xFFFF)
Flags:     0x07 (ADC, DAC, PWM enabled)
SeqNum:    42
CRC:       [calculated]
Reserved:  [48 zeros]
```

### Valid Status Message

```
Header:     0x55AA
DigIn:      0x00FF
DigOut:     0xFF00
ADC[0-7]:   [2048, 1024, 512, 256, 128, 64, 32, 16]
Flags:      0x07 (ADC, DAC, PWM active)
SeqNum:     42
CRC:        [calculated]
LoopTime:   150 µs
Uptime:     123456 ms
ErrorCount: 0
Reserved:   [30 zeros]
```

## Known Test Limitations

1. **Serial Port Tests**: Require actual hardware or virtual serial ports
2. **IPC Tests**: Require shared memory support (Linux/macOS)
3. **Real-time Tests**: Require elevated privileges
4. **Network Tests**: May be affected by firewall

## Continuous Integration

For CI/CD pipelines:

```yaml
# .github/workflows/test.yml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Install dependencies
        run: sudo apt-get install -y libudev-dev pkg-config
      - name: Run tests
        run: cargo test --all
```

## Test Results

Expected output:

```
running 15 tests
test protocol::codec::tests::test_crc_mismatch ... ok
test protocol::codec::tests::test_dac_clamping ... ok
test protocol::codec::tests::test_decode_full_status ... ok
test protocol::codec::tests::test_decode_status ... ok
test protocol::codec::tests::test_encode_command ... ok
test protocol::codec::tests::test_encode_decode_roundtrip ... ok
test protocol::codec::tests::test_invalid_header ... ok
test protocol::codec::tests::test_invalid_message_length ... ok
test protocol::crc::tests::test_crc_empty ... ok
test protocol::crc::tests::test_crc_simple ... ok
test protocol::crc::tests::test_crc_table_matches ... ok
test integration_test::test_command_flags_integration ... ok
test integration_test::test_error_detection ... ok
test integration_test::test_multiple_command_encode ... ok
test integration_test::test_protocol_integration ... ok
test integration_test::test_status_decode_integration ... ok
test integration_test::test_status_flags_integration ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured
```

## Adding New Tests

### Unit Test Template

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_your_feature() {
        // Arrange
        let input = setup_test_data();

        // Act
        let result = your_function(input);

        // Assert
        assert_eq!(result, expected_value);
    }
}
```

### Integration Test Template

```rust
// tests/my_test.rs
use physerver::{Command, Status};

#[test]
fn test_integration_scenario() {
    // Test full workflow
    let cmd = Command::default();
    let bytes = physerver::protocol::encode_command(&cmd);
    assert_eq!(bytes.len(), 64);
}
```

## Troubleshooting Tests

### libudev not found

```bash
sudo apt-get install libudev-dev pkg-config
```

### Shared memory errors

```bash
# Clean up stale shared memory
rm /dev/shm/phycmd_state
```

### Permission denied

```bash
# Add user to dialout group
sudo usermod -a -G dialout $USER
# Log out and back in
```

## Coverage Goals

Target test coverage:
- Protocol module: 100%
- Serial module: 80% (hardware dependent)
- IPC module: 90%
- Web module: 70% (network dependent)
- Overall: 85%

## Performance Benchmarks

Expected performance on modern hardware:
- Protocol encode: < 1 µs
- Protocol decode: < 2 µs
- CRC calculation: < 500 ns
- Full roundtrip: < 5 µs
