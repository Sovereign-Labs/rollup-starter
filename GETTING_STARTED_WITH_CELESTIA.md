# Celestia

Sovereign Rollups support Celestia as the Data Availability (DA) layer. 
Celestia has been designed with rollups in mind and offers instance finality and significant data throughput.

This tutorial describes how to run the rollup starter on Celestia.

## Prerequisites

Your rollup works with MockDa and you want to switch to Celestia.

Here are steps 

1. Local devnet: first use local devnet to basic check that it works with celestia
2. Testnet: moving to public testnet and adjusting configuration
3. Mainnet: configuration-wise it is very similar but requires production like keys security of celestia node

This tutorial only describes steps 1 and 2.

## Celestia devnet

Starter repo provides docker compose configuration for running celestia locally. 
It also has necessary configuration

First, start the celestia docker containers:

```bash,test-ci
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

Then run rollup with `celestia_da` feature and arguments for celestia

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run --no-default-features --features=celestia_da,mock_zkvm -- --rollup-config-path=configs/celestia/rollup.toml --genesis-path=configs/celestia/genesis.json
```

Log output should indicate healthy running rollup, check that REST API is responding.

```bash,test-ci,bashtestmd:compare-output
$ curl http://127.0.0.1:12346/modules/value-setter/state/value
{"value":null}
```

## Celestia testnet

Stop devnet:

```bash
make stop-celestia
```

We use `mocha` testnet in this tutorial.

You will need a celestia light node. 

For robust production setup it is recommended to connect the light node to a reliable RPC provider or use a bridge node

Check celestia documentation: 

* [Install celestia-node](https://docs.celestia.org/how-to-guides/celestia-node)
* [Setting up Celestia light node](https://docs.celestia.org/how-to-guides/light-node)

Before starting the light node, you might want to adjust these values in its config, as for getting started on testnet you will need a few of the past blocks.

Go to the block explorer, for example, https://mocha.celenium.io/, choose a recent block and use it for those values in config: 

* `Header.TrustedHash` - use block hash of selected block
* `DASer.SampleFrom` - use height of this block

This will significantly reduce the time the node needs to become synced.
But it won't be possible to start rollup from the block prior to selected.

To get address use `cel-key` utility:

```bash
$ ./cel-key list --keyring-backend test \
$    --node.type light --p2p.network mocha
using directory:  /Users/developer/.celestia-light-mocha-4/keys
- address: celestia1qd73x7lzh97uxm9lxe49qdfmuup25kp4khaxdd
  name: my_celes_key
  pubkey: '{"@type":"/cosmos.crypto.secp256k1.PubKey","key":"A2hgY3ckADmUQRO01L4J54tZhhZrfE2oGGjGV+63DJcB"}'
  type: local
```
You will need an `address` from its output

Address that celestia node is running with needs some TIA. 
For mocha testnet it can be requested via [faucet](https://docs.celestia.org/how-to-guides/mocha-testnet#mocha-testnet-faucet)

**Checking light node**

Make sure the light node is running and it's synced. 

Values `catch_up_done` and `network_head_height` and `head_of_sampled_chain` tell all necessary information

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

As described in [CLI tutorial](https://docs.celestia.org/tutorials/node-tutorial], submitting a sample blob should succeed.

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

**Preparing configuration**

* Namespace. You will need 2: 1 for batches and 1 for proofs. After you chose namespaces for you rollup they need to be updated in [`crates/rollup/src/da.rs`](crates/rollup/src/da.rs#L15). 
  They need to be committed in binary as it is a part of cryptographic commitment for the prover.
* Genesis. Address of the celestia node needs to be in updated in 2 places: [`configs/celestia/genesis.json`](configs/celestia/genesis.json):
  * `sequencer_registry.sequencer_config.seq_da_address`
  * If paymaster is used: `paymaster.payers[].sequencers_to_register`
  For simplicity you can use this command `sed -i "s/celestia1a68m2l85zn5xh0l07clk4rfvnezhywc53g8x7s/YOUR_ADDRESS/g" configs/celestia/genesis.json`
* In rollup.toml:
  * `da.celestia_rpc_auth_token` which can be fetched via `celestia light auth admin --p2p.network mocha`, as it was shown before.
  * `da.celestia_rpc_address`. Existing value should match default behaviour
  * `da.signer_address`. Rollup will pull the address for the connected node, this parameter is only needed for checking that rollup is connected to the expected node.
  * `runner.genesis_height`. For a new rollup it is better to set it closer to the current tip of the chain

If previously run on devnet, clean a database before starting
```
$ make clean-db
```

Run rollup again
```bash
$ cargo run --no-default-features \
  --features=celestia_da,mock_zkvm \
  -- --rollup-config-path=configs/celestia/rollup.toml \
  --genesis-path=configs/celestia/genesis.json
```

Node will start posting some empty batches to maintain the liveness of the rollup.

Submit transaction and check it out in the 

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

You can follow `tx_hash` in rollup logs and after its posted on DA, you can check out namespace page of your rollup and see that slightly larger batch has been published.

Success!
