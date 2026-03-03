# Debug state acceses for celestia

## Steps

### 1. Prepare input.

1. Ask user for inputt to the script. Don't show default options, but wait for each input.
Write the following message "Let's gather the inputs for debugging state accesses."
Ask the user for:
  - branch name. 
        Assign to {branch-name}.
  - A start-at-rollup-height. 
        Assign to {start-at-rollup-height}.
  - A stop-at-rollup-height. 
        Assign to {stop-at-rollup-height} and validate that {start-at-rollup-height} < {stop-at-rollup-height}.
  - State directory 
         (Print the follwong message "The state must be synced to {start-at-rollup-height}").
         Assign to {state-dir}.  

3. Write the following output:
 "This skill will provide detailed state access logs for rollup on a {branch-name} between heights {start-at-rollup-height} {stop-at-rollup-height} based on state in {state-dir}"


### 2. Override config

1. git checkout {branch-name}
2. create variable {state-dir-debug} = {state-dir}_debug
3. Override path in configs/celestia/rollup.toml with
[storage]
path = {state-dir-debug}

### 3. Run the debuh session

1. Print the following
{start-at-rollup-height}, {stop-at-rollup-height}, {state-dir}, {state-dir-debug}
