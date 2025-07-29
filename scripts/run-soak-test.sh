#!/bin/bash

set -e # Exit immediately if a command exits with a non-zero status.
set -o pipefail # Causes a pipeline to return the exit status of the last command in the pipe that returned a non-zero return value.

# --- Configuration (expected to be set as environment variables) ---
SOAK_TEST_DURATION_SECONDS=${SOAK_TEST_DURATION_SECONDS:-180}
MIN_EXPECTED_THROUGHPUT=${MIN_EXPECTED_THROUGHPUT:-200}
MAX_EXPECTED_THROUGHPUT=${MAX_EXPECTED_THROUGHPUT:-5000} # Sanity check
NO_COLOR=1

ROLLUP_STARTUP_DELAY=10

NODE_RELATIVE_PATH="target/release/rollup"
SOAK_TEST_RUNNER_RELATIVE_PATH="target/release/rollup-starter-soak-test"

NODE_OUTPUT_LOG="node_output.log"
SOAK_TEST_OUTPUT_LOG="soak_test_output.log"
NODE_PID_FILE="node_pid.txt"
SOAK_TEST_PID_FILE="soak_test_pid.txt"

# --- Helper function for cleanup ---
cleanup() {
    echo "--- Running cleanup ---"
    # Terminate Node
    if [ -f "$NODE_PID_FILE" ]; then
        PID=$(cat "$NODE_PID_FILE")
        echo "Attempting to stop rollup node (PID: $PID)..."
        # Send SIGTERM
        kill -SIGTERM "$PID" &>/dev/null || echo "Rollup node (PID: $PID) already stopped or failed to send SIGTERM."
        # Wait for termination
        for _ in $(seq 1 5); do
            if ! ps -p "$PID" > /dev/null; then
                echo "Rollup node (PID: $PID) terminated gracefully."
                rm -f "$NODE_PID_FILE"
                break
            fi
            sleep 1
        done
        # Force kill if still running
        if ps -p "$PID" > /dev/null; then
            echo "Rollup node (PID: $PID) did not terminate after 5s with SIGTERM, sending SIGKILL..."
            kill -SIGKILL "$PID" &>/dev/null || echo "Failed to send SIGKILL to rollup node (PID: $PID)."
            rm -f "$NODE_PID_FILE"
        fi
    else
        echo "Node PID file ($NODE_PID_FILE) not found. Assuming rollup node was not started or already stopped."
    fi

    # Terminate Soak Test Runner
    if [ -f "$SOAK_TEST_PID_FILE" ]; then
        PID=$(cat "$SOAK_TEST_PID_FILE")
        echo "Attempting to stop soak test runner (PID: $PID)..."
        # Send SIGTERM
        kill -SIGTERM "$PID" &>/dev/null || echo "Soak test runner (PID: $PID) already stopped or failed to send SIGTERM."
        # Wait for termination
        for _ in $(seq 1 5); do
            if ! ps -p "$PID" > /dev/null; then
                echo "Soak test runner (PID: $PID) terminated gracefully."
                rm -f "$SOAK_TEST_PID_FILE"
                break
            fi
            sleep 1
        done
        # Force kill if still running
        if ps -p "$PID" > /dev/null; then
            echo "Soak test runner (PID: $PID) did not terminate after 5s with SIGTERM, sending SIGKILL..."
            kill -SIGKILL "$PID" &>/dev/null || echo "Failed to send SIGKILL to soak test runner (PID: $PID)."
            rm -f "$SOAK_TEST_PID_FILE"
        fi
    else
        echo "Soak test PID file ($SOAK_TEST_PID_FILE) not found. Assuming soak test runner was not started or already stopped."
    fi
    echo "--- Cleanup finished ---"
}

# Trap EXIT, INT, TERM signals to ensure cleanup runs
trap cleanup EXIT INT TERM

# --- Main script ---
echo "Starting rollup node in background..."
if [ ! -f "$NODE_RELATIVE_PATH" ]; then
    echo "::error::Rollup node executable not found at $NODE_RELATIVE_PATH"
    exit 1
fi
NO_COLOR=1 RUST_LOG="info,sov_modules_stf_blueprint::sequencer_mode::common=warn" "$NODE_RELATIVE_PATH" --rollup-config-path configs/mock/rollup.toml --genesis-path configs/mock/genesis.json > "$NODE_OUTPUT_LOG" 2>&1 &
echo $! > "$NODE_PID_FILE"
echo "Rollup node started in background with PID $(cat "$NODE_PID_FILE"). Output: $NODE_OUTPUT_LOG"

echo "Sleeping $ROLLUP_STARTUP_DELAY seconds to let rollup startup"
sleep "$ROLLUP_STARTUP_DELAY"

echo "Starting soak test runner in background..."
if [ ! -f "$SOAK_TEST_RUNNER_RELATIVE_PATH" ]; then
    echo "::error::Soak test runner executable not found at $SOAK_TEST_RUNNER_RELATIVE_PATH"
    exit 1
fi
NO_COLOR=1 RUST_LOG="sov_soak_testing=debug" "$SOAK_TEST_RUNNER_RELATIVE_PATH" > "$SOAK_TEST_OUTPUT_LOG" 2>&1 &
echo $! > "$SOAK_TEST_PID_FILE"
echo "Soak test runner started in background with PID $(cat "$SOAK_TEST_PID_FILE"). Output: $SOAK_TEST_OUTPUT_LOG"

echo "Running rollup for $SOAK_TEST_DURATION_SECONDS seconds..."
sleep "$SOAK_TEST_DURATION_SECONDS"
echo "Done. Initiating shutdown of processes..."

kill -SIGTERM "$(cat "$SOAK_TEST_PID_FILE")" || echo "Soak test runner already stopping/stopped."
sleep 5
kill -SIGTERM "$(cat "$NODE_PID_FILE")" || echo "Node already stopping/stopped."


echo "Waiting a few seconds for processes to write final logs and attempt graceful shutdown..."
sleep 5 # Give some time for processes to react to SIGTERM and flush logs before checks.

echo "--- Checking logs (after attempting shutdown) ---"

echo "Checking rollup node logs ($NODE_OUTPUT_LOG)..."
if [ ! -f "$NODE_OUTPUT_LOG" ]; then
    # This could happen if the node failed to start and redirect output.
    # The startup itself might have logged an error if the executable was missing.
    echo "::warning::Rollup node output log ($NODE_OUTPUT_LOG) not found. This might indicate an early failure of the node."
    # If the PID file also doesn't exist or the process isn't running, it's a strong indicator of failure.
    if [ ! -f "$NODE_PID_FILE" ] || ( [ -f "$NODE_PID_FILE" ] && ! ps -p "$(cat "$NODE_PID_FILE")" > /dev/null ) ; then
        echo "::error::Rollup node seems to have failed to start or crashed very early. Log file $NODE_OUTPUT_LOG is missing."
        exit 1 # Treat missing log for a process that should have run as an error
    fi
else
    if grep -Eq 'ERROR' "$NODE_OUTPUT_LOG"; then
        echo "::error file=$NODE_OUTPUT_LOG::Found ERROR in rollup node logs."
        echo "--- $NODE_OUTPUT_LOG contents ---"
        cat "$NODE_OUTPUT_LOG"
        echo "--- end of $NODE_OUTPUT_LOG ---"
        exit 1 # Fail the script
    else
        echo "No ERROR found in rollup node logs."
    fi
fi

echo "Checking soak test runner logs ($SOAK_TEST_OUTPUT_LOG)..."
if [ ! -f "$SOAK_TEST_OUTPUT_LOG" ]; then
    echo "::warning::Soak test runner output log ($SOAK_TEST_OUTPUT_LOG) not found. This might indicate an early failure of the soak test runner."
    if [ ! -f "$SOAK_TEST_PID_FILE" ] || ( [ -f "$SOAK_TEST_PID_FILE" ] && ! ps -p "$(cat "$SOAK_TEST_PID_FILE")" > /dev/null ) ; then
        echo "::error::Soak test runner seems to have failed to start or crashed very early. Log file $SOAK_TEST_OUTPUT_LOG is missing. Throughput check will fail."
        exit 1
    fi
    # If the process was expected to run and log, a missing log is an error for throughput check.
    echo "::error::$SOAK_TEST_OUTPUT_LOG not found. Cannot extract throughput."
    exit 1
else
    if grep -Eq 'ERROR' "$SOAK_TEST_OUTPUT_LOG"; then
        echo "::error file=$SOAK_TEST_OUTPUT_LOG::Found ERROR in soak test runner logs."
        echo "--- $SOAK_TEST_OUTPUT_LOG contents ---"
        cat "$SOAK_TEST_OUTPUT_LOG"
        echo "--- end of $SOAK_TEST_OUTPUT_LOG ---"
        exit 1 # Fail the script
    else
        echo "No ERROR found in soak test runner logs."
    fi
fi

echo "--- Extracting and asserting running throughput ---"
# Re-check soak_test_output.log existence because previous block might not exit if only warning was issued
if [ ! -f "$SOAK_TEST_OUTPUT_LOG" ]; then
    echo "::error::$SOAK_TEST_OUTPUT_LOG still not found before throughput extraction. This indicates a critical failure."
    exit 1
fi

THROUGHPUT_LINE=$(grep 'Running throughput:' "$SOAK_TEST_OUTPUT_LOG" | tail -n 1)
if [ -z "$THROUGHPUT_LINE" ]; then
    echo "::error::No 'Running throughput' log line found in $SOAK_TEST_OUTPUT_LOG."
    echo "--- $SOAK_TEST_OUTPUT_LOG contents ---"
    cat "$SOAK_TEST_OUTPUT_LOG"
    echo "--- end of $SOAK_TEST_OUTPUT_LOG ---"
    exit 1
fi
echo "Last relevant log line: $THROUGHPUT_LINE"

RUNNING_THROUGHPUT_STR=$(echo "$THROUGHPUT_LINE" | awk -F'Running throughput: ' '{print $2}' | awk -F' txs per second' '{print $1}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

# Validate if RUNNING_THROUGHPUT_STR is a number that awk can process
if ! awk -v val="$RUNNING_THROUGHPUT_STR" 'BEGIN { if (val == "" || val+0 != val) exit 1; exit 0; }'; then
    echo "::error::Failed to parse a valid numeric throughput value from line: '$THROUGHPUT_LINE'. Extracted: '$RUNNING_THROUGHPUT_STR' (not a valid number for awk)."
    echo "--- $SOAK_TEST_OUTPUT_LOG contents ---"
    cat "$SOAK_TEST_OUTPUT_LOG"
    echo "--- end of $SOAK_TEST_OUTPUT_LOG ---"
    exit 1
fi

echo "Extracted running throughput: $RUNNING_THROUGHPUT_STR txs per second."
echo "Asserting throughput $RUNNING_THROUGHPUT_STR is between $MIN_EXPECTED_THROUGHPUT and $MAX_EXPECTED_THROUGHPUT..."

if awk -v rt="$RUNNING_THROUGHPUT_STR" -v min="$MIN_EXPECTED_THROUGHPUT" -v max="$MAX_EXPECTED_THROUGHPUT" \
    'BEGIN { if (rt >= min && rt <= max) exit 0; else exit 1; }'; then
    echo "✅ Running throughput $RUNNING_THROUGHPUT_STR is within the expected range [$MIN_EXPECTED_THROUGHPUT, $MAX_EXPECTED_THROUGHPUT]."
else
    echo "::error::Running throughput $RUNNING_THROUGHPUT_STR is NOT within the expected range [$MIN_EXPECTED_THROUGHPUT, $MAX_EXPECTED_THROUGHPUT]."
    # No need to cat log again, it's done if extraction fails or errors/warns are found.
    exit 1
fi

echo "Soak test completed successfully."
# PID files will be removed by the cleanup trap if processes exited.
# If we reach here, it means all checks passed.
exit 0 