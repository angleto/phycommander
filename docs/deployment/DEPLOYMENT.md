# PhyCMD Deployment Guide

Complete guide for deploying PhyServer in production environments.

## Table of Contents

1. [Deployment Overview](#deployment-overview)
2. [System Requirements](#system-requirements)
3. [Installation](#installation)
4. [Systemd Service](#systemd-service)
5. [Security](#security)
6. [Networking](#networking)
7. [Monitoring](#monitoring)
8. [Backup and Recovery](#backup-and-recovery)
9. [Upgrades](#upgrades)
10. [Troubleshooting](#troubleshooting)

---

## Deployment Overview

### Deployment Checklist

- [ ] Hardware connected and tested
- [ ] Operating system configured
- [ ] Dependencies installed
- [ ] PhyServer built with release profile
- [ ] Configuration file created
- [ ] System permissions configured
- [ ] Real-time optimizations applied (see [REALTIME_OS_CONFIG.md](REALTIME_OS_CONFIG.md))
- [ ] Systemd service configured
- [ ] Firewall configured
- [ ] Monitoring configured
- [ ] Backup procedure established

### Deployment Scenarios

1. **Edge Device**: Standalone embedded system
2. **Laboratory Server**: Multi-user access via web interface
3. **Industrial Control**: High-reliability, real-time operation
4. **Test Automation**: Integrated with CI/CD systems

---

## System Requirements

### Minimum Requirements

**Hardware**:
- CPU: Intel Atom or equivalent (x86-64)
- RAM: 512 MB
- Storage: 100 MB for binaries + logs
- USB 2.0 port
- Arduino Due (ATSAM3X8E)

**Operating System**:
- Linux kernel 4.4+ (Ubuntu 20.04+ recommended)
- Real-time kernel (optional, for <10µs jitter)

**Network** (if using web interface):
- Ethernet or WiFi connectivity
- Static IP recommended for production

### Recommended Requirements

**For High Performance** (>5 kHz):
- CPU: Multi-core processor (e.g., Intel Core i3+)
- RAM: 1 GB+
- RT-preempt kernel or PREEMPT_RT patch
- CPU core isolation
- Fast USB 2.0/3.0 controller

See [REALTIME_OS_CONFIG.md](REALTIME_OS_CONFIG.md) for complete real-time OS configuration instructions

---

## Installation

### 1. Install Dependencies

**Ubuntu/Debian**:
```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libudev-dev \
    curl \
    git
```

**Fedora/RHEL**:
```bash
sudo dnf install -y \
    gcc \
    pkg-config \
    libudev-devel \
    curl \
    git
```

### 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustc --version  # Verify installation
```

### 3. Build PhyServer

```bash
# Clone repository
git clone https://github.com/angleto/phycommander.git
cd phycommander/physerver

# Build release version
cargo build --release

# Verify binary
./target/release/physerver --version
```

### 4. Install System-Wide

```bash
# Copy binary
sudo cp target/release/physerver /usr/local/bin/

# Create config directory
sudo mkdir -p /etc/physerver

# Copy configuration
sudo cp config.example.toml /etc/physerver/config.toml

# Edit configuration
sudo nano /etc/physerver/config.toml

# Verify
physerver --version
```

### 5. Configure Permissions

```bash
# Add user to dialout group
sudo usermod -a -G dialout $USER

# Grant real-time capabilities
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip /usr/local/bin/physerver

# Verify
getcap /usr/local/bin/physerver
```

### 6. Create Udev Rules

Create `/etc/udev/rules.d/99-arduino-due.rules`:

```bash
# Arduino Due (Programming Port)
SUBSYSTEM=="usb", ATTR{idVendor}=="2341", ATTR{idProduct}=="003e", MODE="0666", GROUP="dialout"

# Arduino Due (Native USB Port)
SUBSYSTEM=="usb", ATTR{idVendor}=="2341", ATTR{idProduct}=="003d", MODE="0666", GROUP="dialout"
```

Reload rules:
```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

---

## Systemd Service

### Service File

Create `/etc/systemd/system/physerver.service`:

```ini
[Unit]
Description=PhyServer - Physical Commander Server
Documentation=https://github.com/angleto/phycommander
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=physerver
Group=dialout
ExecStart=/usr/local/bin/physerver --config /etc/physerver/config.toml
Restart=always
RestartSec=5
StartLimitBurst=5
StartLimitIntervalSec=60

# Environment
Environment="RUST_LOG=info"

# Working directory
WorkingDirectory=/var/lib/physerver

# Security
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/physerver
ReadWritePaths=/var/log/physerver

# Real-time scheduling
LimitMEMLOCK=infinity
LimitNICE=-20

# Capabilities
AmbientCapabilities=CAP_SYS_NICE CAP_IPC_LOCK CAP_SYS_ADMIN

[Install]
WantedBy=multi-user.target
```

### Create User and Directories

```bash
# Create user
sudo useradd -r -s /bin/false physerver
sudo usermod -a -G dialout physerver

# Create directories
sudo mkdir -p /var/lib/physerver
sudo mkdir -p /var/log/physerver
sudo chown physerver:dialout /var/lib/physerver
sudo chown physerver:dialout /var/log/physerver
```

### Enable and Start Service

```bash
# Reload systemd
sudo systemctl daemon-reload

# Enable service (start on boot)
sudo systemctl enable physerver

# Start service
sudo systemctl start physerver

# Check status
sudo systemctl status physerver

# View logs
sudo journalctl -u physerver -f
```

### Service Management

```bash
# Start
sudo systemctl start physerver

# Stop
sudo systemctl stop physerver

# Restart
sudo systemctl restart physerver

# Status
sudo systemctl status physerver

# Logs (last 100 lines)
sudo journalctl -u physerver -n 100

# Follow logs
sudo journalctl -u physerver -f

# Logs since boot
sudo journalctl -u physerver -b
```

---

## Security

### Firewall Configuration

**UFW (Ubuntu)**:
```bash
# Enable firewall
sudo ufw enable

# Allow SSH (if remote access needed)
sudo ufw allow ssh

# Allow PhyServer web interface
sudo ufw allow 8080/tcp

# Check status
sudo ufw status
```

**firewalld (RHEL/Fedora)**:
```bash
# Allow PhyServer
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --reload

# Check
sudo firewall-cmd --list-ports
```

### Restrict Access by IP

**Using UFW**:
```bash
# Allow only specific IP
sudo ufw allow from 192.168.1.100 to any port 8080 proto tcp

# Allow subnet
sudo ufw allow from 192.168.1.0/24 to any port 8080 proto tcp
```

**Using iptables**:
```bash
# Allow specific IP
sudo iptables -A INPUT -p tcp -s 192.168.1.100 --dport 8080 -j ACCEPT

# Block all others
sudo iptables -A INPUT -p tcp --dport 8080 -j DROP

# Save
sudo iptables-save > /etc/iptables/rules.v4
```

### Reverse Proxy (HTTPS)

Use Nginx as reverse proxy for HTTPS:

**/etc/nginx/sites-available/physerver**:
```nginx
server {
    listen 443 ssl http2;
    server_name physerver.example.com;

    ssl_certificate /etc/letsencrypt/live/physerver.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/physerver.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket support
    location /ws {
        proxy_pass http://127.0.0.1:8080/ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "Upgrade";
        proxy_set_header Host $host;
    }
}
```

Enable:
```bash
sudo ln -s /etc/nginx/sites-available/physerver /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### Authentication

PhyServer doesn't include built-in authentication. Use:

1. **Reverse proxy authentication** (Nginx basic auth)
2. **VPN** (WireGuard, OpenVPN)
3. **SSH tunnel** (for remote access)

**Nginx Basic Auth**:
```bash
# Create password file
sudo htpasswd -c /etc/nginx/.htpasswd phyuser

# Add to Nginx config
location / {
    auth_basic "PhyServer Access";
    auth_basic_user_file /etc/nginx/.htpasswd;
    proxy_pass http://127.0.0.1:8080;
}
```

---

## Networking

### Static IP Configuration

**netplan** (Ubuntu 20.04+):

`/etc/netplan/01-netcfg.yaml`:
```yaml
network:
  version: 2
  ethernets:
    eth0:
      addresses:
        - 192.168.1.100/24
      gateway4: 192.168.1.1
      nameservers:
        addresses: [8.8.8.8, 8.8.4.4]
```

Apply:
```bash
sudo netplan apply
```

### Remote Access

**SSH Tunnel**:
```bash
# On client
ssh -L 8080:localhost:8080 user@physerver.example.com

# Access via browser
open http://localhost:8080
```

**VPN** (WireGuard):
1. Set up WireGuard server on PhyServer host
2. Connect clients via VPN
3. Access via internal IP

---

## Monitoring

### Health Monitoring

**systemd watchdog**:

Add to service file:
```ini
[Service]
WatchdogSec=30
```

PhyServer will notify systemd it's alive every 30 seconds.

### Log Monitoring

**logrotate** configuration:

`/etc/logrotate.d/physerver`:
```
/var/log/physerver/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 0644 physerver dialout
}
```

### Performance Monitoring

**Prometheus** (if available):

Monitor via REST API:
```python
# prometheus_exporter.py
from prometheus_client import start_http_server, Gauge
import requests
import time

loop_time = Gauge('physerver_loop_time_us', 'Loop time in microseconds')
error_count = Gauge('physerver_error_count', 'Error count')

def collect():
    status = requests.get('http://localhost:8080/api/status').json()
    loop_time.set(status['loop_time_us'])
    error_count.set(status['error_count'])

if __name__ == '__main__':
    start_http_server(9090)
    while True:
        collect()
        time.sleep(1)
```

### Alerts

**Simple email alerting**:

```bash
#!/bin/bash
# /usr/local/bin/physerver-check.sh

# Check if service is running
if ! systemctl is-active --quiet physerver; then
    echo "PhyServer is down!" | mail -s "PhyServer Alert" admin@example.com
    systemctl restart physerver
fi

# Check API
if ! curl -sf http://localhost:8080/api/status > /dev/null; then
    echo "PhyServer API not responding!" | mail -s "PhyServer Alert" admin@example.com
fi
```

Cron job:
```cron
# Check every 5 minutes
*/5 * * * * /usr/local/bin/physerver-check.sh
```

---

## Backup and Recovery

### Configuration Backup

```bash
# Backup script
#!/bin/bash
# /usr/local/bin/backup-physerver.sh

BACKUP_DIR="/var/backups/physerver"
DATE=$(date +%Y%m%d_%H%M%S)

mkdir -p $BACKUP_DIR

# Backup configuration
cp /etc/physerver/config.toml $BACKUP_DIR/config_$DATE.toml

# Backup systemd service
cp /etc/systemd/system/physerver.service $BACKUP_DIR/physerver.service_$DATE

# Backup udev rules
cp /etc/udev/rules.d/99-arduino-due.rules $BACKUP_DIR/99-arduino-due.rules_$DATE

# Backup binary (optional)
cp /usr/local/bin/physerver $BACKUP_DIR/physerver_$DATE

# Keep only last 30 days
find $BACKUP_DIR -type f -mtime +30 -delete

echo "Backup complete: $BACKUP_DIR"
```

Daily backup:
```cron
0 2 * * * /usr/local/bin/backup-physerver.sh
```

### Recovery

```bash
# Restore configuration
sudo cp /var/backups/physerver/config_YYYYMMDD.toml /etc/physerver/config.toml

# Restart service
sudo systemctl restart physerver
```

### Disaster Recovery

1. **Document hardware setup** (pin connections, wiring)
2. **Keep firmware binary** backed up
3. **Keep configuration** in version control
4. **Document deployment steps**

Recovery steps:
1. Install fresh OS
2. Install dependencies
3. Build/install physerver
4. Restore configuration
5. Upload firmware to Arduino
6. Start service

---

## Upgrades

### Upgrade Procedure

```bash
# 1. Backup current installation
sudo /usr/local/bin/backup-physerver.sh

# 2. Stop service
sudo systemctl stop physerver

# 3. Build new version
cd phycmd/physerver
git pull
cargo build --release

# 4. Install new binary
sudo cp target/release/physerver /usr/local/bin/

# 5. Verify configuration compatibility
physerver --config /etc/physerver/config.toml --help

# 6. Start service
sudo systemctl start physerver

# 7. Check status
sudo systemctl status physerver
sudo journalctl -u physerver -f
```

### Rollback

```bash
# Restore previous binary
sudo cp /var/backups/physerver/physerver_YYYYMMDD /usr/local/bin/physerver

# Restart
sudo systemctl restart physerver
```

### Zero-Downtime Upgrades

For critical systems:

```bash
# 1. Prepare new version
cargo build --release
sudo cp target/release/physerver /usr/local/bin/physerver.new

# 2. Switch atomically
sudo mv /usr/local/bin/physerver /usr/local/bin/physerver.old
sudo mv /usr/local/bin/physerver.new /usr/local/bin/physerver

# 3. Restart service
sudo systemctl restart physerver

# Verify
sudo systemctl status physerver
```

---

## Troubleshooting

### Service Won't Start

```bash
# Check status
sudo systemctl status physerver

# Check logs
sudo journalctl -u physerver -n 100

# Check permissions
ls -l /usr/local/bin/physerver
groups physerver

# Check configuration
physerver --config /etc/physerver/config.toml --help

# Test manually
sudo -u physerver /usr/local/bin/physerver --config /etc/physerver/config.toml
```

### Device Not Detected

```bash
# Check USB connection
lsusb | grep -i arduino

# Check serial ports
ls /dev/ttyACM*

# Check udev rules
cat /etc/udev/rules.d/99-arduino-due.rules

# Reload udev
sudo udevadm control --reload-rules
sudo udevadm trigger

# Check permissions
ls -l /dev/ttyACM0
```

### High Latency

```bash
# Check system load
top
htop

# Check CPU governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Set to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Check logs for warnings
sudo journalctl -u physerver | grep -i warn
```

### Service Crashes

```bash
# Check logs
sudo journalctl -u physerver -since "1 hour ago"

# Enable core dumps
ulimit -c unlimited

# Run manually to debug
sudo -u physerver RUST_LOG=debug /usr/local/bin/physerver --config /etc/physerver/config.toml
```

---

## Production Best Practices

1. **Use static IP** for server
2. **Enable systemd service** for auto-start
3. **Configure firewall** to restrict access
4. **Use reverse proxy** for HTTPS
5. **Monitor service health**
6. **Set up automated backups**
7. **Document hardware setup**
8. **Test recovery procedure**
9. **Keep system updated**
10. **Monitor logs regularly**

---

## See Also

- [USER_MANUAL.md](../user-guide/USER_MANUAL.md) - Usage guide
- [CONFIGURATION.md](../user-guide/CONFIGURATION.md) - Configuration reference
- [PERFORMANCE.md](../technical/PERFORMANCE.md) - Performance tuning
- [BUILDING.md](../getting-started/BUILDING.md) - Build instructions

---

**End of Deployment Guide**
