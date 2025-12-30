# Getting Started with Celestia

## Table of Contents

- [Prerequisites](#prerequisites)
- [Overview](#overview)
- [Stage 1: Celestia Local Devnet](#stage-1-celestia-local-devnet)
  - [Starting Celestia Devnet](#starting-celestia-devnet)
  - [Running the Rollup](#running-the-rollup)
- [Stage 2: Celestia Testnet](#stage-2-celestia-testnet)
  - [Stopping the Devnet](#stopping-the-devnet)
  - [Setting Up RPC Access](#setting-up-rpc-access)
    - [Option A: QuickNode (Recommended)](#option-a-quicknode-recommended)
    - [Option B: Other RPC Providers](#option-b-other-rpc-providers)
  - [Creating a Celestia Wallet](#creating-a-celestia-wallet)
  - [Funding Your Wallet](#funding-your-wallet)
  - [Configuring Your Rollup](#configuring-your-rollup)
  - [Running on Testnet](#running-on-testnet)
  - [Testing Transactions](#testing-transactions)
- [Success!](#success)
- [Next Steps](#next-steps)
- [Stage 3: Celestia Mainnet](#stage-3-celestia-mainnet)
  - [Overview](#overview-1)
  - [RPC Provider Setup](#rpc-provider-setup)
  - [Key Management](#key-management)
  - [Review Configuration](#review-configuration)
  - [Security Hardening Checklist](#security-hardening-checklist)
  - [Start and Monitor](#start-and-monitor)
- [Troubleshooting](#troubleshooting)

Sovereign SDK rollups support Celestia as a Data Availability (DA) layer. Celestia has been designed for accommodating rollups, offering instant finality and significant data throughput.

This tutorial will guide you through running the rollup starter on Celestia, from local development to testnet deployment.

## Prerequisites

Before starting this tutorial, ensure that:
- You have Rust and Cargo installed
- You have Docker installed (for local devnet)
- Your rollup is working with MockDa
- You have basic familiarity with [Celestia concepts](https://docs.celestia.org/learn/how-celestia-works/overview)

## Overview

It is recommended to proceed through three stages:

1. **Local Devnet**: Test your rollup with a local Celestia instance to verify basic functionality
2. **Testnet**: Deploy to a public testnet using an RPC provider
3. **Mainnet**: Production deployment with secure key management

This tutorial covers all three stages.

## Stage 1: Celestia Local Devnet

The starter repository includes a [Docker Compose](./integrations/docker-compose.celestia.yml) configuration for running Celestia locally, along with all necessary configurations.

The default configuration uses a pre-funded Celestia wallet (`celestia1a68m2l85zn5xh0l07clk4rfvnezhywc53g8x7s`) that works out of the box with the local devnet. No additional setup is required.

### Starting Celestia Devnet

First, start the Celestia Docker containers by running `make start-celestia` command:

```bash,test-ci,bashtestmd:exit-code=0
$ make start-celestia
[+] Running 4/4
 ✔ celestia-validator                           Built                                                                                                                                                                                    0.0s
 ✔ celestia-node-0                              Built                                                                                                                                                                                    0.0s
 ✔ Container integrations-celestia-validator-1  Started                                                                                                                                                                                  0.1s
 ✔ Container integrations-celestia-node-0-1     Started                                                                                                                                                                                  0.2s
waiting for container 'celestia-node-0' to become operational...
[2025-07-31 12:05:14] health == 'starting': Waiting for celestia-node-0 to be up and running...
[2025-07-31 12:05:17] health == 'starting': Waiting for celestia-node-0 to be up and running...
[2025-07-31 12:05:20] celestia-node-0 is healthy
 ✔ Celestia devnet containers are ready.
```

### Running the Rollup

Clean the database to avoid conflicts if you previously ran the rollup with MockDa

```bash,test-ci,bashtestmd:exit-code=0
$ make clean-db
```

Now run your rollup with the `celestia_da` feature enabled:

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run --no-default-features --features=celestia_da,mock_zkvm -- --rollup-config-path=configs/celestia/rollup.toml --genesis-path=configs/celestia/genesis.json
```

The log output should indicate a healthy running rollup. Verify that the REST API is responding:

```bash,test-ci,bashtestmd:compare-output
$ curl -s http://127.0.0.1:12346/modules/value-setter/state/value
{"value":null}
```

## Stage 2: Celestia Testnet

**Prerequisites**: You will need a Celestia wallet funded with TIA tokens on the Mocha testnet. TIA is required to pay for blob submissions to the DA layer.

### Stopping the Devnet

First, stop the local devnet and clean the database if you previously ran on devnet:

```bash
$ make stop-celestia
$ make clean-db
```

### Setting Up RPC Access

The rollup connects directly to Celestia consensus nodes via RPC and gRPC. You don't need to run your own Celestia node — instead, use an RPC provider.

For this tutorial, we'll use the [Mocha testnet](https://docs.celestia.org/how-to-guides/mocha-testnet).

#### Option A: QuickNode (Recommended)

[QuickNode](https://www.quicknode.com/) provides Celestia RPC endpoints with built-in authentication.

1. Create a QuickNode account and add a Celestia Mocha testnet endpoint
2. Note your endpoint details:
   - **Endpoint hostname**: e.g., `your-endpoint.celestia-mocha.quiknode.pro`
   - **API token**: The alphanumeric string in your endpoint URL

Your configuration will look like:
```toml
[da]
rpc_url = "wss://your-endpoint.celestia-mocha.quiknode.pro/your-api-token/"
grpc_url = "https://your-endpoint.celestia-mocha.quiknode.pro:9090"
grpc_auth_token = "your-api-token"
```

#### Option B: Other RPC Providers

You can use any Celestia RPC provider that exposes:
- **Celestia RPC endpoint** (port 26658 by default) — supports both HTTP and WebSocket
- **gRPC endpoint** (port 9090 by default)

Configure the endpoints in your `rollup.toml` accordingly.

### Creating a Celestia Wallet

Your rollup needs a Celestia wallet to sign and submit blobs. The private key must be in unarmored hexadecimal format (64 characters).

**Option A: Using cel-key** (if you have [celestia-node](https://docs.celestia.org/operate/keys-wallets/celestia-node-key) installed):
```bash
# Create a new key
$ cel-key add my-rollup-key --keyring-backend test --node.type light --p2p.network mocha

# Export as unarmored hex
$ cel-key export my-rollup-key --unarmored-hex --unsafe --keyring-backend test --node.type light --p2p.network mocha
```

**Option B: Using openssl**:
```bash
# Generate a random 32-byte hex key
$ openssl rand -hex 32
```

**Option C: Using cast** (from [Foundry](https://book.getfoundry.sh/)):
```bash
$ cast wallet new | grep "Private key" | awk '{print $3}'
```

To derive the Celestia address from a private key, use [cel-key](https://docs.celestia.org/operate/keys-wallets/celestia-node-key) or any Cosmos SDK compatible wallet, for example Keplr.

Note down:
- **Private key** (hex format, 64 characters) — for `signer_private_key` in config
- **Celestia address** — for genesis configuration (e.g., `celestia1abc...`)

### Funding Your Wallet

Your Celestia wallet needs TIA tokens to submit data blobs. For the Mocha testnet, request tokens from the [faucet](https://docs.celestia.org/how-to-guides/mocha-testnet#mocha-testnet-faucet).

### Configuring Your Rollup

Update the following configuration files:

#### 1. Namespaces

Your rollup requires two namespaces: one for batches and one for proofs.

Update these in [`constants.toml`](constants.toml):

```toml
# Must be exactly 10 ASCII characters for Celestia
BATCH_NAMESPACE = { byte_string = "your-batch" }
PROOF_NAMESPACE = { byte_string = "your-proof" }
```

> **Note**: The `byte_string` format converts ASCII characters to bytes, so the string must be exactly 10 characters long (e.g., `"your-batch"` = 10 chars ✓).

These values are compiled into the binary as they're part of the cryptographic commitment for the prover.

#### 2. Rollup Configuration

Update `configs/celestia/rollup.toml`:

```toml
[da]
# Celestia RPC endpoint
# Use ws://http:// for local devnet, wss://https:// for testnet/mainnet
rpc_url = "wss://your-endpoint.celestia-mocha.quiknode.pro/your-api-token/"

# gRPC endpoint for blob submission
grpc_url = "https://your-endpoint.celestia-mocha.quiknode.pro:9090"

# Authentication token (required for most providers)
grpc_auth_token = "your-api-token"

# Your Celestia wallet private key (hex format, 64 characters)
signer_private_key = "your-private-key-hex"
```

#### 3. Genesis Configuration

**Important**: The Celestia address in genesis must match the private key in your rollup config. The default configuration uses the devnet address `celestia1a68m2l85zn5xh0l07clk4rfvnezhywc53g8x7s` — you must update this for testnet/mainnet.

Update your Celestia address in [`configs/celestia/genesis.json`](configs/celestia/genesis.json):

```json
{
  "sequencer_registry": {
    "sequencer_config": {
      "seq_da_address": "celestia1your-address-here"
    }
  },
  "paymaster": {
    "payers": [
      {
        "sequencers_to_register": [
          "celestia1your-address-here"
        ]
      }
    ]
  }
}
```

Both values must be set to your Celestia address (the one corresponding to your `signer_private_key`).

Also make sure that recent address is set `chain_state` section. 
For the new setup it should be pretty close to the latest head to avoid unnecessary processing of older blocks.

```json
{
  "chain_state": {
    "genesis_da_height": 9441180
  }
}
```

### Running on Testnet

Rebuild with updated namespaces (if you changed them):

```bash
$ cargo build --no-default-features --features=celestia_da,mock_zkvm
```

Start your rollup:

```bash
$ cargo run --no-default-features \
  --features=celestia_da,mock_zkvm \
  -- --rollup-config-path=configs/celestia/rollup.toml \
  --genesis-path=configs/celestia/genesis.json
```

Your node will begin posting batches to Celestia. You can monitor activity in the [Celestia block explorer](https://mocha.celenium.io/) by searching for your namespace.

### Testing Transactions

Submit a test transaction using the TypeScript example:

```bash
$ cd examples/starter-js && npm install
$ npm run start
Initializing rollup client...
Rollup client initialized.
Initializing signer...
Signer initialized.
Signer address: 0x9b08ce57a93751ae790698a2c9ebc76a78f23e25
Sending create token transaction...
Tx sent successfully. Response:
{
  id: '0x633b06f81b2884f8f40a3f06535cdbedb859c37d328c24fd4518377c78dac60e',
  events: [
    {
      type: 'event',
      number: 0,
      key: 'Bank/TokenCreated',
      value: {
        token_created: {
          token_name: 'Example Token',
          coins: {
            amount: '1000000000',
            token_id: 'token_10jrdwqkd0d4zf775np8x3tx29rk7j5m0nz9wj8t7czshylwhnsyqpgqtr9'
          },
          mint_to_address: { user: '0x9b08ce57a93751ae790698a2c9ebc76a78f23e25' },
          minter: { user: '0x9b08ce57a93751ae790698a2c9ebc76a78f23e25' },
          supply_cap: '100000000000',
          admins: []
        }
      },
      module: { type: 'moduleRef', name: 'Bank' },
      tx_hash: '0x633b06f81b2884f8f40a3f06535cdbedb859c37d328c24fd4518377c78dac60e'
    }
  ],
  receipt: { result: 'successful', data: { gas_used: [ 21119, 21119 ] } },
  tx_number: 0,
  status: 'submitted'
}
```

You can track the `tx_hash` in your rollup logs. Once posted to the DA layer, check your rollup's namespace page to see the published batch.

## Success!

Congratulations! Your rollup is now running on Celestia testnet. You can monitor your rollup's activity through:
- Rollup logs
- Celestia block explorer
- Your rollup's REST API

## Next Steps

- Explore the [Sovereign SDK documentation](https://docs.sovereign.xyz/) for advanced rollup features
- Learn about [Celestia's architecture](https://docs.celestia.org/) for deeper integration
- Plan your mainnet deployment strategy

## Stage 3: Celestia Mainnet

**Prerequisites**: You will need a Celestia wallet funded with TIA tokens on mainnet. Ensure you have sufficient TIA to cover blob submission fees for your expected transaction volume.

After successfully testing your rollup on Celestia Testnet, you're ready to deploy to mainnet. Mainnet deployment requires enhanced security measures and careful management of keys and secrets.

**Important**: Mainnet deployment involves real assets and cannot be easily reversed. Take extra care with key management, backup procedures, and security hardening.

### Overview

The mainnet deployment process involves:

1. **RPC provider setup** — Configure reliable Celestia mainnet RPC access
2. **Secure key management** — Protect your signing keys
3. **Configuration review** — Audit all configurations for production
4. **Deployment and monitoring** — Launch with comprehensive monitoring

### RPC Provider Setup

For mainnet, use a reliable RPC provider with:
- High availability and redundancy
- Low latency connections
- Appropriate rate limits for your transaction volume

Update your configuration with mainnet endpoints:

```toml
[da]
rpc_url = "wss://your-mainnet-endpoint.quiknode.pro/your-api-token/"
grpc_url = "https://your-mainnet-endpoint.quiknode.pro:9090"
grpc_auth_token = "your-api-token"
signer_private_key = "${CELESTIA_SIGNER_KEY}"  # Use environment variable
```

### Key Management

For mainnet, never store private keys in configuration files:

1. **Use environment variables**:
   ```bash
   export CELESTIA_SIGNER_KEY="your-private-key-hex"
   ```

2. **Or use a secrets manager** (AWS Secrets Manager, HashiCorp Vault, etc.)

3. **Secure your key**:
   - Generate keys on an air-gapped machine
   - Store backups in secure, geographically distributed locations
   - Consider using HSM for high-security deployments

Fund your Celestia address with sufficient TIA tokens:
- Estimate based on expected transaction volume
- Add buffer for fee spikes during network congestion
- Set up monitoring alerts for low balance

### Review Configuration

Carefully audit all configuration files:

#### 1. Chain Constants (`constants.toml`)
- Set production `CHAIN_ID` and `CHAIN_NAME`
- Configure unique namespaces for mainnet
- Ensure values cannot be confused with testnet

#### 2. Genesis Configuration (`configs/genesis.json`)
- Replace all testnet addresses with production addresses
- **Update Celestia address** to match your mainnet signing key:
  - `sequencer_registry.sequencer_config.seq_da_address`
  - `paymaster.payers[].sequencers_to_register`
- Review paymaster settings for production use
- Set appropriate `genesis_da_height`

#### 3. Rollup Configuration (`configs/rollup.toml`)
- Configure mainnet RPC endpoints
- Set production `bind_host` and `bind_port`
- Tune `max_batch_size_bytes` and other performance parameters
- Configure monitoring endpoints

### Security Hardening Checklist

Before mainnet launch:

- [ ] Private keys stored securely (never in code or config files)
- [ ] Authentication tokens managed via environment variables or secrets manager
- [ ] Backup procedures documented and tested
- [ ] Monitoring and alerting configured
- [ ] Rate limiting and DDoS protection in place
- [ ] TLS configured for all public endpoints
- [ ] Access controls configured for admin endpoints
- [ ] Audit logs enabled and stored securely
- [ ] Disaster recovery plan documented

### Start and Monitor

1. **Pre-launch verification**:
   - Verify RPC connectivity
   - Confirm wallet has sufficient TIA
   - Test blob submission manually if needed

2. **Launch your rollup** with production configuration

3. **Monitor**:
   - Transaction throughput and latency
   - DA layer submission success rate
   - Node resource usage (CPU, memory, disk)
   - Error rates and patterns

4. **Set up alerts** for:
   - Low TIA balance
   - Failed DA submissions
   - RPC connection failures
   - Abnormal transaction patterns
   - System resource thresholds

## Troubleshooting

**Connection errors to RPC provider**:
- Verify endpoint URLs are correct (use `ws://`/`wss://` or `http://`/`https://` for RPC, `http://`/`https://` for gRPC)
- Check authentication token is valid
- Ensure your IP is allowlisted if the provider requires it

**Blob submission failures**:
- Verify your wallet has sufficient TIA tokens
- Check that the private key format is correct (64-character hex)
- Ensure the Celestia address in genesis matches the signing key

**Namespace issues**:
- Namespaces must be exactly 10 bytes for Celestia
- Rebuild the binary if you change namespace values in `constants.toml`

**General debugging**:
- Check rollup logs for specific error messages
- Verify all configuration files have been updated correctly
- Consult the [Sovereign SDK GitHub repository](https://github.com/Sovereign-Labs/sovereign-sdk) for known issues