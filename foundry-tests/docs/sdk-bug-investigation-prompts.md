# SDK Bug Investigation Prompts (Failing + Commented Suites)

This file contains one ready-to-run investigation prompt per failing or commented-out suite in `foundry-tests`.

Date context: February 20, 2026.
Rollup log file: `../rollup_starter.log`.
Foundry tests root: `/Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests`.
SDK dependency pinned in `../Cargo.lock` to Sovereign SDK rev `1f5afcc00c879d3601947d09fdf9f5b17239d72b`.
Local SDK checkout root used for code anchors:
`/Users/nikolai/.cargo/git/checkouts/sovereign-sdk-de4a3e6cb918f1e3/5c0a98e`.

## Shared Output Contract (Use For Every Prompt)

Use this exact section structure in your final report:

1. `<ID>: <Short title>`
2. `Severity | Confidence | Category`
3. `Endpoints`
4. `Code locations`
5. `Expected behavior`
6. `Observed behavior`
7. `Evidence` (exact request/response pairs)
8. `Minimal repro` (raw RPC + `./run.sh` command)
9. `Root cause hypothesis`
10. `Competing hypotheses ruled out`
11. `Impact`
12. `Workaround`
13. `Fix proposal`
14. `Regression tests`
15. `Status` (`Confirmed SDK bug` | `Expected behavior` | `Harness/tooling issue` | `Needs more data`)

Global investigation rules:
- Use at least 2 repeated runs for each failing probe.
- Include absolute block numbers in addition to tags.
- Separate transport/parsing failures from semantic failures.
- If behavior is by design, cite code comments/docs proving it.

---

## Prompt 1: FEE-01 (RpcFeeAndEstimationSafetyTests)

```text
You are investigating a potential SDK bug in fee endpoint semantics and gas estimation safety.

ID: FEE-01
Title: Fee endpoint zero-values and estimation safety mismatch
Severity: Medium
Confidence: High
Category: Fee semantics / decode safety

Suite context:
- Harness: script/RpcFeeAndEstimationSafetyTests.s.sol
- Strict detector note: script/RpcFeeAndEstimationSafetyTests.s.sol:8
- Primary failing assertion: script/RpcFeeAndEstimationSafetyTests.s.sol:45
  "eth_maxPriorityFeePerGas returned zero"

Relevant endpoints:
- eth_gasPrice
- eth_maxPriorityFeePerGas
- eth_feeHistory
- eth_estimateGas

Code locations to inspect:
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:174 (eth_gasPrice)
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:187 (eth_feeHistory)
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:290 (eth_estimateGas)
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:413 (eth_maxPriorityFeePerGas)
- crates/module-system/module-implementations/sov-evm/src/rpc/fee_history.rs:38 (fee history core)

What to validate:
1) Whether eth_maxPriorityFeePerGas returning 0x0 is intentional policy or incompatible behavior.
2) Whether gas price and fee history remain internally coherent when maxPriorityFee is zero.
3) Whether estimate/execution linkage violates expected bounds (underestimation or absurd overestimation).

Expected behavior:
- eth_gasPrice and eth_maxPriorityFeePerGas should be decode-safe quantities.
- feeHistory should return consistent object fields.
- estimateGas should not succeed for guaranteed-revert calls.

Minimal repro commands:
- cd /Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests
- export SOV_RPC_URL=http://localhost:12346/rpc
- ./run.sh RpcFeeAndEstimationSafetyTests

Raw RPC repro set:
- {"jsonrpc":"2.0","id":1,"method":"eth_gasPrice","params":[]}
- {"jsonrpc":"2.0","id":1,"method":"eth_maxPriorityFeePerGas","params":[]}
- {"jsonrpc":"2.0","id":1,"method":"eth_feeHistory","params":["0x3","latest",[]]}

Deliverable:
- Confirm if this is an SDK bug vs accepted design, and if design-accepted, document compatibility impact for ethers/viem wallet fee UX.
```

---

## Prompt 2: LOG-01 (RpcLogAndFilterTests)

```text
You are investigating a potential SDK bug in eth_getLogs response shape and filter compatibility.

ID: LOG-01
Title: eth_getLogs shape mismatch (empty/non-JSON path) under Foundry harness
Severity: High
Confidence: High
Category: Response-shape / log filtering

Suite context:
- Harness: script/RpcLogAndFilterTests.s.sol
- Strict detector note: script/RpcLogAndFilterTests.s.sol:8
- Parsing path used by harness:
  - script/RpcAssertions.sol:12 (rpcJson)
  - script/RpcAssertions.sol:41 ("rpc json result must not be empty")
- Address/range filter test entry: script/RpcLogAndFilterTests.s.sol:36
- Topic wildcard test entry: script/RpcLogAndFilterTests.s.sol:101

Known failure signature to reproduce:
- "script failed: rpc json result must not be empty"

Relevant endpoints:
- eth_getLogs
- eth_getLogsWithCursor

Code locations to inspect:
- crates/full-node/sov-ethereum/src/lib.rs:130 (eth_getLogs registration)
- crates/full-node/sov-ethereum/src/handlers/get_logs.rs:33 (eth_get_logs handler)
- crates/full-node/sov-ethereum/src/handlers/get_logs/service.rs:121 (logs_for_filter)
- crates/full-node/sov-ethereum/src/handlers/get_logs/service.rs:145 (range scans)
- crates/full-node/sov-ethereum/src/handlers/get_logs/service.rs:60 (error mapping)

What to validate:
1) Whether eth_getLogs always returns valid JSON array for successful calls.
2) Whether filter range + address + topic wildcard are interpreted correctly.
3) Whether cursor/size-limit logic can surface as malformed result to callers.

Expected behavior:
- Result should be `[]` or `[log...]`, never an empty byte payload.
- Transaction/log indexes and topics should be schema-safe and consistent.

Minimal repro commands:
- cd /Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests
- export SOV_RPC_URL=http://localhost:12346/rpc
- ./run.sh RpcLogAndFilterTests

Raw RPC repro set:
- Emit test events through harness, then run:
  {"jsonrpc":"2.0","id":1,"method":"eth_getLogs","params":[{"fromBlock":"0x...","toBlock":"0x...","address":"0x..."}]}
- Wildcard topic case:
  {"jsonrpc":"2.0","id":1,"method":"eth_getLogs","params":[{"fromBlock":"0x...","toBlock":"0x...","address":"0x...","topics":["0x<topic0>",null,"0x<id-topic>"]}]}

Deliverable:
- Distinguish SDK response serialization bug vs Foundry vm.rpc decoding bug vs harness parser bug.
```

---

## Prompt 3: ERR-01 (RpcErrorEnvelopeTests)

```text
You are investigating a potential SDK bug in error/revert envelope behavior for eth_call and eth_estimateGas.

ID: ERR-01
Title: Revert payload collapse to empty data in eth_call/estimate paths
Severity: High
Confidence: High
Category: Error envelope / revert data compatibility

Suite context:
- Harness: script/RpcErrorEnvelopeTests.s.sol
- Strict detector note: script/RpcErrorEnvelopeTests.s.sol:8
- Failing assertion: script/RpcErrorEnvelopeTests.s.sol:116
  "<label>: revert data too short"
- Malformed-params matrix: script/RpcErrorEnvelopeTests.s.sol:29

Relevant endpoints:
- eth_call
- eth_estimateGas
- malformed param checks on eth_getBalance, eth_getBlockByNumber, eth_getLogs

Code locations to inspect:
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:262 (eth_call)
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:290 (eth_estimateGas)
- crates/module-system/module-implementations/sov-evm/src/rpc/error.rs:27 (ensure_success)
- crates/module-system/module-implementations/sov-evm/src/rpc/error.rs:40 (into_rpc_error)

What to validate:
1) For known reverts, does eth_call return proper ABI revert bytes (selector + payload) or empty 0x?
2) Does estimateGas return deterministic RPC error for guaranteed revert?
3) Are malformed params mapped to stable error envelopes (without transport-layer corruption)?

Expected behavior:
- Reverting eth_call should carry non-truncated revert data or a consistent RPC error object.
- estimateGas on guaranteed revert should fail, not succeed.

Minimal repro commands:
- cd /Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests
- export SOV_RPC_URL=http://localhost:12346/rpc
- ./run.sh RpcErrorEnvelopeTests

Raw RPC repro set:
- eth_call to functions that revert with:
  - Error(string)
  - Panic(uint256)
  - custom error selector
  - empty revert
- eth_estimateGas for guaranteed revert function

Deliverable:
- Confirm exact revert-data path where payload is dropped or transformed and classify as SDK bug or expected error-mode behavior.
```

---

## Prompt 4: TAG-01 (RpcTagAndNonceMatrixTests)

```text
You are investigating a potential SDK bug in block-tag semantics and nonce progression coherence.

ID: TAG-01
Title: latest/pending tag aliasing causes nonce and block coherence regressions
Severity: High
Confidence: High
Category: Internal consistency violation (tags + nonce)

Suite context:
- Harness: script/RpcTagAndNonceMatrixTests.s.sol
- Strict detector note: script/RpcTagAndNonceMatrixTests.s.sol:8
- Tag matrix test entry: script/RpcTagAndNonceMatrixTests.s.sol:37
- Failing assertion: script/RpcTagAndNonceMatrixTests.s.sol:81
  "latest nonce did not progress by two sent txs"

Relevant endpoints:
- eth_getTransactionCount
- eth_getBlockByNumber
- eth_blockNumber

Code locations to inspect:
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:141 (eth_getTransactionCount)
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:281 (eth_blockNumber)
- crates/module-system/module-implementations/sov-evm/src/rpc/mod.rs:380 (block_tag_to_pending_or_block)
- crates/module-system/module-implementations/sov-evm/src/rpc/mod.rs:425 (resolve_block_number)
- crates/module-system/module-implementations/sov-evm/src/rpc/mod.rs:690 (resolve_state_for_block_id)

What to validate:
1) Whether latest and pending are intentionally aliased in this build and what that implies for nonce reads.
2) Whether eth_blockNumber matches eth_getBlockByNumber("latest").number under current semantics.
3) Whether nonce progression after confirmed sends is delayed, stale, or read from unexpected state source.

Expected behavior:
- At minimum, internal endpoint outputs should be self-consistent.
- If latest==pending is intentional, documented invariants still must hold across nonce and block endpoints.

Minimal repro commands:
- cd /Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests
- export SOV_RPC_URL=http://localhost:12346/rpc
- ./run.sh RpcTagAndNonceMatrixTests

Raw RPC repro set:
- {"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}
- {"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["latest",false]}
- {"jsonrpc":"2.0","id":1,"method":"eth_getTransactionCount","params":["0x<sender>","latest"]}
- {"jsonrpc":"2.0","id":1,"method":"eth_getTransactionCount","params":["0x<sender>","pending"]}

Deliverable:
- Determine whether failures are true SDK regressions, expected semantics with missing harness adjustment, or race/state-visibility issues.
```

---

## Prompt 5: LIFE-01 (RpcTxLifecycleFlow, commented inline)

```text
You are investigating tx lifecycle consistency in the dedicated two-phase flow (deploy/read), currently skipped inline in AllTests.

ID: LIFE-01
Title: tx/receipt/block lifecycle consistency across RPC methods in flow mode
Severity: Medium
Confidence: Medium
Category: Flow correctness / possible SDK-harness interaction

Suite context:
- AllTests skip line: script/AllTests.s.sol:34
- Read suite: script/RpcTxLifecycleTests.s.sol
- Key consistency checks begin: script/RpcTxLifecycleTests.s.sol:44
- FFI JSON path used for object responses:
  - script/RpcAssertions.sol:16 (rpcJsonByFfi)
  - script/RpcAssertions.sol:36 (ffi rpc result must not be empty)

Relevant endpoints:
- eth_getTransactionByHash
- eth_getTransactionReceipt
- eth_getBlockByNumber
- eth_getBlockByHash
- eth_getCode

Code locations to inspect:
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:71 (eth_getBlockByNumber)
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:212 (eth_getTransactionByHash)
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:244 (eth_getTransactionReceipt)
- crates/module-system/module-implementations/sov-evm/src/rpc/mod.rs:184 (block fetch plumbing)

What to validate:
1) Whether lifecycle invariants hold end-to-end in two-phase flow.
2) Whether any residual issue is SDK-side or only from script simulation divergence.
3) Whether object-returning endpoints differ in encoding between vm.rpc and raw JSON-RPC.

Expected behavior:
- tx hash linkage and block references should match across tx/receipt/block endpoints.
- receipt polling should converge and remain stable.

Minimal repro commands:
- cd /Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests
- export SOV_RPC_URL=http://localhost:12346/rpc
- ./run.sh RpcTxLifecycleFlow

Deliverable:
- Explicitly classify this as:
  - SDK bug,
  - harness simulation issue,
  - or healthy (no bug) with rationale and evidence.
```

---

## Prompt 6: CALL-01 (CallConsistencyFlow, commented inline)

```text
You are investigating eth_call consistency in dedicated two-phase flow, currently skipped inline in AllTests.

ID: CALL-01
Title: eth_call read consistency vs execution under flow mode
Severity: Medium
Confidence: Medium
Category: Call semantics / harness-vs-SDK split

Suite context:
- AllTests skip line: script/AllTests.s.sol:51
- Read suite: script/CallConsistencyRead.s.sol
- Read-only divergence comment: script/CallConsistencyRead.s.sol:23
- eth_call probes start at:
  - script/CallConsistencyRead.s.sol:39
  - script/CallConsistencyRead.s.sol:63
  - script/CallConsistencyRead.s.sol:78

Relevant endpoints:
- eth_call
- eth_getCode

Code locations to inspect:
- crates/module-system/module-implementations/sov-evm/src/rpc/handlers.rs:262 (eth_call)
- crates/module-system/module-implementations/sov-evm/src/rpc/mod.rs:730 (resolve_block_env_for_call)
- crates/module-system/module-implementations/sov-evm/src/rpc/mod.rs:745 (basefee forced to 0 in call env)
- crates/module-system/module-implementations/sov-evm/src/helpers.rs:13 (prepare_call_env)

What to validate:
1) Whether eth_call deterministic outputs match contract pure/view expectations in this flow.
2) Whether block context fields (especially basefee) are intentionally modified and documented.
3) Whether any mismatch is SDK behavior or harness assumptions.

Expected behavior:
- computeHash/getCounter/conditional branches should be deterministic and decode-safe.
- block context values should be coherent with SDK semantics (including any deliberate basefee override).

Minimal repro commands:
- cd /Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests
- export SOV_RPC_URL=http://localhost:12346/rpc
- ./run.sh CallConsistencyFlow

Deliverable:
- Confirm if this suite belongs in "SDK bug detector" bucket or should remain a flow-only harness check.
```

---

## Optional: One-Liner To Re-run All Six Investigations

```bash
cd /Users/nikolai/workspace/sovereign/rollup-starter/foundry-tests
export SOV_RPC_URL=http://localhost:12346/rpc
./run.sh RpcFeeAndEstimationSafetyTests || true
./run.sh RpcLogAndFilterTests || true
./run.sh RpcErrorEnvelopeTests || true
./run.sh RpcTagAndNonceMatrixTests || true
./run.sh RpcTxLifecycleFlow || true
./run.sh CallConsistencyFlow || true
```

## Notes
- The strict detector suites are intentionally excluded from `AllTests` in `script/AllTests.s.sol` lines 34-51.
- Use raw JSON-RPC evidence first; use Foundry-only evidence second.
- For any claim that something is "by design", cite exact source lines and include why compatibility impact is acceptable.
