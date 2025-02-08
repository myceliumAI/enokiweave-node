# Build stage
FROM rust:1.75-slim-bullseye as builder

# Install system dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    pkg-config \
    liblmdb-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty shell project
WORKDIR /usr/src/enokiweave

# Copy manifests
COPY Cargo.lock Cargo.toml ./

# Copy source code
COPY src ./src
COPY setup ./setup

# Build for release
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    liblmdb0 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r enoki && useradd -r -g enoki enoki

# Create necessary directories and set permissions
RUN mkdir -p /var/lib/enokiweave /etc/enokiweave && \
    chown -R enoki:enoki /var/lib/enokiweave /etc/enokiweave

# Copy the build artifacts from builder
COPY --from=builder /usr/src/enokiweave/target/release/enokiweave /usr/local/bin/
COPY --from=builder /usr/src/enokiweave/target/release/build-transaction /usr/local/bin/

# Copy configuration files
COPY setup/example_genesis_file.json /etc/enokiweave/genesis.json
COPY setup/example_initial_peers_file.txt /etc/enokiweave/peers.txt

# Set working directory
WORKDIR /var/lib/enokiweave

# Switch to non-root user
USER enoki

# Expose ports
EXPOSE 3001

# Add healthcheck
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3001/health || exit 1

# Set entrypoint
ENTRYPOINT ["enokiweave"]
CMD ["--genesis-file-path", "/etc/enokiweave/genesis.json", "--rpc_port", "3001"] 