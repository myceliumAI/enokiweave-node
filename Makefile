.PHONY: node1 node2 node3 network clean build test check fmt clippy run doc watch kill-nodes

# Default ports for nodes
PORT1 = 8001
PORT2 = 8002
PORT3 = 8003

# Logging configuration
LOG_CONFIG = RUST_LOG=off,enokiweave=debug,libp2p_swarm::behaviour=off,libp2p_gossipsub::behaviour=off,libp2p_tcp=off,Swarm::poll=off

# Build commands
build:
	cargo build

build-release:
	cargo build --release

# Development commands
check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy

test:
	cargo test

test-nocapture:
	cargo test -- --nocapture

doc:
	cargo doc --no-deps --open

watch:
	cargo watch -x check -x test

# Run commands
run:
	cargo run

run-release:
	cargo run --release

# Node commands
node1: build
	$(LOG_CONFIG) cargo run -- standalone $(PORT1)

node2: build
	$(LOG_CONFIG) cargo run -- peer $(PORT2) "/ip4/127.0.0.1/tcp/$(PORT1)"

node3: build
	$(LOG_CONFIG) cargo run -- peer $(PORT3) "/ip4/127.0.0.1/tcp/$(PORT2)"

# Start all nodes in separate terminals
network: build
	@echo "Starting network..."
	@gnome-terminal -- bash -c "$(LOG_CONFIG) make node1; exec bash" &
	@sleep 2
	@gnome-terminal -- bash -c "$(LOG_CONFIG) make node2; exec bash" &
	@sleep 2
	@gnome-terminal -- bash -c "$(LOG_CONFIG) make node3; exec bash" &

# Kill any processes running on node ports
kill-nodes:
	@echo "Killing processes on ports $(PORT1), $(PORT2), and $(PORT3)..."
	-@lsof -ti:$(PORT1) | xargs kill -9 2>/dev/null || true
	-@lsof -ti:$(PORT2) | xargs kill -9 2>/dev/null || true
	-@lsof -ti:$(PORT3) | xargs kill -9 2>/dev/null || true
	@echo "Done killing processes."

# Clean commands
clean:
	cargo clean

clean-all: clean
	rm -rf Cargo.lock
	rm -rf target/

# Help command
help:
	@echo "Available commands:"
	@echo ""
	@echo "Build commands:"
	@echo "  make build         - Build the project in debug mode"
	@echo "  make build-release - Build the project in release mode"
	@echo ""
	@echo "Development commands:"
	@echo "  make check         - Check the project for errors"
	@echo "  make fmt           - Format the code"
	@echo "  make clippy        - Run clippy lints"
	@echo "  make test          - Run tests"
	@echo "  make test-nocapture- Run tests with output"
	@echo "  make doc           - Generate and open documentation"
	@echo "  make watch         - Watch for changes and run checks"
	@echo ""
	@echo "Run commands:"
	@echo "  make run           - Run the project"
	@echo "  make run-release   - Run the project in release mode"
	@echo ""
	@echo "Node commands:"
	@echo "  make node1         - Start bootstrap node on port 8001"
	@echo "  make node2         - Start node2 on port 8002, connecting to node1"
	@echo "  make node3         - Start node3 on port 8003, connecting to node2"
	@echo "  make network       - Start all nodes in separate terminals"
	@echo "  make kill-nodes    - Kill all processes running on node ports"
	@echo ""
	@echo "Clean commands:"
	@echo "  make clean         - Clean build artifacts"
	@echo "  make clean-all     - Clean everything including Cargo.lock" 