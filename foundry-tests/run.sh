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
    --sender 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 \
    --unlocked \
    --broadcast
