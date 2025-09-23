# Getting Started with Celestia

## Table of Contents

- [Prerequisites](#prerequisites)
- [Overview](#overview)
- [Stage 1: Celestia Local Devnet](#stage-1-celestia-local-devnet)
  - [Starting Celestia Devnet](#starting-celestia-devnet)
  - [Running the Rollup](#running-the-rollup)
- [Stage 2: Celestia Testnet](#stage-2-celestia-testnet)
  - [Stopping the Devnet](#stopping-the-devnet)
  - [Setting Up a Celestia Light Node](#setting-up-a-celestia-light-node)
    - [Installation and Setup](#installation-and-setup)
    - [Optimizing Initial Sync](#optimizing-initial-sync)
  - [Preparing Your Node](#preparing-your-node)
    - [Getting Your Node Address](#getting-your-node-address)
    - [Funding Your Node](#funding-your-node)
  - [Verifying Your Light Node](#verifying-your-light-node)
  - [Configuring Your Rollup](#configuring-your-rollup)
  - [Running on Testnet](#running-on-testnet)
  - [Testing Transactions](#testing-transactions)
- [Success!](#success)
- [Next Steps](#next-steps)
- [Stage 3: Celestia Mainnet](#stage-3-celestia-mainnet)
  - [Overview](#overview-1)
  - [Prepare Celestia Node](#prepare-celestia-node)
    - [Key Management](#key-management)
    - [Fund Your Node](#fund-your-node)
    - [Node Configuration](#node-configuration)
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
- You have basic familiarity with Celestia concepts

## Overview

It is recommended to proceed through three stages:

1. **Local Devnet**: Test your rollup with a local Celestia instance to verify basic functionality
2. **Testnet**: Deploy to a public testnet with proper configuration
3. **Mainnet**: Production deployment with secure key management (not covered in this tutorial)

This tutorial covers stages 1 and 2.

## Stage 1: Celestia Local Devnet

The starter repository includes a [Docker Compose](./integrations/docker-compose.celestia.yml) configuration for running Celestia locally, along with all necessary configurations.

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

### Stopping the Devnet

First, stop the local devnet and clean the database if you previously ran on devnet:

```bash
$ make stop-celestia
$ make clean-db
```

### Setting Up a Celestia Light Node

For this tutorial, we'll use the [Mocha testnet](https://docs.celestia.org/how-to-guides/mocha-testnet). 
You'll need to run a Celestia light node to connect your rollup to the network.

**Note**: For production deployments, it's recommended to connect your light node to a reliable RPC provider or use a bridge node for enhanced reliability and performance.

#### Installation and Setup

Follow the Celestia documentation for detailed instructions:
- [Install celestia-node](https://docs.celestia.org/how-to-guides/celestia-node)
- [Setting up a Celestia light node](https://docs.celestia.org/how-to-guides/light-node)

#### Optimizing Initial Sync

To speed up the initial synchronization, you can configure your light node to start from a recent block:

1. Visit the block explorer: https://mocha.celenium.io/
2. Select a recent block and note its hash and height. Remember this number, as it is going to be used in SDK rollup configuration.
3. Update your light node configuration in `~/.celestia-light-mocha-4/config.toml` so the celestia node can be operational sooner because it won't need to start from genesis
   - `Header.TrustedHash`: Use the block hash from step 2
   - `DASer.SampleFrom`: Use the block height from step 2

**Important**: This optimization means you won't be able to start your rollup from blocks prior to the selected height.

### Preparing Your Node

#### Getting Your Node Address

Use the `cel-key` utility to list your node's address (assuming you're running it from the repo folder after building):

```bash
$ ./cel-key list --keyring-backend test \
    --node.type light --p2p.network mocha
using directory:  /Users/developer/.celestia-light-mocha-4/keys
- address: celestia1qd73x7lzh97uxm9lxe49qdfmuup25kp4khaxdd
  name: my_celes_key
  pubkey: '{"@type":"/cosmos.crypto.secp256k1.PubKey","key":"A2hgY3ckADmUQRO01L4J54tZhhZrfE2oGGjGV+63DJcB"}'
  type: local
```

#### Funding Your Node

Your Celestia node address needs `TIA` tokens to submit data. For the Mocha testnet, request tokens from the [faucet](https://docs.celestia.org/how-to-guides/mocha-testnet#mocha-testnet-faucet).

### Verifying Your Light Node

Ensure your light node is running and fully synced by checking the sampling statistics:

```bash
$ celestia header sync-state
{
  "result": {
    "id": 1,
    "height": 7439832,
    "from_height": 7428057,
    "to_height": 7481444,
    "from_hash": "6738B417621AD529A42CE31DC2181B69F5C2A482E2D4B0A2728A07C59D21382C",
    "to_hash": "6316B4692AA1474394BB68E804F151A266792CB6F5B73B444F0D033B0D77BD1D",
    "start": "2025-08-04T16:33:14.192732+02:00",
    "end": "0001-01-01T00:00:00Z"
  }
}
```

The key indicator is `height` which should be close to `to_height`, indicating that light node can pull all necessary data.

Test blob submission to verify your node is working correctly:

```bash
$ export AUTH_TOKEN=$(celestia light auth admin --p2p.network mocha)
$ celestia blob submit 0x42690c204d39600fddd3 0x676d auth $AUTH_TOKEN
{
  "result": {
    "height": 7413840,
    "commitments": [
      "0xd0c16160a4148b6054f94d63c4fcc6ed063605557595bde4894fb300aee75226"
    ]
  }
}
```

In the case of a correct submission, the result will indicate the height at which the blob has been submitted and the commitment.

### Configuring Your Rollup

You'll need to update several configuration files:

#### 1. Namespaces

Your rollup requires two namespaces: one for batches and one for proofs. 

Update these in [`crates/rollup/src/da.rs`](crates/rollup/src/da.rs#L15). 
They are specified in rust source files as they're part of the cryptographic commitment for the prover, and need to be compiled into a binary.

#### 2. Genesis Configuration

Update your Celestia node address in [`configs/celestia/genesis.json`](configs/celestia/genesis.json):

- `sequencer_registry.sequencer_config.seq_da_address`
- `paymaster.payers[].sequencers_to_register` (if using paymaster)

#### 3. Rollup Configuration

Update `configs/celestia/rollup.toml`:

- **`da.celestia_rpc_auth_token`**: Get this using:
  ```bash
  $ celestia light auth admin --p2p.network mocha
  ```
- **`da.celestia_rpc_address`**: Default value should work for standard setups. Ensure this matches the port your light node is listening on.
- **`da.signer_address`**: Your node address (for verification purposes)
- **`runner.genesis_height`**: Set to a block that is higher than or equal to the block selected in the Celestia light node configuration.

### Running on Testnet

Start your rollup:

```bash
$ cargo run --no-default-features \
  --features=celestia_da,mock_zkvm \
  -- --rollup-config-path=configs/celestia/rollup.toml \
  --genesis-path=configs/celestia/genesis.json
```

Your node will begin posting empty batches to maintain rollup liveness.
You can open the Celestia block explorer, find your namespace and see that blobs are posted from the address of your light node.

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

You can track the `tx_hash` in your rollup logs. Once posted to the DA layer, check your rollup's namespace page to see the published batch (it will be slightly larger than empty batches).

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

After successfully testing your rollup on Celestia Testnet, you're ready to deploy to the mainnet. 
While the technical setup is similar to testnet, mainnet deployment requires enhanced security measures and careful management of keys and secrets.

**Important**: Mainnet deployment involves real assets and cannot be easily reversed. Take extra care with key management, backup procedures, and security hardening.

### Overview

The mainnet deployment process involves three critical steps:

1. **Secure Celestia node setup** - Configure a production-ready Celestia light node with proper key management
2. **Configuration review and hardening** - Audit all configurations with mainnet-specific values and security considerations
3. **Deployment and monitoring** - Launch with comprehensive monitoring and alerting

### Prepare Celestia Node

#### Key Management

For mainnet, use the OS keyring backend instead of the test backend for enhanced security:

```bash
./cel-key add sov-mainnet --keyring-backend os --node.type light
using directory:  /home/ubuntu/.celestia-light/keys
Enter keyring passphrase:
Re-enter keyring passphrase:

- address: celestia1v3mugr5g37mth6k67wxyx8j4lhqkap7me8lhfr
  name: sov-mainnet
  pubkey: '{"@type":"/cosmos.crypto.secp256k1.PubKey","key":"A6h6NdmFLwxNqRdklOWb3ugNe8C4oR3A2ij52819P9jS"}'
  type: local
```

#### Fund Your Node

Fund your Celestia address with sufficient TIA tokens based on your expected data usage. Consider:
- Current TIA price and network fees
- Your rollup's expected transaction volume
- Buffer for fee spikes during network congestion
- Set up monitoring alerts for low balance thresholds

#### Node Configuration

Follow the [Celestia light node setup guide](https://docs.celestia.org/how-to-guides/light-node) for the mainnet. 

Key differences from testnet:

1. **Run as a system service**: Configure your node to [run as a SystemD service](https://docs.celestia.org/how-to-guides/systemd#celestia-light-node) for automatic restarts and better stability
2. **Configure secure key storage**: Update `~/.celestia-light/config.toml` to use the OS keyring:
    ```toml
    [State]
    DefaultKeyName = "sov-mainnet"
    DefaultBackendName = "os"
    ```
3. **Start and verify synchronization**: Launch your Celestia light node and ensure it's fully synced before proceeding

### Review Configuration

Carefully audit all configuration files for mainnet deployment:

#### 1. Chain Constants (`constants.toml`)
- Verify `CHAIN_ID` and `CHAIN_NAME` are set to production values
- Ensure these cannot be confused with testnet values

#### 2. Genesis Configuration (`configs/genesis.json`)
- **Address rotation**: Replace all testnet addresses with production addresses
- **State migration**: Determine which data (if any) to migrate from testnet:
  - Bank balances
  - Module-specific state
  - User accounts
- **Sequencer configuration**: Update with your new mainnet Celestia address
- **Paymaster settings**: Review and configure production paymaster parameters
- **Chain state**: Finalize all genesis parameters

#### 3. Rollup Configuration (`configs/rollup.toml`)
- **DA settings**: Update with mainnet Celestia node details:
  - New authentication token (get with `celestia light auth admin`)
  - Correct RPC address
  - Mainnet signer address
- **Runner configuration**: 
  - Set appropriate `genesis_height` for mainnet
  - Configure production public endpoints
- **Performance tuning**: Adjust batch sizes and intervals for mainnet load

#### 4. Revenue Share Module
Verify that the revenue share module is properly configured in your rollup's STF for mainnet economics.

#### 5. Nginx proxy

Nginx can be placed in front of the rollup node to handle DDoS separately, terminate TLS and other standard web deployment practices.

### Security Hardening Checklist

Before mainnet launch, ensure:

- [ ] All private keys are stored securely (never in code or configuration files)
- [ ] Authentication tokens are managed through environment variables or secure vaults
- [ ] Backup procedures are documented and tested
- [ ] Monitoring and alerting systems are configured
- [ ] Rate limiting and DDoS protection are in place
- [ ] Access controls are configured for all endpoints
- [ ] Audit logs are enabled and stored securely
- [ ] Disaster recovery plan is documented

### Start and Monitor

1. **Pre-launch verification**: Double-check all configurations and ensure your Celestia node is healthy
2. **Launch your rollup**: Start with your production configuration and closely monitor initial behavior
3. **Post-launch monitoring**: Track:
   - Transaction throughput and latency
   - DA layer submission success rate
   - Node resource usage
   - Error rates and patterns
4. **Set up alerts** for:
   - Low TIA balance
   - Failed DA submissions
   - Abnormal transaction patterns
   - System resource thresholds

## Troubleshooting

If you encounter issues:
1. Ensure your Celestia light node is fully synced
2. Verify your node has sufficient TIA tokens
3. Check that all configuration files have been updated correctly
4. Review logs for specific error messages
5. Consult the [Sovereign SDK GitHub repository](https://github.com/Sovereign-Labs/sovereign-sdk) for known issues