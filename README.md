# Overview

This repository provides a starting point for building rollups with the Sovereign SDK. 

It includes everything you need to create a rollup with customizable modules, REST API for state queries, TypeScript SDK for submitting transactions, WebSocket endpoints to subscribe to transactions and events, built-in token management, and much more.

**Note:** The Sovereign SDK is provided under a revenue share agreement for commercial applications. See the [LICENSE](/LICENSE.md) file for more details.

## Repository Structure

- `crates/stf`: Contains the State Transition Function (STF) derived from the Runtime, used by both the rollup and prover crates
- `crates/provers`: Generates proofs for the STF
- `crates/rollup`: Runs the main rollup binary. This includes both the full-node and the soft-confirming sequencer (as well as replica + fail-over logic.)
- `examples/value-setter`: Example module.

## Prerequisites

Before you begin, ensure you have the following installed:

- **Rust**: 1.88.0 or later
  - Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
  - The project will automatically install the correct version via `rust-toolchain.toml`
- **Node.js**: 20.0 or later (for TypeScript client)
  - Install via [official website](https://nodejs.org/en/download)
- **Git**: For cloning the repository

### Optional Tools

The following tools are optional and only needed for specific features:

- **RISC Zero toolchain**: For generating zero-knowledge proofs with RISC Zero (not needed for initial development)
- **SP1 toolchain**: For generating zero-knowledge proofs with SP1 (not needed for initial development)

> **Note:** Start with the mock DA and zkVM configurations shown below. You can add the optional tools later when needed.

# Getting Started

## Running with Mock DA

### 1. Clone the repository and navigate to the rollup directory:

```bash
git clone https://github.com/Sovereign-Labs/rollup-starter.git
cd rollup-starter
```

### 2. (Optional) Clean the database for a fresh start:

```bash,test-ci
$ make clean-db
```

### 3. Start the rollup node:

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run
```

### Explore the REST API endpoints via Swagger UI

The rollup includes several built-in modules: Bank (for token management), Paymaster, Hyperlane, and more. You can query any state item in these modules:

```bash
open http://127.0.0.1:12346/swagger-ui/#/ 
```

### Example: Query the `ValueSetter` Module's state value

For now, you should just see null returned for the value state item, as the item hasn't been initialized:

```bash,test-ci,bashtestmd:compare-output
$ curl http://127.0.0.1:12346/modules/value-setter/state/value
{"value":null}
```

## Programmatic Interaction with TypeScript

### Set up the TypeScript client:

```bash,test-ci,bashtestmd:exit-code=0
$ cd examples/starter-js && npm install
```

### The TypeScript script demonstrates the complete transaction flow:

```js
// 1. Initialize rollup client
// defaults to http://localhost:12346, or pass url: "custom-endpoint"
const rollup = await createStandardRollup();

// 2. Initialize signer
const privKey = "0d87c12ea7c12024b3f70a26d735874608f17c8bce2b48e6fe87389310191264";
let signer = new Secp256k1Signer(privKey, chainHash);

// 3. Create a transaction (call message)
let callMessage: RuntimeCall = {
  bank: {
    create_token: {
      admins: [],
      token_decimals: 8,
      supply_cap: 100000000000,
      token_name: "Example Token",
      initial_balance: 1000000000,
      mint_to_address: signerAddress, // derived from privKey above (can be any valid address)
    },
  },
};

// 4. Send transaction
let tx_response = await rollup.call(callMessage, { signer });
```

### Run the script

You should see a transaction soft-confirmation with events:

```bash,test-ci,bashtestmd:exit-code=0
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

### Subscribe to events from the sequencer:

You can also subscribe to events from the sequencer (you need to uncomment the subscription code blocks [in the script](/examples/starter-js/src/index.ts#L42)):

```js
// Subscribe to events
async function handleNewEvent(event: any): Promise<void> {
  console.log(event);
}
const subscription = rollup.subscribe("events", handleNewEvent);

// Unsubscribe
subscription.unsubscribe();
```

### Interacting with different modules

To interact with different modules, simply change the call message. 
The top-level key corresponds to the [module's variable name in the runtime](/crates/stf/stf-declaration/src/lib.rs#L86), 
and the nested key is the [CallMessage](examples/value-setter/src/lib.rs#L61) enum variant in snake_case:

```js
// Example: Call the ValueSetter's SetValue method
let callMessage: RuntimeCall = {
  value_setter: {  // Must match Runtime field name of the module
    set_value: 10  
  },
};
```

This transaction would set the ValueSetter's state value to 10. Try setting the [example file's call message](examples/starter-js/src/index.ts#L29) to the expression above and re-running the script. Then verify that the ValueSetter's value changed using [the curl command](#example-query-the-valuesetter-modules-state-value) we showed earlier. 

This time, the curl command should return:
```json
{"value":10}
```

### Learn more

To learn more about building with Sovereign SDK, experiment with the [ValueSetter](/examples/value-setter/src/lib.rs). For a deeper understanding of the abstractions, see the [Quickstart: Your First Module](https://docs.sovereign.xyz/3-quickstart.html) section of the SDK book.

## Observability stack

Starter repo has a helper command to spin up the local observability stack for your rollup. Just run `make start-obs`, 
and it will spin up all necessary Docker containers and provision Grafana dashboards for the rollup:

```bash
$ make start-obs
...
Waiting for all services to become healthy...
⏳ Waiting for services... (45 seconds remaining)
✅ All observability services are healthy!

🚀 Observability stack is ready:
   - Grafana:     http://localhost:3000 (admin/admin123)
   - InfluxDB:    http://localhost:8086 (admin/admin123)
```

To stop it run `make stop-obs` and it will shut down all containers.

Learn more in our [Observability Tutorial](https://sovlabs.notion.site/Tutorial-Getting-started-with-Grafana-Cloud-17e47ef6566b80839fe5c563f5869017?pvs=74).


## Alternative Configurations

### Using Different DA Layers and zkVMs

The examples above use mock DA and zkVM for simplicity. To use Celestia DA with Risc0 zkVM:

```bash
$ make start-celestia # this will spin up celestia devnet container
$ cargo run --no-default-features --features celestia_da,risc0
```

### Enabling the Prover

Proving is disabled by default. Enable it with these environment variables before recompiling the rollup:

- `export SOV_PROVER_MODE=skip` - Skip verification logic
- `export SOV_PROVER_MODE=simulate` - Run verification logic in the current process
- `export SOV_PROVER_MODE=execute` - Run verifier in a zkVM executor
- `export SOV_PROVER_MODE=prove` - Run verifier and create a SNARK proof

### Paymaster Configuration

By default, the gas costs of transactions submitted by the preferred sequencer are covered by the paymaster at address `0xA6edfca3AA985Dd3CC728BFFB700933a986aC085`. 
You can modify this in the [configuration file](configs/mock/genesis.json#L65).

To run without a paymaster, just remove all payers from `paymaster` section:

```json
{
  "paymaster": {
    "payers": []
  }
}
```

With this change, the gas cost of each transaction will be covered by the sender of the transaction.

## Troubleshooting

### Common Issues

**"Address already in use" error when starting the node**
- Another process is using port 12346. Either kill that process or modify the `bind_port` in your [rollup configuration file](configs/mock/rollup.toml#L28)

**Transaction fails with "insufficient funds"**
- If using the default configuration with paymaster, ensure the [paymaster address](configs/mock/genesis.json#L62) is correctly configured
- If running without paymaster, ensure your account has sufficient balance for gas fees

**"Module not found" errors in TypeScript**
- Run `npm install` in the `examples/starter-js` directory
- Ensure you're using Node.js 20.0 or later

**Rollup node crashes on startup**
- Try cleaning the database with `make clean-db` and restart
- Verify you're using the correct Rust version (1.88.0 or later)

**Rollup crashed with `buckets exhausted` error**
- Increase parameter `storage.user_hashtable_buckets`
- Clean the rollup database and resync from DA layer

## Additional Resources
For more details, visit the [Sovereign SDK documentation](https://docs.sovereign.xyz).
