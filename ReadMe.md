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
cargo run --bin run-node -- --genesis-file-path ./setup/example_genesis_file.json --rpc-port 3001
```

# Send a transaction (the node needs to be running)
```bash
curl -X POST http://localhost:3001 \
-H "Content-Type: application/json" \
-d '{
  "id": 1,
  "jsonrpc": "2.0",
  "method": "submitTransaction",
  "params": [
    {
      "amount": {
        "Confidential": {
          "c1": "Ak9CwY1Hiv5EK4sIZ2RV8kXyp1IPPttFs5TDWWXiUTSV",
          "c2": "ApfHlTg0PRpwrsSp+8qInzww07nSexxCOOAcc9hO0Poj",
          "range_proof": "lu/2NVw+LHjzeEWNBNkmjxfFClo4oznWHGpQ9SJUKmtsHPPj6i5Y3e/E8sJLkRHLmHpFxbt9gP1sWJL+bLGtQ3Z1u4RmLI/wzlEuQ5GgYzFmbm5z3DrEqaum67lOObBmNH/9nMF92ZOTDt6opUQHrnzQvsoZdvPBGmLaVEHUmTSWDdkh8kgDVqGHb4sqEqBUXuABe7A2XAJMyPV3g+8FAFU7LWn7mBl/aPEUUED4afhgOXtmELkgDrRZmq9jVVsDdu7rNXv+bvR5No3yx511jES0BA9fMvd5h+WbEgk+6gbE0wGY9/TClWAZOQcaKWYS53w/bUwTaHTk4C6R2nSeNkT+XnKlT5fkODS3Nys77Liyoxsp19aiWzzgpk+VFzg/NIQ/ct2f8mTYmALlf+bBEwwbhN6K6mOI9n4jSjBiSH+u2BBTaEWrzkyXKDcn5sUoECQk2pFEjQWSHi1IE8dEE2QptRJc8cWoclYzB3GhX9GSY84yahNLCjfob+xh9EVVwJ+2/2TEZsyiWhi6V2SxUWUNtpK6qmetF+yxZqzoCW3YNbuTBzZSu/fQH0eTGttqfrf0Fy7LVIHDGoSF6Ol6FTJrbI990wC/LIyH9vOp/Cz23IgUpkSyC/8N1EwevCRlzuH84VPSHSPsOrkdf65ISuM6FlUHi0RdUZMKDELxFX6wTk9/zobeZBqO1o3kwiqlYONoRNqZZIRoH/eDTdk/cgy8wjTXmzIrSdCChcUyOoF+ljXjcK5kM5t4rEx6Cx0RytzfDIrpibiDuTtl24X6bT0ANJzV8q+JK3oGR/WxigNZ9aYa2mfz97eXvk3bhuKktmsXYJsqIp0GdObJBaaLCwV9e3M6uOCZuHKuRf2FxphkbadcGgypE5WQy5ZU1UkO"
        }
      },
      "from": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
      "previous_transaction_id": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
      "public_key": "04c2a4c3969a1acfd0a67f8881a894f0db3b36f7f1dde0b053b988bf7cff325f6c3129d83b9d6eeb205e3274193b033f106bea8bbc7bdd5f85589070effccbf55e",
      "signature": {
        "R": "64701503243dd989ac11ce228f2915ff19e5ec3a88247c58cb17a34d19494d8a",
        "s": "068775b9e99dde597f8df737a029f29b646637d0a0cca2968f91438196c23499"
      },
      "timestamp": 1738941880745,
      "to": "201f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201"
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
--private-key 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff \
--previous-transaction-id 0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
```
