#!/bin/bash
# Deployment script for PhyServer

set -e  # Exit on error

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Functions
info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

check_dependencies() {
    info "Checking dependencies..."

    if ! command -v cargo &> /dev/null; then
        error "Rust/Cargo not found. Install from https://rustup.rs/"
    fi

    if ! command -v systemctl &> /dev/null; then
        warn "systemd not found. Systemd service installation will be skipped."
    fi

    info "Dependencies OK"
}

build_release() {
    info "Building release binary..."
    cd "$PROJECT_ROOT/physerver"
    cargo build --release
    info "Build complete"
}

run_tests() {
    info "Running tests..."
    cd "$PROJECT_ROOT/physerver"
    cargo test --release
    info "Tests passed"
}

install_binary() {
    info "Installing binary..."
    sudo cp "$PROJECT_ROOT/physerver/target/release/physerver" /usr/local/bin/
    sudo chmod +x /usr/local/bin/physerver
    info "Binary installed to /usr/local/bin/physerver"
}

setup_directories() {
    info "Setting up directories..."
    sudo mkdir -p /etc/physerver
    sudo mkdir -p /var/lib/physerver
    sudo mkdir -p /var/log/physerver
    info "Directories created"
}

install_config() {
    info "Installing configuration..."
    if [ ! -f /etc/physerver/config.toml ]; then
        sudo cp "$PROJECT_ROOT/physerver/config.example.toml" /etc/physerver/config.toml
        info "Configuration installed (edit /etc/physerver/config.toml)"
    else
        warn "Configuration already exists, skipping"
    fi
}

setup_permissions() {
    info "Setting up permissions..."

    # Add user to dialout group
    if groups $USER | grep -q dialout; then
        info "User already in dialout group"
    else
        sudo usermod -a -G dialout $USER
        warn "Added to dialout group. Log out and back in for this to take effect"
    fi

    # Set capabilities
    sudo setcap cap_sys_nice,cap_ipc_lock,cap_sys_admin=eip /usr/local/bin/physerver
    info "Capabilities set"
}

install_udev_rules() {
    info "Installing udev rules..."
    sudo tee /etc/udev/rules.d/99-arduino-due.rules > /dev/null <<EOF
# Arduino Due (Programming Port)
SUBSYSTEM=="usb", ATTR{idVendor}=="2341", ATTR{idProduct}=="003e", MODE="0666", GROUP="dialout"

# Arduino Due (Native USB Port)
SUBSYSTEM=="usb", ATTR{idVendor}=="2341", ATTR{idProduct}=="003d", MODE="0666", GROUP="dialout"
EOF

    sudo udevadm control --reload-rules
    sudo udevadm trigger
    info "Udev rules installed"
}

install_systemd_service() {
    if ! command -v systemctl &> /dev/null; then
        warn "systemd not available, skipping service installation"
        return
    fi

    info "Installing systemd service..."
    sudo tee /etc/systemd/system/physerver.service > /dev/null <<EOF
[Unit]
Description=PhyServer - Physical Commander Server
Documentation=https://github.com/your-org/phycmd
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=$USER
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

# Real-time scheduling
LimitMEMLOCK=infinity
LimitNICE=-20

# Capabilities
AmbientCapabilities=CAP_SYS_NICE CAP_IPC_LOCK CAP_SYS_ADMIN

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    info "Systemd service installed"
}

enable_service() {
    if ! command -v systemctl &> /dev/null; then
        return
    fi

    info "Enabling and starting service..."
    sudo systemctl enable physerver
    sudo systemctl start physerver
    sudo systemctl status physerver --no-pager
}

# Main deployment
main() {
    info "Starting PhyServer deployment..."

    check_dependencies
    build_release
    run_tests
    setup_directories
    install_binary
    install_config
    setup_permissions
    install_udev_rules
    install_systemd_service

    info "Deployment complete!"
    echo ""
    info "Next steps:"
    echo "  1. Edit /etc/physerver/config.toml"
    echo "  2. Connect Arduino Due via USB"
    echo "  3. Start service: sudo systemctl start physerver"
    echo "  4. Check logs: sudo journalctl -u physerver -f"
    echo "  5. Access web interface: http://localhost:8080"

    read -p "Enable and start service now? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        enable_service
    fi
}

# Run
main
