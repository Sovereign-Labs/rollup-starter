#!/bin/bash
# Download a specific version of MetaMask for E2E testing

METAMASK_VERSION="${1:-13.13.1}"
EXTENSIONS_DIR="extensions"
METAMASK_DIR="${EXTENSIONS_DIR}/metamask-chrome-${METAMASK_VERSION}"
METAMASK_ZIP="${EXTENSIONS_DIR}/metamask-chrome-${METAMASK_VERSION}.zip"
DOWNLOAD_URL="https://github.com/MetaMask/metamask-extension/releases/download/v${METAMASK_VERSION}/metamask-chrome-${METAMASK_VERSION}.zip"

# Create extensions directory
mkdir -p "$EXTENSIONS_DIR"

# Check if already downloaded
if [ -d "$METAMASK_DIR" ]; then
  echo "MetaMask v${METAMASK_VERSION} already exists at ${METAMASK_DIR}"
  exit 0
fi

echo "Downloading MetaMask v${METAMASK_VERSION}..."
curl -L -o "$METAMASK_ZIP" "$DOWNLOAD_URL"

if [ ! -f "$METAMASK_ZIP" ]; then
  echo "Failed to download MetaMask"
  exit 1
fi

echo "Extracting MetaMask..."
unzip -q "$METAMASK_ZIP" -d "$METAMASK_DIR"
rm "$METAMASK_ZIP"

echo "MetaMask v${METAMASK_VERSION} installed at ${METAMASK_DIR}"
