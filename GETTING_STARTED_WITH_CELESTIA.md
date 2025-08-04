# Getting Started with Celestia

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

Clean the database to avoid any issues with rollup from MockDa, if it has been run previously

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
2. Select a recent block and note its hash and height
3. Update your light node configuration:
   - `Header.TrustedHash`: Use the block hash from step 2
   - `DASer.SampleFrom`: Use the block height from step 2

**Important**: This optimization means you won't be able to start your rollup from blocks prior to the selected height.

### Preparing Your Node

#### Getting Your Node Address

Use the `cel-key` utility to list your node's address:
Assuming you're running it from the repo folder after building.

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
$ celestia das sampling-stats
{
  "result": {
    "head_of_sampled_chain": 7413765,
    "head_of_catchup": 7413794,
    "network_head_height": 7413794,
    "workers": [
      {
        "job_type": "recent",
        "current": 7413767,
        "from": 7413767,
        "to": 7413767
      },
      {
        "job_type": "catchup",
        "current": 7413766,
        "from": 7413672,
        "to": 7413767
      },
      {
        "job_type": "recent",
        "current": 7413768,
        "from": 7413768,
        "to": 7413768
      }
    ],
    "concurrency": 3,
    "catch_up_done": false,
    "is_running": true
  }
}
```

Key indicators to check:
- `catch_up_done`: Should be `true` when fully synced
- `network_head_height`: Current height of the network
- `head_of_sampled_chain`: Should be close to network height when synced

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
- **`runner.genesis_height`**: Set to a recent block height for new rollups

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

## Troubleshooting

If you encounter issues:
1. Ensure your Celestia light node is fully synced
2. Verify your node has sufficient TIA tokens
3. Check that all configuration files have been updated correctly
4. Review logs for specific error messages
5. Consult the [Sovereign SDK GitHub repository](https://github.com/Sovereign-Labs/sovereign-sdk) for known issues