# PhyCMD Documentation Index

Complete guide to PhyCMD documentation.

## Quick Navigation

### 🚀 Getting Started
1. Start here: [README.md](README.md)
2. Quick setup: [QUICK_REFERENCE.md](QUICK_REFERENCE.md)
3. Full user guide: [USER_MANUAL.md](USER_MANUAL.md)

### 📖 By Role

#### End Users
- **Start**: [USER_MANUAL.md](USER_MANUAL.md)
- **Quick Reference**: [QUICK_REFERENCE.md](QUICK_REFERENCE.md)
- **Setup**: [SETUP.md](SETUP.md)

#### Developers
- **API Reference**: [API_REFERENCE.md](API_REFERENCE.md)
- **Architecture**: [ARCHITECTURE.md](ARCHITECTURE.md)
- **Protocol**: [PROTOCOL.md](PROTOCOL.md)
- **Building**: [BUILDING.md](BUILDING.md)

#### DevOps/System Administrators
- **Deployment**: [DEPLOYMENT.md](DEPLOYMENT.md)
- **Configuration**: [CONFIGURATION.md](CONFIGURATION.md)
- **Performance**: [PERFORMANCE.md](PERFORMANCE.md)

#### Hardware/Firmware Engineers
- **Firmware Upload**: [FIRMWARE_UPLOAD.md](FIRMWARE_UPLOAD.md)
- **Firmware Updates**: [FIRMWARE_UPDATES.md](FIRMWARE_UPDATES.md)
- **Protocol**: [PROTOCOL.md](PROTOCOL.md)

---

## Documentation by Category

### Core Documentation

| Document | Description | Length |
|----------|-------------|--------|
| [README.md](README.md) | Main entry point and overview | 10 pages |
| [USER_MANUAL.md](USER_MANUAL.md) | Complete user guide | 60 pages |
| [QUICK_REFERENCE.md](QUICK_REFERENCE.md) | One-page cheat sheet | 3 pages |
| [API_REFERENCE.md](API_REFERENCE.md) | Complete API documentation | 40 pages |

### Setup & Configuration

| Document | Description | Length |
|----------|-------------|--------|
| [SETUP.md](SETUP.md) | Initial setup and installation | 15 pages |
| [BUILDING.md](BUILDING.md) | Build instructions | 20 pages |
| [CONFIGURATION.md](CONFIGURATION.md) | Configuration guide | 30 pages |
| [DEPLOYMENT.md](DEPLOYMENT.md) | Production deployment | 35 pages |

### Technical Reference

| Document | Description | Length |
|----------|-------------|--------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | System design and architecture | 25 pages |
| [PROTOCOL.md](PROTOCOL.md) | PhyCMD-64 protocol specification | 20 pages |
| [PERFORMANCE.md](PERFORMANCE.md) | Performance benchmarks and tuning | 15 pages |
| [FIRMWARE_UPLOAD.md](FIRMWARE_UPLOAD.md) | Firmware upload guide (BOSSA) | 15 pages |
| [FIRMWARE_UPDATES.md](FIRMWARE_UPDATES.md) | Required firmware changes | 10 pages |

**Total**: ~200+ pages of documentation

---

## Documentation by Task

### Installation & Setup

1. [SETUP.md](SETUP.md) - Initial setup
2. [BUILDING.md](BUILDING.md) - Build from source
3. [FIRMWARE_UPLOAD.md](FIRMWARE_UPLOAD.md) - Upload firmware
4. [DEPLOYMENT.md](DEPLOYMENT.md) - Production deployment

### Configuration

1. [CONFIGURATION.md](CONFIGURATION.md) - Complete configuration reference
2. [QUICK_REFERENCE.md](QUICK_REFERENCE.md) - Quick config examples
3. [DEPLOYMENT.md](DEPLOYMENT.md) - Production config

### Using PhyServer

1. [USER_MANUAL.md](USER_MANUAL.md) - Complete usage guide
2. [QUICK_REFERENCE.md](QUICK_REFERENCE.md) - Common operations
3. [API_REFERENCE.md](API_REFERENCE.md) - API details

### Development

1. [API_REFERENCE.md](API_REFERENCE.md) - API reference
2. [ARCHITECTURE.md](ARCHITECTURE.md) - System design
3. [PROTOCOL.md](PROTOCOL.md) - Protocol specification
4. [BUILDING.md](BUILDING.md) - Build instructions

### Performance Tuning

1. [PERFORMANCE.md](PERFORMANCE.md) - Benchmarks and tuning
2. [CONFIGURATION.md](CONFIGURATION.md) - RT configuration
3. [DEPLOYMENT.md](DEPLOYMENT.md) - Production optimization
4. [USER_MANUAL.md](USER_MANUAL.md) - Real-time operation section

### Troubleshooting

1. [USER_MANUAL.md](USER_MANUAL.md) - Troubleshooting section
2. [QUICK_REFERENCE.md](QUICK_REFERENCE.md) - Quick fixes
3. [DEPLOYMENT.md](DEPLOYMENT.md) - Production troubleshooting
4. [SETUP.md](SETUP.md) - Setup issues

---

## Documentation Features

### Code Examples

All documentation includes working code examples in:
- **Bash/cURL**: Command-line operations
- **Python**: REST API and automation
- **JavaScript**: Web interface and WebSocket
- **Rust**: IPC and native integration

### Practical Scenarios

- Development setup
- Production deployment
- High-performance configuration
- Multi-instance deployment
- Remote access setup
- Security hardening

### Complete Coverage

- ✅ Installation procedures
- ✅ Configuration options
- ✅ API endpoints
- ✅ Command-line tools
- ✅ Web interface
- ✅ IPC interface
- ✅ Troubleshooting
- ✅ Performance tuning
- ✅ Security
- ✅ Monitoring
- ✅ Backup/recovery

---

## Search Guide

### By Keyword

**Transport/Communication**:
- USB Bulk transport → [PERFORMANCE.md](PERFORMANCE.md), [USER_MANUAL.md](USER_MANUAL.md)
- Serial transport → [CONFIGURATION.md](CONFIGURATION.md), [USER_MANUAL.md](USER_MANUAL.md)
- Protocol → [PROTOCOL.md](PROTOCOL.md), [ARCHITECTURE.md](ARCHITECTURE.md)

**APIs**:
- REST API → [API_REFERENCE.md](API_REFERENCE.md), [USER_MANUAL.md](USER_MANUAL.md)
- WebSocket → [API_REFERENCE.md](API_REFERENCE.md)
- IPC → [API_REFERENCE.md](API_REFERENCE.md), [USER_MANUAL.md](USER_MANUAL.md)

**Configuration**:
- TOML config → [CONFIGURATION.md](CONFIGURATION.md)
- Command-line → [QUICK_REFERENCE.md](QUICK_REFERENCE.md), [CONFIGURATION.md](CONFIGURATION.md)
- Real-time → [CONFIGURATION.md](CONFIGURATION.md), [USER_MANUAL.md](USER_MANUAL.md)

**Deployment**:
- Systemd → [DEPLOYMENT.md](DEPLOYMENT.md)
- Security → [DEPLOYMENT.md](DEPLOYMENT.md)
- Monitoring → [DEPLOYMENT.md](DEPLOYMENT.md)

**Hardware**:
- Arduino Due → [README.md](README.md), [FIRMWARE_UPLOAD.md](FIRMWARE_UPLOAD.md)
- Pin mapping → [USER_MANUAL.md](USER_MANUAL.md), [PROTOCOL.md](PROTOCOL.md)
- Firmware → [FIRMWARE_UPLOAD.md](FIRMWARE_UPLOAD.md), [FIRMWARE_UPDATES.md](FIRMWARE_UPDATES.md)

---

## Documentation Structure

```
PhyCMD/
│
├── README.md                  # Start here
│
├── Core Documentation
│   ├── USER_MANUAL.md        # Complete user guide
│   ├── QUICK_REFERENCE.md    # Quick reference
│   └── API_REFERENCE.md      # API documentation
│
├── Setup & Configuration
│   ├── SETUP.md              # Initial setup
│   ├── BUILDING.md           # Build instructions
│   ├── CONFIGURATION.md      # Configuration reference
│   └── DEPLOYMENT.md         # Production deployment
│
├── Technical Reference
│   ├── ARCHITECTURE.md       # System architecture
│   ├── PROTOCOL.md           # Protocol specification
│   ├── PERFORMANCE.md        # Performance guide
│   ├── FIRMWARE_UPLOAD.md    # Firmware upload
│   └── FIRMWARE_UPDATES.md   # Firmware updates
│
└── Code
    └── physerver/            # Server implementation
        ├── src/              # Source code
        ├── examples/         # Code examples
        └── static/           # Web interface
```

---

## Recommended Reading Order

### For New Users

1. [README.md](README.md) - Overview
2. [SETUP.md](SETUP.md) - Get it running
3. [USER_MANUAL.md](USER_MANUAL.md) - Learn to use it
4. [QUICK_REFERENCE.md](QUICK_REFERENCE.md) - Keep handy

### For Developers

1. [README.md](README.md) - Overview
2. [ARCHITECTURE.md](ARCHITECTURE.md) - Understand design
3. [PROTOCOL.md](PROTOCOL.md) - Understand protocol
4. [API_REFERENCE.md](API_REFERENCE.md) - Use APIs
5. [BUILDING.md](BUILDING.md) - Build from source

### For System Administrators

1. [README.md](README.md) - Overview
2. [DEPLOYMENT.md](DEPLOYMENT.md) - Deploy to production
3. [CONFIGURATION.md](CONFIGURATION.md) - Configure properly
4. [PERFORMANCE.md](PERFORMANCE.md) - Optimize performance

### For Integration Engineers

1. [README.md](README.md) - Overview
2. [API_REFERENCE.md](API_REFERENCE.md) - APIs
3. [PROTOCOL.md](PROTOCOL.md) - Protocol
4. [ARCHITECTURE.md](ARCHITECTURE.md) - Architecture

---

## Version Information

All documentation is for **PhyCMD v1.0.0**

Last updated: **2025-11-22**

---

## Contributing

When updating documentation:

1. **Keep consistent style** across all docs
2. **Include code examples** for all features
3. **Test all examples** before committing
4. **Update this index** when adding new docs
5. **Cross-reference** related documents

---

## External Resources

- **GitHub Repository**: https://github.com/your-org/phycmd
- **Issue Tracker**: https://github.com/your-org/phycmd/issues
- **Arduino Due**: https://www.arduino.cc/en/Guide/ArduinoDue
- **BOSSA**: https://www.shumatech.com/web/products/bossa
- **Rust**: https://www.rust-lang.org/

---

**For questions or feedback, please open an issue on GitHub.**
