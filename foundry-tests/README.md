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

### 1. Start the Rollup

From the root of the repository:

```bash
cargo run
```

The rollup will expose the RPC endpoint at `http://localhost:12346/rpc`.

### 2. Configure RPC endpoint

In a separate terminal:

```bash
cd foundry-tests
export SOV_RPC_URL=http://localhost:12346/rpc
```

### 3. Run the suites

Run the umbrella suite:

```bash
./run.sh AllTests
```

Run call-consistency checks (two-phase deploy + RPC-read flow):

```bash
./run.sh CallConsistencyFlow
```

Or run an individual suite:

```bash
./run.sh DeploymentTests
./run.sh ContextTests
./run.sh StorageTests
```

### 4. Optional signer override

By default `run.sh` uses the Anvil default key:
`0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`

Override with:

```bash
export SOV_PRIVATE_KEY=<hex-private-key>
```

Value-bearing subtests in `ValueTransferTests` and `InterContractCallTests`
run only when the active broadcast signer has enough ETH. Otherwise they are
explicitly logged as skipped.

## Available Tests

- **DeploymentTests**: Contract deployment including large contracts (512 KiB)
- **ContextTests**: Block and transaction context (timestamps, gas limits, chain ID, etc.)
- **StorageTests**: EVM storage operations
- **EventTests**: Event emission and indexed topic checks
- **MemoryTests**: Memory behavior and deterministic returns
- **PrecompileTests**: Standard precompile contract calls
- **CalldataTests**: ABI decoding and calldata handling
- **GasEstimationTests**: `eth_estimateGas` coherence checks
- **ValueTransferTests**: Native value transfer semantics
- **RevertTests**: Revert payload and panic/custom error checks
- **InterContractCallTests**: CALL/DELEGATECALL/STATICCALL behavior
- **CallConsistencyFlow**: Deploy + `eth_call` consistency read checks
- **AllTests**: Runs all in-script suites sequentially
