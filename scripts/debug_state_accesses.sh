#!/bin/bash
# Copies state to a debug directory and runs the rollup with detailed state access tracing enabled.

set -euo pipefail

# ---- Parameters ----
if [ "$#" -lt 5 ]; then
  echo "Usage: $0 <SOURCE_DIR> <DEST_DIR> <START_HEIGHT> <STOP_HEIGHT> <LOG_FILE>"
  exit 1
fi

SOURCE_DIR="$1"
DEST_DIR="$2"
START_HEIGHT="$3" 
STOP_HEIGHT="$4"
LOG_FILE="$5"

echo "Parameters:"
echo "  SOURCE_DIR:   $SOURCE_DIR"
echo "  DEST_DIR:     $DEST_DIR"
echo "  START_HEIGHT: $START_HEIGHT"
echo "  STOP_HEIGHT:  $STOP_HEIGHT"
echo "  LOG_FILE:     $LOG_FILE"

# ---- Safety check ----
if [ "$SOURCE_DIR" = "$DEST_DIR" ]; then
  echo "ERROR: SOURCE_DIR and DEST_DIR must be different!"
  exit 1
fi

if [ ! -d "$SOURCE_DIR" ]; then
  echo "ERROR: SOURCE_DIR does not exist!"
  exit 1
fi

echo "Preparing destination directory..."
mkdir -p "$DEST_DIR"
rm -rf "${DEST_DIR:?}/"*

echo "Copying contents..."
cp -r --sparse=always "$SOURCE_DIR"/. "$DEST_DIR"/

echo "Starting rollup (cargo run)..."

export RUST_LOG="warn,sov=info,sov_modules_api::containers=trace,sov_modules_api::state::traits=trace"

echo "Using start height: $START_HEIGHT, stop height: $STOP_HEIGHT"
cargo run --release --no-default-features --features celestia_da,mock_zkvm -- --start-at-rollup-height "$START_HEIGHT" --stop-at-rollup-height "$STOP_HEIGHT" > "$LOG_FILE" 2>&1
