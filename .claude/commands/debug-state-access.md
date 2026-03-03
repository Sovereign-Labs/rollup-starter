# Debug state accesses for Celestia

Collect user inputs, configure the rollup, and run a debug session that produces detailed state access logs.

## Variables

All variables below are collected from the user and referenced as `{variable-name}` throughout the steps.

## Steps

### 1. Collect inputs

Print: "Note: The state must be synced to a height just below the fork height so that the log volume doesn't explode."
Print: "Let's gather the inputs for debugging state accesses."

Ask the user for each input one at a time. Do NOT use the AskUserQuestion tool — just print the question as plain text and wait for the user to reply with their value. Never propose or suggest default values. Wait for each response before asking the next question:

1. **Start rollup height** — assign to `{start-at-rollup-height}`.
2. **Stop rollup height** — assign to `{stop-at-rollup-height}`. Validate that `{start-at-rollup-height}` < `{stop-at-rollup-height}`. If invalid, ask the user to re-enter.
3. **State directory** — before asking, print: "The state must be synced to at least height `{start-at-rollup-height}`." Assign to `{state-dir}`.

After all inputs are collected, print:

> This script will provide detailed state access logs for the rollup between heights `{start-at-rollup-height}` and `{stop-at-rollup-height}` based on state in `{state-dir}`.

### 2. Override config

1. Check if commit `753bf44` ("Enable expensive state debug") has already been applied by searching for the commit message (`git log --oneline | grep "Enable expensive state debug"`). If not already applied, cherry-pick it and fix any conflicts if needed. Print: "Cherry-picking commit 753bf44 — Enable expensive state debug." If already applied, print: "Commit 753bf44 (Enable expensive state debug) is already applied, skipping."
2. Create variable `{state-dir-debug}` = `{state-dir}_debug`.
3. In `configs/celestia/rollup.toml`, update the `[storage]` section's `path` value to `{state-dir-debug}`.

### 3. Run the debug session

1. Create variable `{log-file-name}` = `debug_log_{start-at-rollup-height}_{stop-at-rollup-height}.txt`. 
2. Print the following message to the console: 
> "Logs are available in: `{log-file-name}`."
3. Run the following command:

```
./scripts/debug.sh {state-dir} {state-dir-debug} {start-at-rollup-height} {stop-at-rollup-height} {log-file-name}
```


