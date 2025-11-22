# Real-Time Operating System Configuration Guide

This guide provides comprehensive instructions for configuring a Linux system with real-time capabilities to host the PhyServer. A properly configured RT system is essential for achieving the microsecond-level latency and deterministic performance required for precision hardware control.

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [RT-Preempt Kernel Installation](#rt-preempt-kernel-installation)
4. [CPU Core Isolation](#cpu-core-isolation)
5. [Boot Parameters Reference](#boot-parameters-reference)
6. [System Service Configuration](#system-service-configuration)
7. [Validation and Testing](#validation-and-testing)
8. [Troubleshooting](#troubleshooting)
9. [Complete Setup Examples](#complete-setup-examples)

---

## Overview

### What is Real-Time Linux?

Real-Time Linux with the PREEMPT_RT patch provides:
- **Deterministic scheduling**: Guaranteed response times for critical tasks
- **Low latency**: Microsecond-level interrupt and scheduling latency
- **Priority inversion handling**: Proper priority inheritance for mutexes
- **Reduced jitter**: Consistent timing for periodic operations

### Why PhyServer Needs RT Linux

PhyServer achieves:
- **10 kHz update rates** over USB (100 μs cycle time)
- **1 kHz update rates** over Serial (1 ms cycle time)
- **Microsecond-level latency** for command processing
- **Deterministic behavior** for safety-critical control loops

Without proper RT configuration, you may experience:
- ❌ Timing jitter and missed deadlines
- ❌ Unpredictable latency spikes
- ❌ Dropped commands or communication timeouts
- ❌ Reduced control loop performance

---

## Prerequisites

### Hardware Requirements

- **Multi-core CPU**: Minimum 2 cores (4+ recommended)
  - 1 core for RT tasks (PhyServer)
  - 1+ cores for general OS and background processes
- **Sufficient RAM**: 4GB minimum (8GB+ recommended)
- **USB 2.0/3.0 port**: For Arduino Due communication

### Software Requirements

- **Linux distribution**: Ubuntu 20.04/22.04, Fedora 35+, or Debian 11+
- **Root access**: Required for kernel installation and system configuration
- **Build tools**: gcc, make, kernel build dependencies (for kernel compilation)

### Verify Current Kernel

Check your current kernel version:

```bash
uname -r
```

Check if RT patch is already installed:

```bash
uname -r | grep -i rt
```

If output contains `rt` or `PREEMPT_RT`, you already have an RT kernel.

---

## RT-Preempt Kernel Installation

The PREEMPT_RT patch converts Linux into a fully preemptible real-time kernel. Choose the installation method based on your distribution.

### Option 1: Pre-built RT Kernel Packages (Recommended)

Pre-built packages are easier to install and maintain.

#### Ubuntu 20.04 / 22.04

Ubuntu provides RT kernels through the `linux-lowlatency` package:

```bash
# Update package index
sudo apt update

# Install low-latency kernel (includes RT patches)
sudo apt install linux-lowlatency linux-headers-lowlatency

# Reboot to use new kernel
sudo reboot
```

After reboot, verify:

```bash
uname -r
# Should show: x.xx.x-xx-lowlatency
```

For full PREEMPT_RT kernel on Ubuntu:

```bash
# Add Ubuntu RT PPA (Ubuntu 22.04)
sudo add-apt-repository ppa:canonical-kernel-team/ppa
sudo apt update

# Install RT kernel
sudo apt install linux-image-realtime linux-headers-realtime

# Reboot
sudo reboot
```

#### Debian 11/12

```bash
# Update package index
sudo apt update

# Install RT kernel
sudo apt install linux-image-rt-amd64 linux-headers-rt-amd64

# Reboot
sudo reboot
```

Verify after reboot:

```bash
uname -r | grep rt
```

#### Fedora 35+ / RHEL 8+

```bash
# Install RT kernel
sudo dnf install kernel-rt kernel-rt-devel

# Reboot
sudo reboot
```

Verify:

```bash
uname -r | grep rt
```

### Option 2: Build RT Kernel from Source

Use this method if pre-built packages are unavailable or you need a specific kernel version.

#### Step 1: Install Build Dependencies

**Ubuntu/Debian:**

```bash
sudo apt update
sudo apt install build-essential libncurses-dev bison flex libssl-dev \
    libelf-dev bc curl wget xz-utils
```

**Fedora:**

```bash
sudo dnf install gcc make ncurses-devel bison flex openssl-devel \
    elfutils-libelf-devel bc curl wget xz
```

#### Step 2: Download Kernel Source and RT Patch

Visit https://kernel.org/pub/linux/kernel/projects/rt/ to find matching versions.

Example for kernel 6.1.x:

```bash
# Create working directory
mkdir -p ~/rt-kernel && cd ~/rt-kernel

# Download kernel source (example: 6.1.38)
KERNEL_VERSION=6.1.38
wget https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz

# Download matching RT patch
RT_PATCH_VERSION=6.1.38-rt13
wget https://cdn.kernel.org/pub/linux/kernel/projects/rt/6.1/patch-${RT_PATCH_VERSION}.patch.xz

# Extract kernel
tar xf linux-${KERNEL_VERSION}.tar.xz
cd linux-${KERNEL_VERSION}

# Apply RT patch
xzcat ../patch-${RT_PATCH_VERSION}.patch.xz | patch -p1
```

#### Step 3: Configure Kernel

```bash
# Copy current config as base
cp /boot/config-$(uname -r) .config

# Update config for new kernel version
make olddefconfig

# Configure for full preemption
make menuconfig
```

In menuconfig, set:

```
General setup
  └─> Preemption Model
      └─> [*] Fully Preemptible Kernel (RT)  # Select this option

Processor type and features
  └─> [*] Timer frequency
      └─> 1000 Hz  # Select this for high resolution

Kernel hacking
  └─> [ ] Compile-time checks and compiler options
      └─> [ ] Debug preemptible kernel  # Disable for production
```

Save and exit.

#### Step 4: Compile and Install

```bash
# Compile kernel (use -j$(nproc) for parallel build)
make -j$(nproc) deb-pkg LOCALVERSION=-rt-custom

# This will take 30-60 minutes depending on your system
```

For Ubuntu/Debian, `.deb` packages are created in parent directory:

```bash
cd ..
sudo dpkg -i linux-image-*.deb linux-headers-*.deb

# Update GRUB
sudo update-grub

# Reboot
sudo reboot
```

For Fedora/RHEL, use `make rpm-pkg` instead.

#### Step 5: Verify Installation

After reboot:

```bash
uname -r
# Should show: 6.1.38-rt13-custom (or similar)

# Verify full preemption
cat /sys/kernel/realtime
# Should output: 1

# Check preemption mode
zcat /proc/config.gz | grep PREEMPT
# Should show: CONFIG_PREEMPT_RT=y
```

---

## CPU Core Isolation

CPU core isolation dedicates specific cores exclusively to RT tasks, preventing interference from general OS processes.

### Understanding Core Isolation

**Goal**: Reserve CPU cores for PhyServer, free from:
- General process scheduling
- Kernel threads
- Interrupts (where possible)
- RCU callbacks
- Timers

**Recommended Setup** (4-core system):
- **Cores 0-1**: General OS, background processes
- **Cores 2-3**: Isolated for PhyServer RT tasks

### Step 1: Identify CPU Topology

```bash
# List CPU cores
lscpu

# Show CPU topology
lscpu -e

# Output example:
# CPU NODE SOCKET CORE L1d:L1i:L2:L3 ONLINE
#   0    0      0    0 0:0:0:0          yes
#   1    0      0    1 1:1:1:0          yes
#   2    0      0    2 2:2:2:0          yes
#   3    0      0    3 3:3:3:0          yes
```

Choose cores to isolate (e.g., cores 2-3 on a 4-core system).

### Step 2: Configure GRUB Boot Parameters

Edit GRUB configuration:

```bash
sudo nano /etc/default/grub
```

Modify `GRUB_CMDLINE_LINUX_DEFAULT` or `GRUB_CMDLINE_LINUX`:

**Basic isolation (recommended starting point):**

```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3"
```

**Aggressive isolation (maximum performance):**

```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3 rcu_nocb_poll intel_pstate=disable processor.max_cstate=1 idle=poll"
```

### Step 3: Update GRUB and Reboot

**Ubuntu/Debian:**

```bash
sudo update-grub
sudo reboot
```

**Fedora/RHEL:**

```bash
sudo grub2-mkconfig -o /boot/grub2/grub.cfg
sudo reboot
```

### Step 4: Verify Isolation

After reboot:

```bash
# Check boot parameters
cat /proc/cmdline

# Verify isolated CPUs
cat /sys/devices/system/cpu/isolated
# Should output: 2-3

# Verify nohz_full
cat /sys/devices/system/cpu/nohz_full
# Should output: 2-3

# Check processes on isolated cores (should be minimal)
ps -eo psr,pid,comm | grep "^  2\|^  3"
```

---

## Boot Parameters Reference

### Core Isolation Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| `isolcpus=<list>` | Isolate CPUs from general scheduler | `isolcpus=2,3` or `isolcpus=2-5` |
| `nohz_full=<list>` | Disable periodic timer ticks on CPUs | `nohz_full=2,3` |
| `rcu_nocbs=<list>` | Offload RCU callbacks from CPUs | `rcu_nocbs=2,3` |

### Performance Parameters

| Parameter | Description | When to Use |
|-----------|-------------|-------------|
| `intel_pstate=disable` | Disable Intel P-State driver, use acpi-cpufreq | Intel CPUs, for manual frequency control |
| `processor.max_cstate=1` | Limit CPU C-states (prevent deep sleep) | Reduce wakeup latency, increases power usage |
| `idle=poll` | Poll instead of using halt instructions | Absolute minimum latency, high power usage |
| `nosoftlockup` | Disable soft lockup detector | Prevent false warnings on RT tasks |
| `nmi_watchdog=0` | Disable NMI watchdog | Reduce interrupts on isolated cores |

### Timer Frequency

| Parameter | Description | Recommendation |
|-----------|-------------|----------------|
| `clocksource=tsc` | Use TSC as clock source | Usually automatic, can force if needed |
| `tsc=reliable` | Mark TSC as reliable | Only if TSC is stable on your system |

### Complete Example (Aggressive)

For a 4-core system, isolating cores 2-3:

```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3 rcu_nocb_poll intel_pstate=disable processor.max_cstate=1 idle=poll nosoftlockup nmi_watchdog=0"
```

**Warning**: Aggressive settings increase power consumption and heat. Start with basic isolation and add parameters only if needed.

---

## System Service Configuration

### Disable IRQ Balance on Isolated Cores

IRQ balance distributes interrupts across cores. Prevent it from using isolated cores.

#### Method 1: Configuration File

```bash
sudo nano /etc/default/irqbalance
```

Add or modify:

```bash
# Banned CPUs (cores 2-3 in hex bitmask: 0x0C = 1100 binary)
IRQBALANCE_BANNED_CPUS=0c
```

Bitmask calculation:
- Core 0 = bit 0 = 0x01
- Core 1 = bit 1 = 0x02
- Core 2 = bit 2 = 0x04
- Core 3 = bit 3 = 0x08
- Cores 2+3 = 0x04 + 0x08 = 0x0C

For cores 4-7: `0xF0` (11110000 binary)

Restart irqbalance:

```bash
sudo systemctl restart irqbalance
```

#### Method 2: Disable IRQ Balance Completely

```bash
sudo systemctl stop irqbalance
sudo systemctl disable irqbalance
```

Then manually assign critical interrupts to non-isolated cores:

```bash
# Find IRQ numbers
cat /proc/interrupts

# Example: Assign USB controller IRQ 16 to core 0
echo 1 | sudo tee /proc/irq/16/smp_affinity
```

### Disable CPU Frequency Scaling

Frequency scaling can cause timing jitter. Lock CPUs to maximum frequency.

#### Method 1: Using cpupower (Recommended)

```bash
# Install cpupower
sudo apt install linux-tools-common linux-tools-$(uname -r)  # Ubuntu/Debian
sudo dnf install kernel-tools  # Fedora

# Set performance governor
sudo cpupower frequency-set -g performance

# Verify
cpupower frequency-info
```

Make permanent:

```bash
sudo nano /etc/default/cpupower
```

Set:

```bash
CPUPOWER_START_OPTS="frequency-set -g performance"
CPUPOWER_STOP_OPTS="frequency-set -g ondemand"
```

Enable service:

```bash
sudo systemctl enable cpupower
sudo systemctl start cpupower
```

#### Method 2: Disable Intel P-State (Alternative)

Add to GRUB parameters:

```bash
intel_pstate=disable
```

Then use cpufreq:

```bash
# Set performance governor for isolated cores
echo performance | sudo tee /sys/devices/system/cpu/cpu2/cpufreq/scaling_governor
echo performance | sudo tee /sys/devices/system/cpu/cpu3/cpufreq/scaling_governor

# Lock to maximum frequency
cat /sys/devices/system/cpu/cpu2/cpufreq/cpuinfo_max_freq | \
    sudo tee /sys/devices/system/cpu/cpu2/cpufreq/scaling_min_freq

cat /sys/devices/system/cpu/cpu3/cpufreq/cpuinfo_max_freq | \
    sudo tee /sys/devices/system/cpu/cpu3/cpufreq/scaling_min_freq
```

### Disable Transparent Huge Pages (THP)

THP can cause unpredictable latency:

```bash
# Disable THP
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/defrag
```

Make permanent by adding to `/etc/rc.local` or creating systemd service:

```bash
sudo nano /etc/systemd/system/disable-thp.service
```

```ini
[Unit]
Description=Disable Transparent Huge Pages
DefaultDependencies=no
After=sysinit.target local-fs.target
Before=basic.target

[Service]
Type=oneshot
ExecStart=/bin/sh -c 'echo never > /sys/kernel/mm/transparent_hugepage/enabled'
ExecStart=/bin/sh -c 'echo never > /sys/kernel/mm/transparent_hugepage/defrag'

[Install]
WantedBy=basic.target
```

Enable:

```bash
sudo systemctl daemon-reload
sudo systemctl enable disable-thp
sudo systemctl start disable-thp
```

### Configure USB Power Management

Disable USB autosuspend to prevent latency:

```bash
# Disable USB autosuspend globally
echo -1 | sudo tee /sys/module/usbcore/parameters/autosuspend

# Or per device (find USB device with lsusb)
# Example for device 1-1
echo on | sudo tee /sys/bus/usb/devices/1-1/power/control
```

Make permanent:

```bash
sudo nano /etc/modprobe.d/usb-no-autosuspend.conf
```

Add:

```
options usbcore autosuspend=-1
```

---

## Validation and Testing

### Verify RT Capabilities

```bash
# Check RT kernel
uname -r | grep rt
cat /sys/kernel/realtime  # Should output: 1

# Check preemption model
zcat /proc/config.gz 2>/dev/null | grep PREEMPT_RT || \
    grep PREEMPT /boot/config-$(uname -r)
# Should show: CONFIG_PREEMPT_RT=y

# Verify CPU isolation
cat /sys/devices/system/cpu/isolated  # Should show: 2-3 (or your isolated cores)
cat /proc/cmdline  # Should show isolcpus, nohz_full, rcu_nocbs

# Check CPU frequency governor
cpupower frequency-info | grep "current policy"
# Should show: performance
```

### Test Latency with cyclictest

Install rt-tests:

```bash
sudo apt install rt-tests  # Ubuntu/Debian
sudo dnf install rt-tests  # Fedora
```

Run latency test:

```bash
# Basic test (5 minutes on isolated cores)
sudo cyclictest -p 95 -t 2 -a 2,3 -m -n -D 5m

# Stress test (with system load)
sudo cyclictest -p 95 -t 2 -a 2,3 -m -n -D 10m &
stress-ng --cpu 2 --io 2 --vm 1 --vm-bytes 512M --timeout 600s
```

**Good Results**:
- Average latency: < 10 μs
- Maximum latency: < 100 μs
- Minimal outliers

**Poor Results** (indicates configuration issues):
- Average latency: > 50 μs
- Maximum latency: > 1000 μs (1 ms)
- Frequent spikes

### Test PhyServer Performance

Configure PhyServer with RT settings (see [CONFIGURATION.md](CONFIGURATION.md)):

```toml
[realtime]
enable = true
priority = 80
cpu_affinity = [2, 3]
```

Run PhyServer and monitor latency:

```bash
sudo ./physerver -c physerver.toml
```

Check metrics (if enabled):

```bash
# Query latency statistics
curl http://localhost:8080/metrics | grep latency
```

Expected results:
- **USB mode**: < 100 μs average latency, 10 kHz stable update rate
- **Serial mode**: < 1 ms average latency, 1 kHz stable update rate

---

## Troubleshooting

### Permission Denied Errors

**Symptom**: PhyServer fails with "Permission denied" when setting RT priority.

**Cause**: User lacks CAP_SYS_NICE capability.

**Solution 1**: Run with sudo (testing only):

```bash
sudo ./physerver
```

**Solution 2**: Set capabilities on binary:

```bash
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=ep ./physerver
```

**Solution 3**: Configure systemd service (see [DEPLOYMENT.md](DEPLOYMENT.md)):

```ini
[Service]
AmbientCapabilities=CAP_SYS_NICE CAP_IPC_LOCK CAP_SYS_ADMIN
```

### High Latency Despite RT Kernel

**Symptom**: cyclictest shows latency > 100 μs.

**Check**:

```bash
# Verify RT kernel is active
uname -r | grep rt

# Check if running on isolated cores
taskset -cp $(pgrep cyclictest)

# Verify CPU governor
cpupower frequency-info
```

**Solutions**:
1. Ensure CPU frequency governor is set to `performance`
2. Disable CPU frequency scaling completely
3. Add `processor.max_cstate=1 idle=poll` to boot parameters
4. Disable irqbalance on isolated cores

### Kernel Panic on Boot After Installing RT Kernel

**Symptom**: System fails to boot after installing RT kernel.

**Solution**:

1. Boot into previous kernel (select from GRUB menu)
2. Remove problematic RT kernel:

```bash
# Ubuntu/Debian
sudo apt remove linux-image-*-rt-*

# Fedora
sudo dnf remove kernel-rt
```

3. Try alternative installation method or kernel version

### USB Communication Timeouts

**Symptom**: PhyServer reports USB timeouts or dropped packets.

**Check**:

```bash
# Verify USB autosuspend is disabled
cat /sys/module/usbcore/parameters/autosuspend
# Should output: -1

# Check USB device power control
lsusb -t
# Find your Arduino Due device path (e.g., 1-1)
cat /sys/bus/usb/devices/1-1/power/control
# Should output: on
```

**Solutions**:

1. Disable USB autosuspend globally (see System Service Configuration)
2. Use dedicated USB 2.0 port (avoid hubs)
3. Assign USB controller IRQ to non-isolated core:

```bash
# Find USB IRQ
cat /proc/interrupts | grep -i usb

# Assign to core 0 (IRQ 16 example)
echo 1 | sudo tee /proc/irq/16/smp_affinity
```

### RCU Stalls or Soft Lockup Warnings

**Symptom**: Kernel log shows "rcu_sched detected stalls" or "soft lockup".

**Cause**: RT task consuming 100% CPU prevents RCU or watchdog from running.

**Solution**:

Add to boot parameters:

```bash
nosoftlockup rcu_nocb_poll
```

Or configure PhyServer to yield periodically (if issue persists).

### IRQs Still Hitting Isolated Cores

**Symptom**: `cat /proc/interrupts` shows IRQs on isolated cores.

**Check**:

```bash
# Check IRQ affinity
grep . /proc/irq/*/smp_affinity_list | grep -E " (2|3)$"
```

**Solution**:

Manually set affinity for problematic IRQs:

```bash
# Example: Move IRQ 25 to core 0
echo 0 | sudo tee /proc/irq/25/smp_affinity_list
```

Or disable irqbalance entirely (see System Service Configuration).

---

## Complete Setup Examples

### Example 1: Ubuntu 22.04 LTS (4-core Intel System)

**Goal**: Configure for USB mode (10 kHz) with cores 2-3 isolated.

#### Step 1: Install RT Kernel

```bash
sudo apt update
sudo apt install linux-lowlatency linux-headers-lowlatency
sudo reboot
```

#### Step 2: Configure GRUB

```bash
sudo nano /etc/default/grub
```

Set:

```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3 intel_pstate=disable"
```

Update and reboot:

```bash
sudo update-grub
sudo reboot
```

#### Step 3: Configure System Services

```bash
# Disable irqbalance on cores 2-3
sudo nano /etc/default/irqbalance
# Add: IRQBALANCE_BANNED_CPUS=0c

sudo systemctl restart irqbalance

# Install and configure cpupower
sudo apt install linux-tools-$(uname -r)
sudo cpupower frequency-set -g performance

# Make permanent
sudo nano /etc/default/cpupower
# Add: CPUPOWER_START_OPTS="frequency-set -g performance"

sudo systemctl enable cpupower

# Disable USB autosuspend
echo -1 | sudo tee /sys/module/usbcore/parameters/autosuspend
sudo nano /etc/modprobe.d/usb-no-autosuspend.conf
# Add: options usbcore autosuspend=-1

# Disable THP
sudo systemctl enable disable-thp  # (create service from earlier section)
```

#### Step 4: Configure PhyServer

Create `physerver.toml`:

```toml
[realtime]
enable = true
priority = 80
cpu_affinity = [2, 3]

[transport]
mode = "usb"
usb_vendor_id = 0x2341
usb_product_id = 0x003e

[server]
bind = "0.0.0.0:8080"
```

#### Step 5: Test

```bash
# Test latency
sudo apt install rt-tests
sudo cyclictest -p 95 -t 2 -a 2,3 -m -n -D 5m

# Expected: avg < 10 μs, max < 100 μs

# Run PhyServer
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=ep ./physerver
./physerver -c physerver.toml
```

### Example 2: Debian 12 (8-core AMD System, Serial Mode)

**Goal**: Serial mode (1 kHz) with cores 6-7 isolated.

#### Step 1: Install RT Kernel

```bash
sudo apt update
sudo apt install linux-image-rt-amd64 linux-headers-rt-amd64
sudo reboot
```

#### Step 2: Configure GRUB

```bash
sudo nano /etc/default/grub
```

Set:

```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet isolcpus=6,7 nohz_full=6,7 rcu_nocbs=6,7"
```

Update and reboot:

```bash
sudo update-grub
sudo reboot
```

#### Step 3: Configure CPU Performance

```bash
# Install cpupower
sudo apt install linux-cpupower

# Set performance governor
sudo cpupower frequency-set -g performance

# Lock cores 6-7 to max frequency
for core in 6 7; do
    MAX_FREQ=$(cat /sys/devices/system/cpu/cpu${core}/cpufreq/cpuinfo_max_freq)
    echo $MAX_FREQ | sudo tee /sys/devices/system/cpu/cpu${core}/cpufreq/scaling_min_freq
done
```

#### Step 4: Configure PhyServer

Create `physerver.toml`:

```toml
[realtime]
enable = true
priority = 70
cpu_affinity = [6, 7]

[transport]
mode = "serial"
serial_port = "/dev/ttyACM0"
serial_baud = 2000000

[server]
bind = "0.0.0.0:8080"
```

#### Step 5: Test

```bash
# Test latency on cores 6-7
sudo cyclictest -p 95 -t 2 -a 6,7 -m -n -D 5m

# Run PhyServer
sudo setcap cap_sys_nice,cap_ipc_lock=ep ./physerver
./physerver -c physerver.toml
```

### Example 3: Fedora 38 (Production Deployment)

**Goal**: Systemd service with RT kernel, cores 4-5 isolated.

#### Step 1: Install RT Kernel

```bash
sudo dnf install kernel-rt kernel-rt-devel
sudo reboot
```

#### Step 2: Configure GRUB

```bash
sudo nano /etc/default/grub
```

Set:

```bash
GRUB_CMDLINE_LINUX="isolcpus=4,5 nohz_full=4,5 rcu_nocbs=4,5 nosoftlockup"
```

Update and reboot:

```bash
sudo grub2-mkconfig -o /boot/grub2/grub.cfg
sudo reboot
```

#### Step 3: Create Systemd Service

```bash
sudo nano /etc/systemd/system/physerver.service
```

```ini
[Unit]
Description=PhyServer Real-Time Hardware Control
After=network.target
Requires=network.target

[Service]
Type=simple
User=physerver
Group=physerver
WorkingDirectory=/opt/physerver
ExecStart=/opt/physerver/physerver -c /opt/physerver/physerver.toml

# Real-time capabilities
AmbientCapabilities=CAP_SYS_NICE CAP_IPC_LOCK CAP_SYS_ADMIN
LimitMEMLOCK=infinity
LimitNICE=-20

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/physerver/logs

Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

#### Step 4: Configure and Start

```bash
# Create user and directories
sudo useradd -r -s /bin/false physerver
sudo mkdir -p /opt/physerver/logs
sudo chown -R physerver:physerver /opt/physerver

# Copy binary and config
sudo cp physerver /opt/physerver/
sudo cp physerver.toml /opt/physerver/

# Set capabilities
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=ep /opt/physerver/physerver

# Start service
sudo systemctl daemon-reload
sudo systemctl enable physerver
sudo systemctl start physerver

# Check status
sudo systemctl status physerver
```

---

## Additional Resources

### Related Documentation

- **[CONFIGURATION.md](CONFIGURATION.md)**: PhyServer RT configuration options
- **[DEPLOYMENT.md](DEPLOYMENT.md)**: Production deployment and systemd services
- **[PERFORMANCE.md](PERFORMANCE.md)**: Performance benchmarks and tuning
- **[USER_MANUAL.md](USER_MANUAL.md)**: Complete user guide

### External Resources

- **RT-Preempt Wiki**: https://wiki.linuxfoundation.org/realtime/start
- **RT-Preempt HOWTO**: https://wiki.linuxfoundation.org/realtime/documentation/howto
- **Kernel RT Patches**: https://kernel.org/pub/linux/kernel/projects/rt/
- **Linux Foundation RT Wiki**: https://wiki.linuxfoundation.org/realtime/documentation/start
- **cyclictest Documentation**: https://wiki.linuxfoundation.org/realtime/documentation/howto/tools/cyclictest

### Support

For issues or questions:
- **GitHub Issues**: https://github.com/angleto/phycmd/issues
- **Documentation**: See `DOCUMENTATION_INDEX.md` for all guides

---

## Quick Reference

### Verify RT Configuration Checklist

```bash
# 1. RT Kernel installed
uname -r | grep rt && cat /sys/kernel/realtime

# 2. CPU isolation active
cat /sys/devices/system/cpu/isolated

# 3. Boot parameters present
cat /proc/cmdline | grep -E "isolcpus|nohz_full|rcu_nocbs"

# 4. CPU governor set to performance
cpupower frequency-info | grep "current policy"

# 5. USB autosuspend disabled
cat /sys/module/usbcore/parameters/autosuspend

# 6. THP disabled
cat /sys/kernel/mm/transparent_hugepage/enabled | grep "\[never\]"

# 7. Test latency
sudo cyclictest -p 95 -t 2 -a 2,3 -m -n -D 60s
```

All checks should pass for optimal RT performance.
