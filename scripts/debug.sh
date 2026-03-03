#!/bin/bash

set -euo pipefail

# ---- Parameters ----
if [ "$#" -lt 2 ]; then
  echo "Usage: $0 <SOURCE_DIR> <DEST_DIR> [STOP_HEIGHT]"
  exit 1
fi

SOURCE_DIR="$1"
DEST_DIR="$2"
STOP_HEIGHT="${3:-}"

LOG_FILE="log.txt"

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
cp -r "$SOURCE_DIR"/. "$DEST_DIR"/

echo "Starting rollup (cargo run)..."

echo "Using stop height: $STOP_HEIGHT"
cargo run --release --no-default-features --features celestia_da,mock_zkvm -- --stop-at-rollup-height "$STOP_HEIGHT" > "$LOG_FILE" 2>&1



EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "Cargo failed with exit code $EXIT_CODE"
  exit $EXIT_CODE
fi
