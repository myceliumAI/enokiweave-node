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
          "c1": "AkTUULAgsOWfzGOn3tSf3KTCrkkLq6xNdRXE3bHCpHHZ",
          "c2": "A998vn77x3DbtMrLo6T1Ab5gy7ZvSePSt4zlEZ6yF11P",
          "range_proof": "ctrMObmFsTvJIdBYMBcqc9hRXjY7p1VNa6syukqOo3FSEWsl5WCV1Qh4cmFsB6/Cl+kXbeaaUk/L1ORuMBEoP55PjwKHmTlQ6E5I//m5cyQ/+Q8S4/3UrkLlHkVFjo857IpnfZ3tgzEP8SRnaX00th1nOK0P/bHAEQxtQv1bDQPkpsLxQhSjLQ7DKrejdOL8oYdfuTgGJ6P5wlan6YWWBuKCdHpUBb1UYYtS7vlUqLLD1CtK5Gj52XKZ6v3D5mkCk5uGsTDzQCUNEEwzcXXXVKjaWLONIBJdQutpiTW3Twksw56vwOrMLjhpbeWYVOxOA+eA03YLmwmr4edag2lUQvbdzLRgwgKe7XOB7TKy9EY0NPETY+Pn7VTjaHWWm/dNgtfzL2sbyD5aefWBWJ0ETt9og2lUHwcexWx020fZ1COcNX/x7SwnP3xAJx7HAR+t+vaVJFfX8wvKoz+VpjXiSw7mYxX/wL7U2Kmmpfu2f3Ff1hHIkO8IUO5OW0nWyl1CHAEtWnI65a/abZJnS6OFgRWTQZWAPb7nt/I4poGcnFUer+gsYqZrIlTbEWL+6DQ/xaMTG0btTpWSwEKgP3U+ItCqTdVL+w1kQc6hIp8AwHoYJnHrR/3RIAimiqGCTbVxWN4Ha+zDCYTlUQhKNG+xd+9L6H4ygPwjK7O8mADlDRRyCDxD7/xxu4mXbKammh94kqqV74jt4evGLqG21YBbLIYRGAK6J7zqgq1/aEOcCB/ME4w464pV6B5f20F2141m5IJV2zAIPjNjrro6LuoIormnv96dOg9ynHy7zTQMY2DmB9GUid6+mykEMq7dkdn0iE6qe5mVy2dMfcCJ79CHDQvZfVR7AfKblphexD5ZQCmQfpIcCgseyPRU1RPsXoED"
        }
      },
      "from": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
      "previous_transaction_id": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
      "public_key": "04c2a4c3969a1acfd0a67f8881a894f0db3b36f7f1dde0b053b988bf7cff325f6c3129d83b9d6eeb205e3274193b033f106bea8bbc7bdd5f85589070effccbf55e",
      "signature": {
        "R": "5a702a3abc0d2238f5c7a88d28c8ebfb42d9b2ca007c2a4059ce9e031f96f857",
        "s": "6a3ae950a830882f88ef47d6b4e0b8cd4039d682de28b12769f43435f81889df"
      },
      "timestamp": 1738943005293,
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
