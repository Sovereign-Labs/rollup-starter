# Overview

This readme explains how to run the rollup using an external mock DA.
Before proceeding, make sure you’ve read the main README.md, as we’ll skip explanations already covered there.

### 1. Start the external mock-da:

```bash,test-ci
$ make clean-db
```

```bash,test-ci,bashtestmd:long-running
$ cargo run --bin mock-da-server --no-default-features --features="mock_da_external,mock_zkvm"
```

### 2. Start the rollup node:

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run --no-default-features --features="mock_da_external,mock_zkvm"
```
