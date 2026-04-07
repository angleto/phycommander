# Remote Access Guide

## Overview

Physerver exposes multiple interfaces for remote access, allowing you to control and monitor your Arduino Due from any computer on the network.

## Network Configuration

### Server Binding

The physerver binds to `0.0.0.0:8080` by default, making it accessible from:
- **Localhost**: http://localhost:8080
- **Local network**: http://192.168.1.100:8080 (replace with server IP)
- **Remote network**: http://your-public-ip:8080 (requires port forwarding)

### Find Your Server IP

```bash
# Linux
ip addr show | grep "inet "

# macOS
ifconfig | grep "inet "

# Get public IP (if accessing from internet)
curl ifconfig.me
```

## Access Methods

### 1. Web Dashboard (Browser)

**URL**: http://server-ip:8080

**Features**:
- Real-time GPIO control
- Live ADC readings
- DAC sliders
- System telemetry
- WebSocket live updates

**Remote Access**:
```bash
# From another computer on same network
http://192.168.1.100:8080

# From anywhere (with port forwarding)
http://your-domain.com:8080
```

### 2. REST API (HTTP)

All REST endpoints are accessible remotely via HTTP.

#### Get System Status

```bash
# From remote computer
curl http://server-ip:8080/api/status

# Returns JSON:
{
  "digital_in": 0,
  "digital_out": 0,
  "adc": [2048, 1024, 512, 256, 128, 64, 32, 16],
  "flags": {
    "adc_active": true,
    "dac_active": true,
    "pwm_active": false,
    "error": false,
    "watchdog_triggered": false,
    "usb_configured": true,
    "overrun": false
  },
  "seq_num": 42,
  "loop_time_us": 150,
  "uptime_ms": 123456,
  "error_count": 0
}
```

#### Get Current Command

```bash
curl http://server-ip:8080/api/command

# Returns:
{
  "digital_out": 0,
  "dac": [0, 0],
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

#### Set GPIO Pin

```bash
# Set pin 3 high
curl -X POST http://server-ip:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 3, "value": true}'

# Set pin 5 low
curl -X POST http://server-ip:8080/api/gpio/set \
  -H "Content-Type: application/json" \
  -d '{"pin": 5, "value": false}'
```

#### Set DAC Output

```bash
# Set DAC channel 0 to 2047 (mid-scale)
curl -X POST http://server-ip:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 0, "value": 2047}'

# Set DAC channel 1 to maximum
curl -X POST http://server-ip:8080/api/dac/set \
  -H "Content-Type: application/json" \
  -d '{"channel": 1, "value": 4095}'
```

#### Read ADC Values

```bash
curl http://server-ip:8080/api/adc/read

# Returns:
{
  "channels": [2048, 1024, 512, 256, 128, 64, 32, 16]
}
```

#### Set Complete Command

```bash
curl -X POST http://server-ip:8080/api/command \
  -H "Content-Type: application/json" \
  -d '{
    "digital_out": 255,
    "dac": [2047, 4095],
    "pwm": [32768, 0],
    "flags": {
      "adc_enable": true,
      "dac_enable": true,
      "pwm_enable": false,
      "reset_seq": false,
      "watchdog_disable": false
    },
    "seq_num": 0
  }'
```

### 3. WebSocket (Real-time Bidirectional)

**URL**: ws://server-ip:8080/ws

#### JavaScript Client

```html
<!DOCTYPE html>
<html>
<head>
    <title>PhyCMD Remote Control</title>
</head>
<body>
    <h1>PhyCMD Remote Control</h1>
    <div id="status"></div>

    <script>
        const ws = new WebSocket('ws://192.168.1.100:8080/ws');

        ws.onopen = () => {
            console.log('Connected to PhyCMD');
        };

        ws.onmessage = (event) => {
            const status = JSON.parse(event.data);
            document.getElementById('status').innerHTML = `
                <h2>Status</h2>
                <p>Digital In: ${status.digital_in}</p>
                <p>ADC 0: ${status.adc[0]}</p>
                <p>Loop Time: ${status.loop_time_us} µs</p>
                <p>Uptime: ${status.uptime_ms} ms</p>
            `;
        };

        // Send command
        function setGpio(pin, value) {
            const cmd = {
                digital_out: value ? (1 << pin) : 0,
                dac: [0, 0],
                pwm: [0, 0],
                flags: {
                    adc_enable: true,
                    dac_enable: true,
                    pwm_enable: false,
                    reset_seq: false,
                    watchdog_disable: false
                },
                seq_num: 0
            };
            ws.send(JSON.stringify(cmd));
        }
    </script>

    <button onclick="setGpio(0, true)">GPIO 0 High</button>
    <button onclick="setGpio(0, false)">GPIO 0 Low</button>
</body>
</html>
```

#### Python WebSocket Client

```python
import asyncio
import websockets
import json

async def phycmd_client():
    uri = "ws://192.168.1.100:8080/ws"

    async with websockets.connect(uri) as websocket:
        # Receive status updates
        while True:
            message = await websocket.recv()
            status = json.loads(message)
            print(f"ADC 0: {status['adc'][0]}")
            print(f"Loop time: {status['loop_time_us']} µs")

            # Send command
            command = {
                "digital_out": 0xFF,
                "dac": [2047, 2047],
                "pwm": [0, 0],
                "flags": {
                    "adc_enable": True,
                    "dac_enable": True,
                    "pwm_enable": False,
                    "reset_seq": False,
                    "watchdog_disable": False
                },
                "seq_num": 0
            }
            await websocket.send(json.dumps(command))

            await asyncio.sleep(1)

asyncio.run(phycmd_client())
```

### 4. IPC (Shared Memory - Local Only)

**Note**: Shared memory IPC is only available for applications running on the same machine as physerver.

For remote access from other computers, use REST API or WebSocket instead.

## Python Remote Client Example

```python
import requests
import json

class PhyCmdClient:
    def __init__(self, host='192.168.1.100', port=8080):
        self.base_url = f'http://{host}:{port}'

    def get_status(self):
        """Get current system status"""
        response = requests.get(f'{self.base_url}/api/status')
        return response.json()

    def set_gpio(self, pin, value):
        """Set a GPIO pin high or low"""
        data = {'pin': pin, 'value': value}
        response = requests.post(
            f'{self.base_url}/api/gpio/set',
            json=data
        )
        return response.status_code == 200

    def set_dac(self, channel, value):
        """Set DAC output (0-4095)"""
        data = {'channel': channel, 'value': value}
        response = requests.post(
            f'{self.base_url}/api/dac/set',
            json=data
        )
        return response.status_code == 200

    def read_adc(self):
        """Read all ADC channels"""
        response = requests.get(f'{self.base_url}/api/adc/read')
        return response.json()['channels']

# Usage
client = PhyCmdClient(host='192.168.1.100')

# Get status
status = client.get_status()
print(f"Uptime: {status['uptime_ms']} ms")
print(f"ADC values: {status['adc']}")

# Control GPIO
client.set_gpio(3, True)   # Set pin 3 high
client.set_gpio(5, False)  # Set pin 5 low

# Set DAC
client.set_dac(0, 2047)    # Mid-scale on channel 0

# Read ADC
adc_values = client.read_adc()
print(f"ADC readings: {adc_values}")
```

## Node.js Remote Client Example

```javascript
const axios = require('axios');

class PhyCmdClient {
    constructor(host = '192.168.1.100', port = 8080) {
        this.baseUrl = `http://${host}:${port}`;
    }

    async getStatus() {
        const response = await axios.get(`${this.baseUrl}/api/status`);
        return response.data;
    }

    async setGpio(pin, value) {
        await axios.post(`${this.baseUrl}/api/gpio/set`, {
            pin: pin,
            value: value
        });
    }

    async setDac(channel, value) {
        await axios.post(`${this.baseUrl}/api/dac/set`, {
            channel: channel,
            value: value
        });
    }

    async readAdc() {
        const response = await axios.get(`${this.baseUrl}/api/adc/read`);
        return response.data.channels;
    }
}

// Usage
(async () => {
    const client = new PhyCmdClient('192.168.1.100');

    // Get status
    const status = await client.getStatus();
    console.log(`Uptime: ${status.uptime_ms} ms`);

    // Control GPIO
    await client.setGpio(3, true);

    // Set DAC
    await client.setDac(0, 2047);

    // Read ADC
    const adc = await client.readAdc();
    console.log('ADC values:', adc);
})();
```

## Firewall Configuration

### Linux (UFW)

```bash
# Allow port 8080
sudo ufw allow 8080/tcp

# Check status
sudo ufw status
```

### Linux (iptables)

```bash
# Allow port 8080
sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT

# Save rules
sudo iptables-save > /etc/iptables/rules.v4
```

### macOS

```bash
# No firewall configuration needed by default
# If firewall is enabled, add rule in System Preferences > Security & Privacy > Firewall > Firewall Options
```

### Windows Firewall

```powershell
# Allow inbound on port 8080
New-NetFirewallRule -DisplayName "PhyCMD Server" -Direction Inbound -LocalPort 8080 -Protocol TCP -Action Allow
```

## Router Port Forwarding (Internet Access)

To access from outside your local network:

1. **Find your local server IP**: 192.168.1.100 (example)
2. **Log into your router** (usually http://192.168.1.1)
3. **Set up port forwarding**:
   - External port: 8080
   - Internal IP: 192.168.1.100
   - Internal port: 8080
   - Protocol: TCP

4. **Find your public IP**: `curl ifconfig.me`
5. **Access from anywhere**: http://your-public-ip:8080

**Security Warning**: Exposing to internet without authentication is risky. Consider:
- Using VPN instead
- Adding authentication layer
- Using SSH tunnel
- Restricting to specific IPs

## SSH Tunnel (Secure Remote Access)

Instead of exposing ports, use SSH tunnel:

```bash
# On remote computer, create tunnel
ssh -L 8080:localhost:8080 user@server-ip

# Now access via localhost
curl http://localhost:8080/api/status
```

## CORS Configuration

The server already has CORS enabled with `CorsLayer::permissive()`, allowing access from any origin.

To restrict origins, modify `physerver/src/web/mod.rs`:

```rust
use tower_http::cors::{CorsLayer, AllowOrigin};

// In create_router()
.layer(
    CorsLayer::new()
        .allow_origin("http://example.com".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST])
)
```

## Performance Over Network

### Latency

- **Local network**: ~1-5ms
- **Internet**: Depends on distance and connection (50-200ms typical)

### Bandwidth

- Status updates: ~1KB each
- At 10 Hz update rate: ~10 KB/s
- Minimal bandwidth requirement

### WebSocket vs REST

- **WebSocket**: Better for continuous monitoring (lower overhead)
- **REST**: Better for occasional queries (simpler)

## Testing Remote Access

```bash
# Test from another computer on network
ping server-ip

# Test HTTP access
curl http://server-ip:8080/api/status

# Test WebSocket (using websocat tool)
websocat ws://server-ip:8080/ws
```

## API Rate Limits

Currently no rate limiting is implemented. For production use, consider adding rate limiting middleware.

## Security Best Practices

1. **Use HTTPS**: Set up reverse proxy (nginx/caddy) with SSL
2. **Add Authentication**: Implement token-based auth
3. **Firewall**: Only allow specific IPs
4. **VPN**: Use VPN for remote access instead of exposing ports
5. **Monitor**: Log all API access
6. **Update**: Keep physerver and dependencies updated

## Reverse Proxy Setup (HTTPS)

### Nginx Configuration

```nginx
server {
    listen 443 ssl;
    server_name phycmd.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### Caddy Configuration

```
phycmd.example.com {
    reverse_proxy localhost:8080
}
```

## Troubleshooting

### Cannot connect from remote computer

1. Check server is running: `curl http://localhost:8080/api/status`
2. Check firewall: `sudo ufw status`
3. Check server is binding to 0.0.0.0, not 127.0.0.1
4. Verify IP address: `ip addr` or `ifconfig`
5. Test network connectivity: `ping server-ip`

### WebSocket connection fails

1. Check CORS settings
2. Verify WebSocket URL (ws:// not http://)
3. Check for proxy/firewall blocking WebSocket
4. Test with browser dev tools

### Slow response times

1. Check network latency: `ping server-ip`
2. Check server CPU usage
3. Reduce update rate if using WebSocket
4. Use local network instead of internet

## Support

For issues with remote access:
1. Verify local access works first
2. Check firewall and network configuration
3. Review server logs: `RUST_LOG=debug ./physerver`
4. Create issue with network details (redact public IPs)
