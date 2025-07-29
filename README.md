# Overview

This repository provides a starting point for building rollups with the Sovereign SDK. 

It includes everything you need to create a rollup with customizable modules, REST API for state queries, TypeScript SDK for submitting transactions, WebSocket endpoints to subscribe to transactions and events, built-in token management, and much more.

## Repository Structure

- `crates/stf`: Contains the State Transition Function (STF) derived from the Runtime, used by both the rollup and prover crates
- `crates/provers`: Generates proofs for the STF
- `crates/rollup`: Runs the main rollup binary. This includes both the full-node and the soft-confirming sequencer (as well as replica + fail-over logic.)
- `examples/value-setter`: Example of Sovereign Rollup module.

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
git clone https://github.com/Sovereign-Labs/sov-rollup-starter.git
cd sov-rollup-starter
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
open http://localhost:12346/swagger-ui/#/ 
```

### Example: Query the `ValueSetter` Module's state value

For now, you should just see null returned for the value state item, as the item hasn't been initialized:

```bash,test-ci,bashtestmd:compare-output
$ curl http://127.0.0.1:12346/modules/value-setter/state/value
{"value":null}
```

## Programmatic Interaction with Typescript

### Set up the Typescript client:

```bash,test-ci,bashtestmd:exit-code=0
$ cd examples/starter-js && npm install
```

### The Typescript script demonstrates the complete transaction flow:

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

You can also subscribe to events from the sequencer (you need to uncomment the subscription code blocks [in the script](/examples/starter-js/src/index.ts#L35)):

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

To interact with different modules, simply change the call message. The top-level key corresponds to the [module's variable name in the runtime](/crates/stf/src/runtime.rs#L85), and the nested key is the [CallMessage](crates/value-setter/src/lib.rs#L90) enum variant in snake_case:

```js
// Example: Call the ValueSetter's SetValue method
let callMessage: RuntimeCall = {
  value_setter: {  // Must match Runtime field name of the module
    set_value: 10  
  },
};
```

This transaction would set the ValueSetter's state value to 10. Try setting the [example file's call message](js/src/index.ts#L39) to the expression above and re-running the script. Then verify that the ValueSetter's value changed using [the curl command](#example-query-the-value-setters-state-value) we showed earlier. 

This time, the curl command should return:
```json
{"value":10}
```

### Learn more

To learn more about building with Sovereign SDK, experiment with the [ValueSetter](/crates/value-setter/src/lib.rs). For a deeper understanding of the abstractions, see the [Building a module](https://docs.sovereign.xyz/rollup-devs/build-a-module.html) section of the SDK book.


## Alternative Configurations

### Using Different DA Layers and zkVMs

The examples above use mock DA and zkVM for simplicity. To use Celestia DA with Risc0 zkVM:

```bash
$ cargo run --no-default-features --features celestia_da,risc0
```

### Enabling the Prover

Proving is disabled by default. Enable it with these environment variables:

- `export SOV_PROVER_MODE=skip` - Skip verification logic
- `export SOV_PROVER_MODE=simulate` - Run verification logic in the current process
- `export SOV_PROVER_MODE=execute` - Run verifier in a zkVM executor
- `export SOV_PROVER_MODE=prove` - Run verifier and create a SNARK proof

### Paymaster Configuration

By default, the gas costs of transactions submitted by the preferred sequencer are covered by the paymaster at address `0xA6edfca3AA985Dd3CC728BFFB700933a986aC085`. You can modify this in the [configuration file](configs/mock/genesis.json#L65).

To run without a paymaster, use our configuration file that does not have a paymaster setting configured:

```bash
$ cargo run --rollup_config_path="configs/mock/genesis_without_paymaster.json"
```

## Troubleshooting

### Common Issues

**"Address already in use" error when starting the node**
- Another process is using port 12346. Either kill that process or modify the `bind_port` in your [rollup configuration file](configs/mock/rollup.toml#L28)

**Transaction fails with "insufficient funds"**
- If using the default configuration with paymaster, ensure the [paymaster address](configs/mock/genesis.json#L68) is correctly configured
- If running without paymaster, ensure your account has sufficient balance for gas fees

**"Module not found" errors in TypeScript**
- Run `npm install` in the `examples/starter-js` directory
- Ensure you're using Node.js 20.0 or later

**Rollup node crashes on startup**
- Try cleaning the database with `make clean-db` and restart
- Verify you're using the correct Rust version (1.88.0 or later)

## Additional Resources
For more details, visit the [Sovereign SDK documentation](https://docs.sovereign.xyz).
