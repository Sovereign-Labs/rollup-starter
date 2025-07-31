##


## Prerequisites

Your rollup works with MockDa and you want to switch to Celestia.

Here is steps 

1. Local devnet: first use local devnet to basic check that it works with celestia
2. Testnet: moving to public testnet and adjusting configuration
3. Mainnet: 

## Celestia devnet

Starter repo provides docker compose configuration for running celestia locally. 
It also has necenssary configuration

First, start celestia node

```bash,test-ci
$ make start-celestia
```

Then run rollup

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run --no-default-features --features=celestia_da,mock_zkvm -- --rollup-config-path=configs/celestia/rollup.toml --genesis-path=configs/celestia/genesis.json
```

Log output is there, just check that REST API is responding

```bash,test-ci,bashtestmd:compare-output
$ curl http://127.0.0.1:12346/modules/value-setter/state/value
{"value":null}
```

## Celestia testnet

Stop devnet:

```bash
make stop-celestia
```

You will need a celestia light node. 

For robust production setup it is recommended to connect the light node to a reliable RPC provider or use a bridge node

Check celestia documentation how to start the light node.

Address that celestia node is running with needs some TIA. For mocha testned it can be requested via faucet.


**Preparing configuration**

* Namespace. You will need 2: 1 for batches and 1 for proofs. After you chose namespaces for you rollup they need to be updated in [`crates/rollup/src/da.rs`](crates/rollup/src/da.rs#L15). 
  They need to be commited in binary as it is a part of cryptograic ocmmitment for the prover.
* Genesis. Address of the celestia node needs to be in updated in 2 places: [`configs/celestia/genesis.json`](configs/celestia/genesis.json):
  * `sequencer_registry.sequencer_config.seq_da_address`
  * If paymaster is used: `paymaster.payers[].sequencers_to_register`
  For simplicity you can used this command `sed -i "s/celestia1a68m2l85zn5xh0l07clk4rfvnezhywc53g8x7s/YOUR_ADDRESS/g"`
* In rollup.toml:
  * `da.celestia_rpc_auth_token` which can be fetched via TBD
  * `da.celestia_rpc_address`. Existing value should match default behaviour
  * `da.signer_address`. Rollup will pull address for the connected node, this parameter only needed for checking that rollup is connected to expected node.

