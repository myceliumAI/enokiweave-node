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
          "quorum": {
            "c1": "A81XNukTrG8hCg3UR42jxZOuto32so09ygcug49EZOaL",
            "c2": "ArqdwyfIa1fWPOl5Tbmgi99fs3QWWJb2+PIrzwbSiB38",
            "range_proof": "/p3dnwZa3zrq8dmoNTkp+2nIDgkijwYs91pt7WQTn39SRPSyH+2MquBx5azDIP7qs0X+meq9Ru+3z/un4NgoKpSnfBuqORnVJHjbG0mtFywdXkgNeiSVQxP2Z6fTMNQz0lhVwSdZRLEahu6CFfvfhn3/UhO0jWQyUojJz9zZm3gqYHYk510IqvNYFiwYne3CgVrsbcypmov8hO7pU0jaDYJhgbQJOdV2cNCB0HSOT4mwBWec46NfjfBSuT3OjxkB+ruITQ8wFQ4Z6Zr5JSRHRB2naT4U5/umWkfDFK2IgQTMiGf3tmJJFNTEgtT21RHwdeecfsht3JstHYglgUtUbUTzL6f1+TJjfLeQVZtzih1GOQB1gdI53y9W5hmh0skq0Dd0bdiOkZiW3JUUAowo0/zNkLtYV3DUHljCIf9gAzPYWKVpTLNFnUgqKG50SGwmyQU8QSu4eD8nr+c5iyx5Zw5yiWY7LHPgJfBNENARnbbFk2mY3HvYCLv5qiDQemwPzC6Ol0POALLJeW80zUlE+yMzMIJHArnQ2wswUR3ITwf829O5+U532a60+jqFOPtJbD78OOUK6BLtFTkuu528Z3Lbau0ylaHV9hLd1k9KZR3ROXYvQL45KE2jkuBIEp0PYsH/2A+ab4ha6Kpv5/FJ2o5wZhWC68DNEndR5z15LmxSMeGUyRQqKonXSxn9QE/O1ilFQ506mXrCt0xkEhsnXmApHU/3q7rgtc6XhPIFapYeBBTVvtUYFMp5cg83GBZKbAQsg3prvKxiRTi65bEAKAf67lAy+6ZQazG9svQw93uzqla1lJv5pMI9JSuHnNT/9LYgOevNkWQpjpko1QTQDbFCV+2DSoQ7SM3XvvNfDbWgu2gmQK7/jan3gQRgGDkL"
          },
          "recipient": {
            "c1": "AmZUcbkVn00B13l/PAuhSdnCl2e1hV1zekO6KFjEa2Os",
            "c2": "AnV7SKWnnkaNoqw2AsdZkI6iLTRYno9XXpu+1Qasr//+",
            "range_proof": "/A7PWNkQhRxFdOcmP126QcvECCGnyKpTx0S7j7MShFzSPB5DRojAJLw5Vh3D3Slu68Ru+Ke9Csm7FGBKLShVVWwaN00kvGvkk5QhQD27kh6PlF9SoLTG8OM3yjrLnUZFpllhstCknawOGGGgb1UOATqpd6itnsgqN/CpKnOpb1C7Q8f5GiKR8DWuWGKiavqRL6c1DizE7+rYaKVcf05CCMlko7okHJUCMLCB8yLqiEmhxF1KtoaF/ImXlGJSSLEE3wAKqHHM28OOpRoZj3RNpMdD5gTVgdUlbHeQaXi1pwlKzvzC2oYNo+Es/6Su+nAYGnatyFgb+2DOOTSKNNWmWOZV3rUumaH3bcXaJ3d6xyOsrZwOE2F8zO3eJ/RVNsd1ajqKOAm0bfBPEcCT2OBFPNPIEOmoTmfRGi01UW1AwjlMHtsJ49I2bYJhS5gL1WD92ihw0j4DLs0okEBVveupeES8iDTI5Npnu34OfUjMYOndHyNA5vbP5LZMbIgFMyQcdrFpp5+xKk+UkLgLCGxZ7cBuJAnKKkDAwoA/T16ITgtGKb9pLKnSjPaGZG25v1/uUF1tcwxCA36YE5ayGyUka6JtkO8Tv1tDgk4PnDNWJGuwayWIozTJJ+avslGNBN0F3GWvY5nC+aoCyZdqIMgdDfwKXUNGw8KNBKOwQRFsvTAgw4FW403jcQpOJ1F9hbEm4snu05YLU5XlKjK17CbIMrp0eJN0ScFz06LBEGaTbfE/Wl6k2epAee5WEqqi21BvgPFNX6Z7IIgmAlSCj2aJRH7BlLYjpiKbB4Ac33afeQeRhg6c1RY0gKil/9aG0peHUoM5wSaFIFSQ8v3s9aF6AllOF54+VVnTiXz29af0zrte7qxYMwFL6MzactGcPzoN"
          },
          "sender": {
            "c1": "Avpd1MdI4mwBfMLdyyDR9Qr3BDFELiKvApX3gcMDgDjy",
            "c2": "AhRI67yjl9CWoZCbsHT34lFCokgOg5tQglnmLSnDB2Pw",
            "range_proof": "wnkgzlnj9klbHG9qIvJA9mUnGCIjK2inL/8LeWxzRQSg8vbFxuW4KPcWEGGsqbxWrPFtxYx3pbss1uDhyy18c2yG2vZ5shPo0vxS4dhXxR959F5daLdulObvDfX9DQE/tHdaOu/qpBElEBicm+bae+31KA+ehHfnbXJJtZ68HyXfFoxdEMUdQZQUZf5WBjQjwcUVI1Zj/eKAXgGp8pixA/wAFzK7808YBzN4jUq+W4nC9IFyLKQg8O1pM/M1dNoKyo5NSIG6MgC3OKnDt/ZWb4HRUHwZUB+eJhNNi9jOOAM27QS7wp8ZqEp4xZyU8uVh4IIy5Wijk7ONVGGd4GxuX0Inyk1X94FETxeXoXM9WWitCCLMnZ7KG32R91qNJx9VLqxLgGLK0hFnQcK4xBnGf54kMBqel+sb7flphscPMiNs9MEfXCH2B1LnylwqHuqu8m/ixCC8xuZcGZDLP3ZRWeZoVaNY4otKmt0EhNIkqoCXXl4uOly5eYko0+1tQE9Vavi0eSqOTxwoVSMmLtvVc/meCawrR2/lVmYKuM740XMcxMQSGonK2ZLYjoOBZPOTJUGkijK5DsL6+LTXuwcIb5xNQxpAzXTDbAOOUfr15cK1kpP+h9kPIwbJrwGTjN8KzAWs/qf3K704ktBTqhZAa0uZtIvBwGLe3wfP47enU0xoSMCTB0jfjkIFXhkQgmLGW8rFFbJ0OEomzmsRDSrcXWhsrkt6gfwgyll5YRTDnUr38DuAnkeryqwHDq3Jk3J6uIoTiGOFQsFJ+d2vCepwcuzXeqM2uCk33+eSnRPLqTi+4RUbrJjWT5b3xpOM0pRWmlRGxLZ5CclVqzXeJCLwC5vrptMXScggMDS98qtEWFDxIPB8kY/WWiUjGCb/CAEF"
          }
        }
      },
      "from": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
      "previous_transaction_id": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
      "public_key": "04c2a4c3969a1acfd0a67f8881a894f0db3b36f7f1dde0b053b988bf7cff325f6c3129d83b9d6eeb205e3274193b033f106bea8bbc7bdd5f85589070effccbf55e",
      "signature": {
        "R": "44152312c43cac7f0f3d6be1e72c9e746166d291d42f9f64e3b2a257ad1fb49f",
        "s": "325bd5f08af004e2533f8ba80fc27370399e5767f28422683d17dc49718ac750"
      },
      "timestamp": 1738970684326,
      "to": "201f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020f"
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
