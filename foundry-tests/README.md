# Foundry Tests

This directory contains Foundry-based acceptance tests for the EVM module of the Sovereign SDK rollup.

## Prerequisites

Before running the tests, ensure you have the following installed:

- **Foundry**: Install via [getfoundry.sh](https://getfoundry.sh/)
  ```bash
  curl -L https://foundry.paradigm.xyz | bash
  foundryup
  ```
- **Docker**: Required for running the rollup (if using Docker)
- **Rust**: Required for building the rollup from source (if running natively)

## Setup

### 1. Install Foundry Dependencies

```bash
cd foundry-tests
forge install
```

### 2. Build the Foundry Contracts

```bash
forge build
```

## Running the Tests

The tests require a running rollup node with the EVM module enabled. You have two options:

### Option A: Using Docker (Recommended)

This is the easiest way to get started. The rollup will run in a Docker container with all dependencies included.

#### Step 1: Build the Docker Image

From the root of the repository:

```bash
make build-docker-mock-da
```

#### Step 2: Start the Rollup in Background

```bash
make run-docker-mock-da BACKGROUND=true
```

This will:
- Start the rollup node with MockDA
- Expose the RPC endpoint at `http://localhost:12346`
- Store data in `test-data/docker/`

#### Step 3: Set Environment Variable and Run Tests

```bash
cd foundry-tests
export SOV_RPC_URL=http://localhost:12346
./run.sh AllTests
```

Or run individual tests:

```bash
./run.sh DeploymentTests
./run.sh ContextTests
./run.sh StorageTests
# ... etc
```

#### Step 4: Stop the Rollup

When you're done testing:

```bash
make stop-docker-mock-da
```

### Option B: Running Natively (From Source)

If you prefer to run the rollup directly without Docker:

#### Step 1: Clean Database (Optional)

```bash
make clean-db
```

#### Step 2: Start the Rollup

From the root of the repository:

```bash
cargo run
```

The rollup will start and expose the RPC endpoint at `http://localhost:12346` by default.

#### Step 3: Set Environment Variable and Run Tests

In a separate terminal:

```bash
cd foundry-tests
export SOV_RPC_URL=http://localhost:12346
./run.sh AllTests
```

## Available Tests

The test suite includes:

- **DeploymentTests**: Tests contract deployment including large contracts (up to 512 KiB)
- **ContextTests**: Tests block and transaction context (timestamps, gas limits, chain ID, etc.)
- **StorageTests**: Tests EVM storage operations (SLOAD, SSTORE)
- **CallTests**: Tests contract calls and interactions
- **LogTests**: Tests event emission and log generation
- **SelfdestructTests**: Tests SELFDESTRUCT opcode
- **AllTests**: Runs all tests sequentially

## Running Individual Tests

You can run any test script individually:

```bash
export SOV_RPC_URL=http://localhost:12346
./run.sh <TestName>
```

Examples:
```bash
./run.sh DeploymentTests
./run.sh ContextTests
./run.sh StorageTests
```

## Configuration

The tests are configured via:

- **foundry.toml**: Foundry configuration file
  - RPC endpoint is read from `SOV_RPC_URL` environment variable
  - Code size limit is set to 524,288 bytes (512 KiB) to support large contracts
- **run.sh**: Test runner script
  - Uses unlocked account mode (`--unlocked`) for easier testing
  - Broadcasts transactions to the network

## Troubleshooting

### Error: "environment variable SOV_RPC_URL not found"

Make sure you've set the environment variable:

```bash
export SOV_RPC_URL=http://localhost:12346
```

Or set it inline when running tests:

```bash
SOV_RPC_URL=http://localhost:12346 ./run.sh AllTests
```

### Error: "connection refused" or "Failed to send transaction"

The rollup node is not running or not accessible. Make sure:

1. The rollup is running (check with `docker ps` or verify the cargo process)
2. The RPC endpoint is accessible: `curl http://localhost:12346/health`
3. The port 12346 is not blocked by a firewall

### Tests timeout or fail with "OutOfGas"

Some tests (especially large contract deployments) may exceed the block gas limit when broadcasting. This is expected behavior in simulation mode - the tests validate correctness even if the broadcast fails.

### Docker container fails to start

Make sure no other processes are using port 12346:

```bash
lsof -i :12346
```

Clean up any leftover containers:

```bash
docker rm -f rollup-mock-da
```

## Test Structure

Each test script follows this pattern:

```solidity
contract MyTests is Script {
    function run() public {
        vm.startBroadcast();

        // Deploy contracts and run tests
        testFunction1();
        testFunction2();

        vm.stopBroadcast();
    }
}
```

Tests use:
- `console2.log()` for output
- `require()` for assertions
- Forge's cheatcodes (`vm.*`) for test control

## Development

To add new tests:

1. Create a new Solidity contract in `script/`
2. Implement test functions
3. Run with `./run.sh YourTestName`

See existing test scripts for examples.
