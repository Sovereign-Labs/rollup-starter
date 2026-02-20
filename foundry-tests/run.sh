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

if [ "$TEST_NAME" = "RpcTxLifecycleFlow" ] || [ "$TEST_NAME" = "RpcTxLifecycleTests" ]; then
    if ! DEPLOY_OUTPUT=$(forge script \
        RpcTxLifecycleDeploy \
        --rpc-url "$RPC_ALIAS" \
        --private-key "$PRIVATE_KEY" \
        --broadcast \
        --code-size-limit "$CODE_SIZE_LIMIT" 2>&1); then
        echo "$DEPLOY_OUTPUT"
        exit 1
    fi
    echo "$DEPLOY_OUTPUT"

    BROADCAST_FILE=$(printf '%s\n' "$DEPLOY_OUTPUT" | sed -n 's/.*Transactions saved to:[[:space:]]*\(.*run-latest\.json\).*/\1/p' | tail -n1)
    if [ -z "$BROADCAST_FILE" ] || [ ! -f "$BROADCAST_FILE" ]; then
        echo "Failed to locate RpcTxLifecycleDeploy broadcast file"
        exit 1
    fi

    TARGET_ADDRESS=$(jq -r '.transactions[] | select(.transactionType=="CREATE" and .contractName=="RpcLifecycleTester") | .contractAddress' "$BROADCAST_FILE" | tail -n1)
    TX_PRIMARY=$(jq -r '.transactions[] | select(.transactionType=="CALL") | .hash' "$BROADCAST_FILE" | sed -n '1p')
    TX_SECONDARY=$(jq -r '.transactions[] | select(.transactionType=="CALL") | .hash' "$BROADCAST_FILE" | sed -n '2p')
    if [ -z "$TX_SECONDARY" ]; then
        TX_SECONDARY="$TX_PRIMARY"
    fi

    if [ -z "$TARGET_ADDRESS" ] || [ -z "$TX_PRIMARY" ] || [ -z "$TX_SECONDARY" ]; then
        echo "Failed to parse lifecycle deploy context from $BROADCAST_FILE"
        exit 1
    fi

    RPC_LIFECYCLE_TARGET="$TARGET_ADDRESS" \
    RPC_LIFECYCLE_TX_PRIMARY="$TX_PRIMARY" \
    RPC_LIFECYCLE_TX_SECONDARY="$TX_SECONDARY" \
    forge script \
        RpcTxLifecycleTests \
        --rpc-url "$RPC_ALIAS" \
        --private-key "$PRIVATE_KEY" \
        --broadcast \
        --ffi \
        --code-size-limit "$CODE_SIZE_LIMIT"
    exit $?
fi

FFI_FLAG=""
if [ "$TEST_NAME" = "AllTests" ] || [ "$TEST_NAME" = "RpcTagAndNonceMatrixTests" ]; then
    FFI_FLAG="--ffi"
fi

forge script \
    "$TEST_NAME" \
    --rpc-url "$RPC_ALIAS" \
    --private-key "$PRIVATE_KEY" \
    --broadcast \
    $FFI_FLAG \
    --code-size-limit "$CODE_SIZE_LIMIT"
