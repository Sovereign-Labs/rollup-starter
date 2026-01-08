# Foundry Tests

EVM acceptance tests for Sovereign SDK rollup.

## Prerequisites

- **Foundry**: Install via [getfoundry.sh](https://getfoundry.sh/)
  ```bash
  curl -L https://foundry.paradigm.xyz | bash
  foundryup
  ```

## Setup

Install Foundry dependencies:

```bash
cd foundry-tests
forge install
```

## Running the Tests

Ensure the rollup is running and the RPC endpoint is available at `http://localhost:12346/rpc`.

```bash
cd foundry-tests
export SOV_RPC_URL=http://localhost:12346/rpc
./run.sh AllTests
```

Or run individual tests:

```bash
./run.sh DeploymentTests
./run.sh ContextTests
./run.sh StorageTests
```

## Available Tests

- **DeploymentTests**: Contract deployment including large contracts (512 KiB)
- **ContextTests**: Block and transaction context (timestamps, gas limits, chain ID, etc.)
- **StorageTests**: EVM storage operations
- **CallTests**: Contract calls and interactions
- **LogTests**: Event emission and log generation
- **SelfdestructTests**: SELFDESTRUCT opcode
- **AllTests**: Runs all tests sequentially
