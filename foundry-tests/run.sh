#!/bin/bash
# Run EVM tests
# Usage: ./run.sh [TestName]
# Examples:
#   ./run.sh AllTests
#   ./run.sh DeploymentTests

TEST_NAME=${1:-AllTests}

forge script \
    "$TEST_NAME" \
    --rpc-url sovereign \
    --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
    --broadcast \
    --code-size-limit 524288
