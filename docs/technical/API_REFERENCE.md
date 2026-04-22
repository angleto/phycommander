# PhyCMD API Reference

**Version 2.0.0**

Complete API documentation for PhyServer interfaces.

## Table of Contents

1. [REST API](#rest-api)
2. [WebSocket API](#websocket-api)
3. [Rust IPC API](#rust-ipc-api)
4. [Protocol Types](#protocol-types)
5. [Error Codes](#error-codes)

---

## REST API

Base URL: `http://localhost:8080/api`

### GET /api/status

Get the current device status.

**Request**:
```http
GET /api/status HTTP/1.1
Host: localhost:8080
```

**Response** (200 OK):
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

**Fields**:
- `digital_in` (u16): Digital input state (bits 0-15)
- `digital_out` (u16): Digital output state (bits 0-15)
- `adc` (u16[8]): ADC values (0-4095), channels 0-7
- `seq_num` (u8): Sequence number (wraps at 255)
- `loop_time_us` (u16): Loop execution time in microseconds
- `uptime_ms` (u32): Device uptime in milliseconds
- `error_count` (u16): Cumulative error counter
- `flags` (object): Status flags (see below)

**Status Flags**:
- `adc_active`: ADC is enabled
- `dac_active`: DAC is enabled
- `pwm_active`: PWM is enabled
- `error`: Error condition detected
- `watchdog_triggered`: Communications watchdog triggered
- `usb_configured`: USB is configured
- `overrun`: Data overrun detected

---

### POST /api/gpio/set

Set a single GPIO output pin.

**Request**:
```http
POST /api/gpio/set HTTP/1.1
Host: localhost:8080
Content-Type: application/json

{
  "pin": 3,
  "value": true
}
```

**Parameters**:
- `pin` (integer, 0-15): GPIO pin number
- `value` (boolean): Pin state (true=high, false=low)

**Response** (200 OK):
```json
{
  "success": true,
  "pin": 3,
  "value": true
}
```

**Error Response** (400 Bad Request):
```json
{
  "error": "Invalid pin number: 16. Must be 0-15"
}
```

**Examples**:

```bash
# Set pin 5 high
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 5, "value": true}'

# Set pin 10 low
curl -X POST http://localhost:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 10, "value": false}'
```

---

### POST /api/dac/set

Set a DAC channel output value.

**Request**:
```http
POST /api/dac/set HTTP/1.1
Host: localhost:8080
Content-Type: application/json

{
  "channel": 0,
  "value": 2047
}
```

**Parameters**:
- `channel` (integer, 0-1): DAC channel number
- `value` (integer, 0-4095): DAC value (0 = 0V, 4095 = 3.3V)

**Response** (200 OK):
```json
{
  "success": true,
  "channel": 0,
  "value": 2047
}
```

**Error Response** (400 Bad Request):
```json
{
  "error": "Invalid DAC value: 5000. Must be 0-4095"
}
```

**Voltage Conversion**:
```
voltage = (value / 4095) * 3.3V

Examples:
  0    = 0.0V
  2047 = 1.65V
  4095 = 3.3V
```

**Examples**:

```bash
# Set DAC0 to 1.65V (mid-scale)
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 2047}'

# Set DAC1 to 3.3V (full-scale)
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 1, "value": 4095}'

# Set DAC0 to 0V
curl -X POST http://localhost:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 0}'
```

---

### POST /api/command

Send a complete command to the device.

**Request**:
```http
POST /api/command HTTP/1.1
Host: localhost:8080
Content-Type: application/json

{
  "digital_out": 255,
  "dac": [2047, 4095],
  "pwm": [0, 0],
  "flags": {
    "adc_enable": true,
    "dac_enable": true,
    "pwm_enable": false,
    "reset_seq": false,
    "watchdog_disable": false
  }
}
```

**Parameters**:
- `digital_out` (u16): Digital output state (16 bits)
- `dac` (u16[2]): DAC values for channels 0-1
- `pwm` (u16[2]): PWM values for channels 0-1 (0-65535)
- `flags` (object): Command flags

**Command Flags**:
- `adc_enable`: Enable ADC sampling
- `dac_enable`: Enable DAC outputs
- `pwm_enable`: Enable PWM outputs
- `reset_seq`: Reset sequence number
- `watchdog_disable`: Disable communications watchdog

**Response** (200 OK):
```json
{
  "success": true
}
```

---

### Error Responses

All endpoints may return error responses:

**400 Bad Request**:
```json
{
  "error": "Invalid parameter: ..."
}
```

**500 Internal Server Error**:
```json
{
  "error": "Communication error: ..."
}
```

**503 Service Unavailable**:
```json
{
  "error": "Device not connected"
}
```

---

## WebSocket API

WebSocket endpoint: `ws://localhost:8080/ws`

### Connection

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onopen = () => {
  console.log('Connected to PhyServer');
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = () => {
  console.log('Disconnected from PhyServer');
};
```

### Receiving Status Updates

The server broadcasts status updates to all connected WebSocket clients automatically.

**Message Format** (Server → Client):
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

**Example**:
```javascript
ws.onmessage = (event) => {
  const status = JSON.parse(event.data);

  // Display ADC values
  console.log('ADC 0:', status.adc[0]);
  console.log('ADC 1:', status.adc[1]);

  // Check digital inputs
  for (let i = 0; i < 16; i++) {
    const value = (status.digital_in >> i) & 1;
    console.log(`Digital Input ${i}:`, value);
  }

  // Monitor performance
  console.log('Loop time:', status.loop_time_us, 'µs');

  // Check for errors
  if (status.flags.error) {
    console.error('Device error!');
  }
};
```

### Sending Commands

**Message Format** (Client → Server):
```json
{
  "digital_out": 255,
  "dac": [2047, 4095],
  "pwm": [0, 0],
  "flags": {
    "adc_enable": true,
    "dac_enable": true,
    "pwm_enable": false,
    "reset_seq": false,
    "watchdog_disable": false
  },
  "seq_num": 0
}
```

**Example**:
```javascript
// Send command
const cmd = {
  digital_out: 0xFF,  // All outputs high
  dac: [2047, 4095],  // DAC0=1.65V, DAC1=3.3V
  pwm: [0, 0],
  flags: {
    adc_enable: true,
    dac_enable: true,
    pwm_enable: false,
    reset_seq: false,
    watchdog_disable: false
  },
  seq_num: 0  // Server will override
};

ws.send(JSON.stringify(cmd));
```

### Full Example

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onopen = () => {
  console.log('Connected');

  // Send command to set outputs
  ws.send(JSON.stringify({
    digital_out: 0x00FF,  // Pins 0-7 high
    dac: [2047, 0],
    pwm: [0, 0],
    flags: {
      adc_enable: true,
      dac_enable: true,
      pwm_enable: false,
      reset_seq: false,
      watchdog_disable: false
    },
    seq_num: 0
  }));
};

ws.onmessage = (event) => {
  const status = JSON.parse(event.data);
  document.getElementById('adc0').textContent = status.adc[0];
  document.getElementById('latency').textContent = status.loop_time_us;
};

ws.onerror = (error) => {
  console.error('Error:', error);
};

ws.onclose = () => {
  console.log('Disconnected');
};
```

---

## Rust IPC API

For ultra-low latency communication from Rust applications.

### IPC Client

```rust
use physerver::{IpcClient, Command, Status, CommandFlags, StatusFlags};

fn main() -> anyhow::Result<()> {
    // Connect to IPC server
    let client = IpcClient::connect()?;

    // Read status
    let status: Status = client.read_status();

    // Write command
    let mut cmd = Command::default();
    cmd.digital_out = 0x00FF;
    cmd.dac[0] = 2047;
    cmd.flags = CommandFlags {
        adc_enable: true,
        dac_enable: true,
        pwm_enable: false,
        reset_seq: false,
        watchdog_disable: false,
    };
    client.write_command(&cmd);

    Ok(())
}
```

### IpcClient Methods

#### `IpcClient::connect() -> Result<Self>`

Connect to the IPC server.

**Returns**: IpcClient instance

**Errors**:
- `anyhow::Error` if connection fails

**Example**:
```rust
let client = IpcClient::connect()?;
```

---

#### `read_status(&self) -> Status`

Read the current device status from shared memory.

**Returns**: Status structure

**Performance**: ~100ns (lock-free atomic read)

**Example**:
```rust
let status = client.read_status();
println!("ADC 0: {}", status.adc[0]);
println!("Digital inputs: 0x{:04X}", status.digital_in);
```

---

#### `write_command(&self, cmd: &Command)`

Write a command to shared memory for transmission to device.

**Parameters**:
- `cmd`: Reference to Command structure

**Performance**: ~100ns (lock-free atomic write)

**Example**:
```rust
let mut cmd = Command::default();
cmd.digital_out = 0x00FF;
cmd.dac[0] = 2047;
client.write_command(&cmd);
```

---

### Transport Trait

For advanced use cases, you can use the Transport trait directly.

```rust
use physerver::{Transport, SerialTransport, Command, Status};

fn main() -> anyhow::Result<()> {
    // Create transport
    let mut transport = SerialTransport::new("/dev/ttyACM0", 921600)?;

    // Send command and receive status
    let mut cmd = Command::default();
    cmd.digital_out = 0x00FF;

    let status = transport.exchange(&cmd)?;
    println!("Loop time: {} µs", status.loop_time_us);

    // Get transport statistics
    let stats = transport.stats();
    println!("Bytes sent: {}", stats.bytes_sent);
    println!("Bytes received: {}", stats.bytes_received);
    println!("Errors: {}", stats.errors);

    Ok(())
}
```

### Transport Methods

#### `send_command(&mut self, cmd: &Command) -> Result<()>`

Send a command to the device.

**Parameters**:
- `cmd`: Reference to Command

**Returns**: `Result<()>`

---

#### `receive_status(&mut self) -> Result<Status>`

Receive a status message from the device.

**Returns**: `Result<Status>`

---

#### `exchange(&mut self, cmd: &Command) -> Result<Status>`

Send a command and receive status (atomic operation).

**Parameters**:
- `cmd`: Reference to Command

**Returns**: `Result<Status>`

**Example**:
```rust
let status = transport.exchange(&cmd)?;
```

---

#### `stats(&self) -> &TransportStats`

Get transport statistics.

**Returns**: Reference to TransportStats

**Example**:
```rust
let stats = transport.stats();
println!("Latency: {} µs", stats.last_latency_us);
```

---

#### `max_rate(&self) -> u32`

Get maximum sustainable update rate for this transport.

**Returns**: u32 (Hz)

**Example**:
```rust
println!("Max rate: {} Hz", transport.max_rate());
```

---

#### `typical_latency_us(&self) -> u32`

Get typical round-trip latency for this transport.

**Returns**: u32 (microseconds)

**Example**:
```rust
println!("Typical latency: {} µs", transport.typical_latency_us());
```

---

## Protocol Types

### Command

Command sent from host to device.

```rust
pub struct Command {
    pub digital_out: u16,      // Digital output state
    pub dac: [u16; 2],         // DAC values [0-4095]
    pub pwm: [u16; 2],         // PWM values [0-65535]
    pub flags: CommandFlags,   // Command flags
    pub seq_num: u8,           // Sequence number
}
```

**Default**:
```rust
Command {
    digital_out: 0,
    dac: [0, 0],
    pwm: [0, 0],
    flags: CommandFlags::default(),
    seq_num: 0,
}
```

---

### CommandFlags

Flags in command message.

```rust
pub struct CommandFlags {
    pub adc_enable: bool,         // Enable ADC
    pub dac_enable: bool,         // Enable DAC
    pub pwm_enable: bool,         // Enable PWM
    pub reset_seq: bool,          // Reset sequence number
    pub watchdog_disable: bool,   // Disable watchdog
}
```

**Default**:
```rust
CommandFlags {
    adc_enable: true,
    dac_enable: true,
    pwm_enable: false,
    reset_seq: false,
    watchdog_disable: false,
}
```

---

### Status

Status received from device.

```rust
pub struct Status {
    pub digital_in: u16,       // Digital input state
    pub digital_out: u16,      // Digital output echo
    pub adc: [u16; 8],         // ADC values [0-4095]
    pub seq_num: u8,           // Sequence number
    pub loop_time_us: u16,     // Loop time (µs)
    pub uptime_ms: u32,        // Uptime (ms)
    pub error_count: u16,      // Error counter
    pub flags: StatusFlags,    // Status flags
}
```

**Default**:
```rust
Status {
    digital_in: 0,
    digital_out: 0,
    adc: [0; 8],
    seq_num: 0,
    loop_time_us: 0,
    uptime_ms: 0,
    error_count: 0,
    flags: StatusFlags::default(),
}
```

---

### StatusFlags

Flags in status message.

```rust
pub struct StatusFlags {
    pub adc_active: bool,          // ADC active
    pub dac_active: bool,          // DAC active
    pub pwm_active: bool,          // PWM active
    pub error: bool,               // Error condition
    pub watchdog_triggered: bool,  // Watchdog triggered
    pub usb_configured: bool,      // USB configured
    pub overrun: bool,             // Data overrun
}
```

---

### TransportStats

Transport-level statistics.

```rust
pub struct TransportStats {
    pub bytes_sent: u64,           // Total bytes sent
    pub bytes_received: u64,       // Total bytes received
    pub errors: u64,               // Error count
    pub last_latency_us: u32,      // Last latency (µs)
}
```

---

## Error Codes

### Transport Errors

- `TransportError::IoError`: I/O error (device disconnected, etc.)
- `TransportError::CrcError`: CRC validation failed
- `TransportError::TimeoutError`: Operation timed out
- `TransportError::DeviceNotFound`: Device not found

### HTTP Status Codes

- `200 OK`: Success
- `400 Bad Request`: Invalid parameters
- `500 Internal Server Error`: Server error
- `503 Service Unavailable`: Device not connected

---

## Rate Limits

### REST API

- No enforced rate limit
- Recommended: <100 requests/second

### WebSocket

- Status updates: At configured update_rate (default 1 kHz)
- Commands: No limit (processed asynchronously)

### IPC

- No rate limit
- Can sustain >10 kHz update rates

---

## CORS

The REST API has CORS enabled:
- All origins allowed (`*`)
- Methods: GET, POST
- Headers: Content-Type

This allows web applications from any origin to access the API.

---

## Examples Repository

See `physerver/examples/` for complete examples:
- `simple_client.rs`: Basic IPC client
- `high_frequency.rs`: High-frequency data acquisition
- `web_client.html`: WebSocket browser client

---

**End of API Reference**
