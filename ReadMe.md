# Enokiweave

Enokiweave is a block-lattice cryptocurrency designed for sub-second transactions with zero fees. Built to deliver value transfer without compromising on speed or cost.

## Key Features

- **Zero Fees**: All transactions are completely free
- **Sub-second Finality**: Transactions confirm in milliseconds
- **Block-lattice Architecture**: Individual account chains enable parallel processing and swift finality
- **Minimal Resource Usage**: Efficient consensus mechanism keeps network operation lean and sustainable

## How It Works

Enokiweave uses a block-lattice structure where each account operates its own blockchain. This allows for:

- Immediate transaction processing without global consensus bottlenecks
- Parallel validation of transactions across different account chains
- No mining or transaction fees
- Ultra-low latency finalization

## High-level architecture
![Architecture](./assets/architecture.png)

## Getting Started

# Clone the repository
```bash
git clone https://github.com/enokiweave/enokiweave
```

# Run the node
```bash
cargo run --bin enokiweave -- --genesis-file-path ./setup/example_genesis_file.json --rpc_port 3001
```

# Send a transaction (the node needs to be running)
```bash
curl -X POST http://localhost:3001 \
-H "Content-Type: application/json" \
-d '{
    "jsonrpc": "2.0",
    "method": "submitTransaction",
    "params": [
        {
            "from": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
            "to": "201f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201",
            "amount": 100,
            "public_key": "3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29",
            "signature": {
                "R": "d3b9bc2c6224e1b0d327f83f2fba25b66f58ea7c87c98a90b9f7f99f4e870be4",
                "s": "0acefe7c263262675dc07f0f270795cf319bd0bb8734dda8d28f055bfa1aa70f"
            },
            "timestamp": 1734345081238,
            "id": "19c44707ea1cc53b699190bea179582b2e947bb59d9695da5961b9cc11e7dd93"
        }
    ]
}'
```

# Get the balance of a given address
```bash
curl -X POST http://localhost:3001 \
-H "Content-Type: application/json" \
-d '{
    "jsonrpc": "2.0",
    "method": "addressBalance",
    "params": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
}'
```

# Build a transaction that you can send via a JSON-RPC request
```bash
cargo run --bin build-transaction -- \
--sender 0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 \
--recipient 201f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201 \
--amount 100 \
--private-key 0000000000000000000000000000000000000000000000000000000000000000
```
