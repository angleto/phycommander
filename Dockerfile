# Multi-stage Dockerfile for PhyServer
# Optimized for production deployment

# Build stage
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libudev-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy manifests
COPY physerver/Cargo.toml physerver/Cargo.lock ./

# Build dependencies (cache layer)
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY physerver/src ./src
COPY physerver/static ./static
COPY physerver/examples ./examples
COPY physerver/tests ./tests
COPY physerver/benches ./benches

# Build application
RUN cargo build --release --bin physerver

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libudev1 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false -u 1000 physerver && \
    usermod -a -G dialout physerver

# Create directories
RUN mkdir -p /etc/physerver /var/lib/physerver /var/log/physerver && \
    chown -R physerver:physerver /etc/physerver /var/lib/physerver /var/log/physerver

# Copy binary from builder
COPY --from=builder /build/target/release/physerver /usr/local/bin/physerver

# Copy static files
COPY --from=builder /build/static /opt/physerver/static

# Set capabilities for real-time and network access
RUN setcap cap_sys_nice,cap_ipc_lock,cap_net_bind_service=+eip /usr/local/bin/physerver

# Switch to non-root user
USER physerver

# Expose ports
EXPOSE 8080

# Environment variables
ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/physerver", "--help"] || exit 1

# Default command
CMD ["/usr/local/bin/physerver", "--config", "/etc/physerver/config.toml"]

# Labels
LABEL org.opencontainers.image.title="PhyServer"
LABEL org.opencontainers.image.description="Physical Commander - Real-time hardware control server"
LABEL org.opencontainers.image.version="1.0.0"
LABEL org.opencontainers.image.authors="PhyCMD Contributors"
LABEL org.opencontainers.image.licenses="MIT"
