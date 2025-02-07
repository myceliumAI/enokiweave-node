# Build stage
FROM rust:1.75-slim-bullseye as builder

# Install system dependencies
RUN apt-get update && \
    apt-get install -y \
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
    apt-get install -y \
    liblmdb0 \
    && rm -rf /var/lib/apt/lists/*

# Copy the build artifacts from builder
COPY --from=builder /usr/src/enokiweave/target/release/enokiweave /usr/local/bin/
COPY --from=builder /usr/src/enokiweave/target/release/build-transaction /usr/local/bin/

# Copy configuration files
COPY setup/example_genesis_file.json /etc/enokiweave/genesis.json
COPY setup/example_initial_peers_file.txt /etc/enokiweave/peers.txt

# Create data directory
RUN mkdir -p /var/lib/enokiweave

# Set working directory
WORKDIR /var/lib/enokiweave

# Expose ports
EXPOSE 3001

# Set entrypoint
ENTRYPOINT ["enokiweave"]
CMD ["--genesis-file-path", "/etc/enokiweave/genesis.json", "--rpc_port", "3001"] 