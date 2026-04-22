# Contributing to PhyCMD

Thank you for your interest in contributing to PhyCMD! This document provides guidelines and instructions for contributing.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Getting Started](#getting-started)
3. [Development Setup](#development-setup)
4. [Making Changes](#making-changes)
5. [Testing](#testing)
6. [Documentation](#documentation)
7. [Pull Request Process](#pull-request-process)
8. [Style Guidelines](#style-guidelines)

---

## Code of Conduct

- Be respectful and constructive
- Welcome newcomers and help them get started
- Focus on what is best for the project and community
- Show empathy towards other community members

---

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Linux: libudev-dev and pkg-config
- Git
- Arduino Due (for hardware testing)

### Clone the Repository

```bash
git clone https://github.com/angleto/phycommander.git
cd phycommander
```

---

## Development Setup

### 1. Install Dependencies

**Ubuntu/Debian**:
```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libudev-dev
```

### 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 3. Build the Project

```bash
cd physerver
cargo build
```

### 4. Run Tests

```bash
# Host-only unit + integration tests (no hardware required).
# This is what CI runs.
cargo test --release --workspace
```

#### Hardware selftests (Arduino Due loopback required)

The `physerver/tests/selftest.rs` suite lives behind `#[ignore]` so
CI and casual contributors don't run it. It needs physical loopback
wiring (DAC0↔ADC0, DAC1↔ADC1, DOUT[0..15]↔DIN[0..15]) on a flashed
PhyCommander connected over USB. To run:

```bash
# All hardware selftests
cargo test --release --test selftest -- --ignored

# One sub-suite at a time
cargo test --release --test selftest gpio   -- --ignored
cargo test --release --test selftest analog -- --ignored

# Pick a specific serial port (otherwise auto-detect picks the first
# ACM device on the host — fine for a one-Due desk, wrong if you have
# multiple).
PHYCMD_PORT=/dev/ttyACM0 cargo test --release --test selftest -- --ignored
```

Expect the test run to take 10-30 s per sub-suite because of the
DAC/ADC settling waits and digital scan. If any assertion fires, the
firmware may not match the host-side protocol layout — the
`const _: () = assert!(offset_of!(...))` compile-time checks in
`protocol/types.rs` are the first thing to verify.

### 5. Run the Server

```bash
cargo run -- --auto-detect
```

---

## Making Changes

### Branching Strategy

- `main` - Stable releases
- `develop` - Development branch
- `feature/*` - New features
- `bugfix/*` - Bug fixes
- `docs/*` - Documentation updates

### Creating a Feature Branch

```bash
git checkout develop
git pull origin develop
git checkout -b feature/your-feature-name
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add WebSocket authentication
fix: resolve serial port detection issue
docs: update API reference for GPIO endpoints
test: add integration tests for protocol codec
refactor: simplify transport abstraction
perf: optimize CRC calculation
chore: update dependencies
```

Examples:
- `feat(web): add health check endpoint`
- `fix(transport): handle USB disconnection gracefully`
- `docs(config): clarify real-time settings`

---

## Testing

### Running Tests

```bash
# All tests
cargo test

# Specific module
cargo test protocol

# With output
cargo test -- --nocapture

# Integration tests
cargo test --test integration_test
```

### Writing Tests

- Add unit tests in the same file as the code
- Add integration tests in `tests/` directory
- Test both success and failure cases
- Use descriptive test names

Example:
```rust
#[test]
fn test_encode_command_with_valid_data() {
    let cmd = Command { /* ... */ };
    let bytes = encode_command(&cmd);
    assert_eq!(bytes.len(), 64);
    assert_eq!(bytes[0], 0x55);
}

#[test]
fn test_decode_status_with_invalid_crc() {
    let data = [/* invalid data */];
    let result = decode_status(&data);
    assert!(result.is_err());
}
```

---

## Documentation

### Code Documentation

Use rustdoc comments for public APIs:

```rust
/// Encodes a command into a 64-byte message.
///
/// # Arguments
///
/// * `cmd` - The command to encode
///
/// # Returns
///
/// A 64-byte array containing the encoded message
///
/// # Examples
///
/// ```
/// let cmd = Command::default();
/// let bytes = encode_command(&cmd);
/// assert_eq!(bytes.len(), 64);
/// ```
pub fn encode_command(cmd: &Command) -> [u8; 64] {
    // ...
}
```

### Markdown Documentation

- Update relevant .md files when adding features
- Include code examples
- Test all code examples
- Cross-reference related documents

---

## Pull Request Process

### Before Submitting

1. **Ensure all tests pass**:
   ```bash
   cargo test
   ```

2. **Check formatting**:
   ```bash
   cargo fmt -- --check
   ```

3. **Run clippy**:
   ```bash
   cargo clippy -- -D warnings
   ```

4. **Update documentation** if needed

5. **Update CHANGELOG.md** with your changes

### Submitting a Pull Request

1. **Push your branch**:
   ```bash
   git push origin feature/your-feature-name
   ```

2. **Create a pull request** on GitHub

3. **Fill out the PR template**:
   - Description of changes
   - Related issue (if any)
   - Testing performed
   - Documentation updated

4. **Address review feedback**

5. **Wait for CI to pass**

### PR Review Process

- At least one maintainer approval required
- All CI checks must pass
- Code must be formatted and pass clippy
- Tests must pass
- Documentation must be updated

---

## Style Guidelines

### Rust Code Style

Follow standard Rust conventions:

```rust
// Use snake_case for functions and variables
fn calculate_crc(data: &[u8]) -> u16 {
    // ...
}

// Use CamelCase for types
struct CommandMessage {
    // ...
}

// Use SCREAMING_SNAKE_CASE for constants
const MESSAGE_SIZE: usize = 64;

// Prefer explicit types when clarity is important
let header: u16 = 0xAA55;

// Use Result for error handling
fn parse_data(data: &[u8]) -> Result<Status> {
    // ...
}

// Prefer early returns
if data.len() != 64 {
    return Err(ProtocolError::InvalidLength { /* ... */ });
}

// Use meaningful variable names
let calculated_crc = crc16_ccitt(&data[0..24]);
```

### Error Handling

- Use `anyhow::Result` for application errors
- Use custom error types (thiserror) for library code
- Provide context with `.context()`
- Don't panic in library code

### Performance

- Avoid unnecessary allocations
- Use `&str` instead of `String` when possible
- Use zero-copy where appropriate
- Profile before optimizing

---

## Architecture Guidelines

### Adding a New Transport

1. Implement the `Transport` trait
2. Add to `transport/mod.rs`
3. Update configuration
4. Add tests
5. Update documentation

### Adding a New API Endpoint

1. Add route in `web/mod.rs`
2. Implement handler function
3. Add to OpenAPI/documentation
4. Add tests
5. Update API_REFERENCE.md

### Modifying the Protocol

1. Update protocol specification in PROTOCOL.md
2. Update types in `protocol/types.rs`
3. Update codec in `protocol/codec.rs`
4. Update firmware (FIRMWARE_UPDATES.md)
5. Add backward compatibility if needed
6. Update tests

---

## Questions?

- Open an issue for bugs or feature requests
- Start a discussion for questions
- Check existing documentation
- Ask in pull request comments

---

## Thank You!

Your contributions make PhyCMD better for everyone. We appreciate your time and effort!
