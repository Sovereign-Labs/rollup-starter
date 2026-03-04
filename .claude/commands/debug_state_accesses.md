# Gate detailed state accesses.

Copies the rollup state to a fresh directory and replays batches between user-specified heights with verbose state-access tracing enabled, producing detailed logs of every state read and write. The current command works for Celestia rollups but can be easily adapted to other data-availability (DA) layers.

## Variables

All variables below are collected from the user and referenced as `{variable-name}` throughout the steps.

## Steps

### 1. Collect inputs

print: 
> "Note: The state must be synced to a height just below the height being investigated so that the log volume doesn’t explode."
> "Let's gather the inputs for debugging state accesses."

Ask the user for each input one at a time. Do NOT use the AskUserQuestion tool — just print the question as plain text and wait for the user to reply with their value. Never propose or suggest default values. Wait for each response before asking the next question:

1. **Start rollup height** — assign to `{start-at-rollup-height}`.
2. **Stop rollup height** — assign to `{stop-at-rollup-height}`. Validate that `{start-at-rollup-height}` < `{stop-at-rollup-height}`. If invalid, ask the user to re-enter.
3. **State directory** — before asking, print: 
> "The state must be synced to height `{start-at-rollup-height} - 1`." Assign to `{state-dir}`.

After all inputs are collected, print:
> This script will provide detailed state access logs for the rollup between heights `{start-at-rollup-height}` and `{stop-at-rollup-height}` based on state in `{state-dir}`.

### 2. Override config

1. Enable `expensive-observability` feature in `sov-modules-api`.
Check if commit `753bf44` ("Enable expensive state debug") has already been applied by searching for the commit message (`git log --oneline | grep "Enable expensive state debug"`). If not already applied, cherry-pick it and fix any conflicts if needed. Print: "Cherry-picking commit 753bf44 — Enable expensive state debug." If already applied, print: 
> "Commit 753bf44 (Enable expensive state debug) is already applied, skipping."
2. Create variable `{state-dir-debug}` = `{state-dir}_debug`.
3. In `configs/celestia/rollup.toml`, update the `[storage]` section's `path` value to `{state-dir-debug}`.

### 3. Run the debug session

1. Create variable `{log-file-name}` = `debug_log_{start-at-rollup-height}_{stop-at-rollup-height}.txt`. 
2. Print
> "Logs are available in: `{log-file-name}`. After the rollup run ends, the log file can be used to investigate state accesses in detail—for example, to determine why two forks diverge."
3. Run the following command:

```
./scripts/debug_state_accesses.sh {state-dir} {state-dir-debug} {start-at-rollup-height} {stop-at-rollup-height} {log-file-name}
```
