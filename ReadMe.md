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
![Project Logo](./assets/architecture.png)

## Getting Started

# Clone the repository
```bash
git clone https://github.com/enokiweave/enokiweave
```

# Run the node
```bash
git clone https://github.com/enokiweave/enokiweave
```

# Send a transaction
```bash
echo '{
    "jsonrpc": "2.0",
    "method": "submitTransaction",
    "params": [{
        "from": "0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
        "to":   "0x201f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201",
        "amount": 100,
        "public_key": "f7a4654958b940d85ef8c3c40afea448f75a236177adbfc2c27da1953bc68b70",
        "signature": {
            "R": "9b3d055ee6a487d46729cb480a1404ad0aa1cc634d734c8ce84673a83b55b126",
            "s": "28f2e14c9fb932301734a86f6e6c1a2d7bf9ebab0973a42e491679fe9d7ee80d"
        },
        "timestamp": 1709159751000,
        "id": "1d2cd8bc76f002b46bde3460b712e8d2f159b20eb899ead2147722615ea805f6"
    }]
}' | nc localhost 3001
```
