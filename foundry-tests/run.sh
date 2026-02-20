#!/bin/bash
# Run EVM tests
# Usage: ./run.sh [TestName]
# Examples:
#   ./run.sh AllTests
#   ./run.sh DeploymentTests
#   ./run.sh CallConsistencyFlow

TEST_NAME=${1:-AllTests}
# Override with SOV_PRIVATE_KEY when targeting a non-default funded signer.
PRIVATE_KEY=${SOV_PRIVATE_KEY:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}
RPC_ALIAS=sovereign
CODE_SIZE_LIMIT=524288

if [ "$TEST_NAME" = "CallConsistencyFlow" ] || [ "$TEST_NAME" = "CallConsistencyTests" ]; then
    if ! DEPLOY_OUTPUT=$(forge script \
        CallConsistencyDeploy \
        --rpc-url "$RPC_ALIAS" \
        --private-key "$PRIVATE_KEY" \
        --broadcast \
        --code-size-limit "$CODE_SIZE_LIMIT" 2>&1); then
        echo "$DEPLOY_OUTPUT"
        exit 1
    fi
    echo "$DEPLOY_OUTPUT"

    TARGET_ADDRESS=$(printf '%s\n' "$DEPLOY_OUTPUT" | sed -n 's/.*CallConsistencyTester deployed at:[[:space:]]*\(0x[a-fA-F0-9]\{40\}\).*/\1/p' | tail -n1)
    if [ -z "$TARGET_ADDRESS" ]; then
        echo "Failed to parse CallConsistency target address from deploy output"
        exit 1
    fi

    CALL_CONSISTENCY_TARGET="$TARGET_ADDRESS" forge script \
        CallConsistencyRead \
        --rpc-url "$RPC_ALIAS" \
        --private-key "$PRIVATE_KEY" \
        --broadcast \
        --code-size-limit "$CODE_SIZE_LIMIT"
    exit $?
fi

forge script \
    "$TEST_NAME" \
    --rpc-url "$RPC_ALIAS" \
    --private-key "$PRIVATE_KEY" \
    --broadcast \
    --code-size-limit "$CODE_SIZE_LIMIT"
