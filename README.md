# Relay Chain

This repository contains the complete code for the relay chain full node.

## Prerequisites

Before you begin, ensure you have the following installed:

- **Rust**: 1.88.0 or later
  - Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
  - The project will automatically install the correct version via `rust-toolchain.toml`
- **Git**: For cloning the repository


You will also need the following config params:

- **Celestia RPC**
  - The easiest setup is via quicknode: <https://www.quicknode.com/docs/celestia/quickstart>
  - If you prefer to bring your own node, you'll need rpc acces to a "bridge" node and grpc access to a "consensus" node.

# Getting Started

## Running the Node

The following steps set up a "read-only" node which can handle RPC queries but not submit transactions:

### 1. Clone the repository and navigate to the rollup directory:

```bash
git clone https://github.com/Sovereign-Labs/rollup-starter.git
cd rollup-starter
```
### 2. Update your config

Open `configs/celestia/rollup.toml` in your favorite editor and fill in your RPC info.

```toml
[da]
rpc_url = "https://my-celestia-node.celestia-mainnet.quiknode.pro/MY_API_TOKEN"
grpc_url = "https://my-celestia-node.celestia-mainnet.quiknode.pro:9090"
grpc_auth_token = "MY_API_TOKEN"
```

### 3. Start the rollup node and wait for sync

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run --release
```
You can monitor the rollup's sync status via API.
```bash
$ curl localhost:12346/rollup/sync-status
```

### 4. Interact with the node via JSON-RPC
```
$ curl -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["latest"]}'  http://localhost:12346/rpc
```

## Submitting Transactions

### Prerequisite: Obtaining Gas

Before submitting transactions on Relay chain, you'll need to obtain ETH on Relay chain to pay gas. This can be done by bridging ETH from L1 Hyperlane (instructions can be found [here] (TODO: link))

### (Recommended) Submitting via Preferred Sequencer

Once submit transactions via the preferred sequencer, send an `eth_sendRawTransaction` request to  `https://rpc.chain.relay.link/rpc` using your favorite ethereum tooling. Transactions submitted through this link provide reliable soft-confirmations and near-instant finality.


### (Not Recommended) Self-Sequencing transactions

Self-sequenced transactions do not enjoy the same guarantees provided by the preferred sequencer. Typically, they'll see delays of 3-15 seconds before finality, but this can be up to 24 hours under some (extremely rare) conditions.

If you wish to self-sequence transactions, you'll need to obtain and fund a celestia address. See <https://docs.celestia.org/operate/keys-wallets/celestia-node-key/>.

Paste the private key hex into your rollup config file (`configs/celestia/rollup.toml`): 

```rust
signer_private_key = "..." # Hex private key. Do not prefix with `0x`
```

Then, submit your transaction in the usual way using `eth_sendRawTransaction`.

## Observability stack

This starter repo has a helper command to spin up the local observability stack for your rollup. Just run `make start-obs`, 
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



## Additional Resources
For more details, visit the [Sovereign SDK documentation](https://docs.sovereign.xyz).
