## Acceptance Test

This crate runs a test which syncs the rollup against a known set of block and asserts that all
of the *ledger* state responses are as expected. This guarantees the correct state roots are being
calculated - which transitively guarantees that the state is correct. After resyncing, we run a 
soak test for a fixed length of time and ensure that (1) there are no errors and (2) the throughput
is within the expected range.

To run the test simply `cargo run --bin acceptance-test`. All data should have been prepopulated.
`build.rs` compiles the EVM state consistency contract via `solc` at build time, so ensure it is available on PATH.

The test is meant to be idempotent. It deletes any possible leftover files at the beginning of each run.
However, in case of errors it can sometimes be the case that docker containers haven't been shut down 
from the previous run. To fix, simply `docker rm -f postgres-acceptance-test`.


### Resetting the Test

If you need to generate a new test, simply run `rm -r acceptance-test-data && cargo run --bin setup`. This will generate all of the 
needed files, including a fresh mockDA. Note that setup may take an hour or more to run, since we have to generate a full history
for the rollup.

### RPC Compatibility Checks

Use the compatibility runner to validate high-priority Ethereum RPC invariants against a live rollup endpoint:

```bash
cargo run -p acceptance-test --bin rpc-compat -- --rpc-url http://127.0.0.1:12346/rpc
```

What it validates:

- Decode-safe core quantity responses (`eth_chainId`, `eth_blockNumber`, `eth_gasPrice`)
- `eth_feeHistory` response shape coherence
- Block-tag behavior for `eth_getBlockByNumber` and `eth_getTransactionCount`
- `eth_call` / `eth_estimateGas` / executed tx consistency
- Tx/receipt/block cross-endpoint consistency
- `eth_getLogs` linkage with emitted events
- Nonce progression under pending/latest semantics
