# Debug state accesses for Celestia

Collect user inputs, configure the rollup, and run a debug session that produces detailed state access logs.

## Variables

All variables below are collected from the user and referenced as `{variable-name}` throughout the steps.

## Steps

### 1. Collect inputs

Print: "Let's gather the inputs for debugging state accesses."

Ask the user for each input one at a time (do not show default options). Wait for each response before asking the next question:

1. **Start rollup height** — assign to `{start-at-rollup-height}`.
2. **Stop rollup height** — assign to `{stop-at-rollup-height}`. Validate that `{start-at-rollup-height}` < `{stop-at-rollup-height}`. If invalid, ask the user to re-enter.
3. **State directory** — before asking, print: "The state must be synced to height `{start-at-rollup-height}`." Assign to `{state-dir}`.

After all inputs are collected, print:

> This script will provide detailed state access logs for the rollup between heights `{start-at-rollup-height}` and `{stop-at-rollup-height}` based on state in `{state-dir}`.

### 2. Override config

1. Cherry pick the folloing commit `753bf44`. Fix all conflict if needed.
2. Create variable `{state-dir-debug}` = `{state-dir}_debug`.
3. In `configs/celestia/rollup.toml`, update the `[storage]` section's `path` value to `{state-dir-debug}`.

### 3. Run the debug session

Run the following command:

```
./scripts/debug.sh {state-dir} {state-dir-debug} {start-at-rollup-height} {stop-at-rollup-height}
```
