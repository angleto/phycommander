# Building PhyServer

## System Requirements

### Linux (Ubuntu/Debian)

```bash
# Install required dependencies
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libudev-dev
```

**Why libudev-dev is required:**
- Required by `serialport` crate for serial/USB CDC communication
- Required by `rusb` crate for direct USB bulk transfers (when `usb` feature is enabled)
- This is a Linux-only requirement

### macOS

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# No additional system dependencies required
```

### Windows

```bash
# Install Rust (if not already installed)
# Download from: https://rustup.rs/

# For USB support, you may need to install USB drivers using Zadig
# See: https://github.com/libusb/libusb/wiki/Windows
```

## Build Features

PhyServer supports conditional compilation features:

### Default Build (All Features)

```bash
cd physerver
cargo build --release
```

This includes:
- Serial transport (USB CDC)
- Direct USB bulk transfer (`usb` feature)
- All other functionality

### Serial-Only Build

```bash
cargo build --release --no-default-features
```

**Note**: On Linux, this still requires `libudev-dev` because the `serialport` crate depends on it.

### Feature Flags

- `usb` (default): Enable direct USB bulk transfer support via `rusb`
  - Provides ~10x better performance than serial
  - Requires `libudev-dev` on Linux
  - VID:PID = 0x2341:0x003e (Arduino Due)

## Build Profiles

### Debug Build

```bash
cargo build
```

- Faster compilation
- Includes debug symbols
- Lower optimization
- Located in: `target/debug/physerver`

### Release Build (Recommended)

```bash
cargo build --release
```

- Maximum optimization
- LTO enabled
- Strip debug symbols
- Single codegen unit for better optimization
- Located in: `target/release/physerver`

## Running Tests

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration_test

# All tests
cargo test --workspace
```

## Troubleshooting

### Error: "libudev not found"

**Linux:**
```bash
sudo apt-get install libudev-dev pkg-config
```

**Verify installation:**
```bash
pkg-config --modversion libudev
```

### Error: "Permission denied" when accessing serial port

**Linux:**
```bash
# Add user to dialout group
sudo usermod -a -G dialout $USER

# Log out and back in for changes to take effect
```

### Error: "USB device not found"

**Check device connection:**
```bash
# List USB devices
lsusb | grep -i arduino

# List serial ports
ls /dev/ttyACM*
```

**Check permissions:**
```bash
# Grant USB access (temporary)
sudo chmod 666 /dev/ttyACM0

# Or create udev rule (permanent)
echo 'SUBSYSTEM=="usb", ATTR{idVendor}=="2341", ATTR{idProduct}=="003e", MODE="0666"' | sudo tee /etc/udev/rules.d/99-arduino-due.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

## Cross-Compilation

### For ARM (Raspberry Pi, etc.)

```bash
# Install cross-compilation toolchain
sudo apt-get install gcc-arm-linux-gnueabihf

# Add target
rustup target add armv7-unknown-linux-gnueabihf

# Build
cargo build --release --target armv7-unknown-linux-gnueabihf
```

### For Windows (from Linux)

```bash
# Install MinGW
sudo apt-get install gcc-mingw-w64

# Add target
rustup target add x86_64-pc-windows-gnu

# Build
cargo build --release --target x86_64-pc-windows-gnu
```

## Installation

### System-wide Installation

```bash
# Build
cargo build --release

# Install to /usr/local/bin
sudo cp target/release/physerver /usr/local/bin/

# Verify
physerver --version
```

### User Installation

```bash
# Build
cargo build --release

# Install to ~/.local/bin
mkdir -p ~/.local/bin
cp target/release/physerver ~/.local/bin/

# Add to PATH (add to ~/.bashrc or ~/.zshrc)
export PATH="$HOME/.local/bin:$PATH"

# Verify
physerver --version
```

## Real-Time Capabilities

For real-time performance, grant necessary capabilities:

```bash
# Grant real-time scheduling and memory locking capabilities
sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip target/release/physerver

# Verify
getcap target/release/physerver
```

## Systemd Service

Create `/etc/systemd/system/physerver.service`:

```ini
[Unit]
Description=PhyServer - Physical Commander Server
After=network.target

[Service]
Type=simple
User=your_username
Group=dialout
ExecStart=/usr/local/bin/physerver --config /etc/physerver/config.toml
Restart=always
RestartSec=5

# Real-time scheduling
LimitMEMLOCK=infinity
LimitNICE=-20

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable physerver
sudo systemctl start physerver
sudo systemctl status physerver
```

## Performance Tuning

### CPU Governor

```bash
# Set to performance mode
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

### Kernel Parameters

For hard real-time, add to kernel command line (`/etc/default/grub`):

```
GRUB_CMDLINE_LINUX="isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3"
```

Then:

```bash
sudo update-grub
sudo reboot
```

## Development Build

For development with hot-reload:

```bash
# Install cargo-watch
cargo install cargo-watch

# Auto-rebuild on changes
cargo watch -x 'build' -x 'test'

# Auto-rebuild and run
cargo watch -x 'run -- --auto-detect'
```

## Documentation

Build and view documentation:

```bash
# Build docs
cargo doc --open --no-deps

# With private items
cargo doc --open --no-deps --document-private-items
```

## Size Optimization

For embedded systems with limited storage:

```toml
# Add to Cargo.toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

Then build:

```bash
cargo build --release
strip target/release/physerver  # Further reduce size
```

## Benchmarking

```bash
# Run benchmarks
cargo bench

# Specific benchmark
cargo bench --bench transport_benchmark
```

## Clean Build

```bash
# Remove build artifacts
cargo clean

# Clean and rebuild
cargo clean && cargo build --release
```

## Build Cache

Speed up builds using `sccache`:

```bash
# Install sccache
cargo install sccache

# Configure
export RUSTC_WRAPPER=sccache

# Build (will cache compilation results)
cargo build --release
```

## References

- [Rust Installation](https://rustup.rs/)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [Cross-Compilation](https://rust-lang.github.io/rustup/cross-compilation.html)
- [Linux Real-Time](https://wiki.linuxfoundation.org/realtime/)
