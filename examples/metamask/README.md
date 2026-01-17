# MetaMask EIP-712 Example

This example demonstrates how to use MetaMask with Sovereign SDK rollups using EIP-712 typed data signing.

## Overview

This example shows the complete flow for:
- Adding the rollup network to MetaMask
- Connecting to MetaMask
- Creating an EIP-712 signer
- Signing and sending transactions to a Sovereign SDK rollup

Users see a human-readable signing prompt in MetaMask instead of raw hex data, making it clear what they're approving.

## What is EIP-712?

[EIP-712](https://eips.ethereum.org/EIPS/eip-712) is an Ethereum standard for typed structured data signing. Instead of signing opaque bytes, users see structured, human-readable data in their wallet.

**Without EIP-712:**
```
Sign this message?
0x7b2262616e6b223a7b22637265617465...
```

**With EIP-712:**
```
Sign this message?

bank.create_token:
  token_name: "My Token"
  token_decimals: 8
  initial_balance: 1000000000
  ...
```

### Why use EIP-712 with Sovereign SDK?

1. **User Trust**: Users can verify exactly what transaction they're signing
2. **Standard Wallets**: Works with MetaMask, Rabby, and other Ethereum wallets
3. **No Custom Wallet**: Users don't need to install a Sovereign-specific wallet

## Prerequisites

1. **Node.js** (v18+) and npm/pnpm
2. **MetaMask** browser extension installed
3. **Running Sovereign SDK rollup** (default: `http://localhost:12346`)

## Quick Start

1. **Install dependencies:**
   ```bash
   npm install
   ```

2. **Configure environment** (optional):
   ```bash
   cp .env.example .env
   # Edit .env to set VITE_ROLLUP_URL if not using localhost
   ```

3. **Start the rollup** (in repo root):
   ```bash
   cargo run
   ```

4. **Run the example:**
   ```bash
   npm run dev
   ```

5. Open `http://localhost:5173`, connect MetaMask, and send a transaction.

## How It Works

### 1. Add Network to MetaMask

Users can add the rollup as a custom network using the `wallet_addEthereumChain` method:

```typescript
await window.ethereum.request({
  method: "wallet_addEthereumChain",
  params: [
    {
      chainId: "0x1a0d", // 6669 in hex
      chainName: "Sovereign Rollup",
      rpcUrls: ["http://localhost:12346"],
      nativeCurrency: {
        name: "SOV",
        symbol: "SOV",
        decimals: 18,
      },
    },
  ],
});
```

This prompts MetaMask to add the network, making it easy for users to switch to your rollup.

### 2. Connect to MetaMask

Use the standard `eth_requestAccounts` method to connect:

```typescript
const accounts = await window.ethereum.request({
  method: "eth_requestAccounts",
});
const account = accounts[0];
```

### 3. Create the Rollup Client

Initialize the Sovereign SDK client:

```typescript
import { createStandardRollup } from "@sovereign-sdk/web3";

const rollup = await createStandardRollup({
  url: "http://localhost:12346",
});
```

### 4. Get the Schema and Create EIP-712 Signer

The rollup exposes a schema that defines all transaction types. The `Eip712Signer` uses this schema to generate proper EIP-712 typed data:

```typescript
import { Eip712Signer } from "@sovereign-sdk/signers/wasm";

// Get the schema from the rollup
const serializer = await rollup.serializer();

// Create the signer with MetaMask provider, schema, and account address
const signer = new Eip712Signer(
  window.ethereum,
  serializer.schema,
  account
);
```

### 5. Sign and Send Transaction

Pass the signer to the rollup client's `call` method:

```typescript
const transaction = {
  bank: {
    create_token: {
      token_name: "My Token",
      token_decimals: 8,
      initial_balance: 1000000000,
      mint_to_address: account,
      admins: [],
      supply_cap: 100000000000,
    },
  },
};

const result = await rollup.call(transaction, { signer }, { path: "/sequencer/eip712_tx" });
console.log("Transaction hash:", result.response?.id);
```

When `call()` is invoked:
1. The SDK serializes the transaction
2. `Eip712Signer` generates EIP-712 typed data from the schema
3. MetaMask prompts the user to sign
4. The signed transaction is submitted to the rollup

## Integration Guide

To add EIP-712 MetaMask support to your own Sovereign SDK application:

### Step 1: Install Dependencies

```bash
npm install @sovereign-sdk/web3 @sovereign-sdk/signers
npm install -D @metamask/providers  # Optional: TypeScript types for window.ethereum
```

### Step 2: Configure Vite for WASM

The EIP-712 signer uses WebAssembly internally. Add these plugins to `vite.config.ts`:

```typescript
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
});
```

Install the plugins:
```bash
npm install -D vite-plugin-wasm vite-plugin-top-level-await
```

### Step 3: Update package.json Scripts

Add the WASM flag to your dev script:
```json
{
  "scripts": {
    "dev": "NODE_OPTIONS=--experimental-wasm-modules vite dev"
  }
}
```

### Step 4: Implement the Integration

```typescript
import { createStandardRollup } from "@sovereign-sdk/web3";
import { Eip712Signer } from "@sovereign-sdk/signers/wasm";

async function sendTransaction(tx: RuntimeCall, account: string) {
  // 1. Create rollup client
  const rollup = await createStandardRollup({
    url: process.env.ROLLUP_URL,
  });

  // 2. Get schema and create signer
  const serializer = await rollup.serializer();
  const signer = new Eip712Signer(
    window.ethereum,
    serializer.schema,
    account
  );

  // 3. Send transaction (EIP-712 requires dedicated endpoint)
  const result = await rollup.call(tx, { signer }, { path: "/sequencer/eip712_tx" });
  return result.response;
}
```

### Step 5: Generate TypeScript Types

Generate TypeScript types for your rollup's transactions. This provides type safety when constructing transactions:

```bash
npm install -D quicktype
npx quicktype -s schema path/to/json-schema.json -o src/types.ts --top-level RuntimeCall
```

Then use the generated types in your code:

```typescript
import type { RuntimeCall } from "./types";

const tx: RuntimeCall = {
  bank: {
    create_token: { /* TypeScript will validate this structure */ }
  }
};
```

## WASM Requirements

The `Eip712Signer` uses WebAssembly internally via `@sovereign-sdk/universal-wallet-wasm`. This WASM module handles:

- Parsing the rollup's transaction schema
- Generating EIP-712 typed data structures
- Computing the signing hash for public key recovery

### Why WASM?

The schema parsing and EIP-712 generation logic is shared between Rust (rollup) and TypeScript (frontend) to ensure consistency. WASM allows the same Rust code to run in the browser.

### Vite Configuration

```typescript
// vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
});
```

### Node.js Compatibility

When running the dev server:
```json
"dev": "NODE_OPTIONS=--experimental-wasm-modules vite dev"
```

## Key Files

| File | Purpose |
|------|---------|
| `src/App.tsx` | Main component with connect, sign, and send logic |
| `src/ConnectButton.tsx` | MetaMask connection button |
| `src/types.ts` | Generated TypeScript types for RuntimeCall |
| `vite.config.ts` | Vite config with WASM plugins |

## Troubleshooting

### MetaMask not detected

**Symptom:** "Please install MetaMask!" alert

**Solution:** Ensure MetaMask extension is installed and enabled. Refresh the page after installing.

### Schema loading errors

**Symptom:** Error when creating `Eip712Signer`

**Possible causes:**
- Rollup not running at the configured URL
- Network connectivity issues

**Solution:** Verify the rollup is running and accessible:
```bash
curl http://localhost:12346/health
```

### WASM loading errors

**Symptom:** `WebAssembly` or import errors in console

**Possible causes:**
- Missing WASM Vite plugins
- Missing `--experimental-wasm-modules` flag

**Solution:** Ensure `vite.config.ts` includes WASM plugins and the dev script uses the experimental flag.

### Transaction signing rejected

**Symptom:** Transaction fails after MetaMask popup

**Possible causes:**
- User rejected the signature request
- MetaMask is locked

**Solution:** Unlock MetaMask and click "Sign" when prompted.

### Invalid transaction format

**Symptom:** "Invalid JSON format" or schema validation errors

**Solution:** Ensure your transaction matches the rollup's expected format. Use the generated types from `types.ts` for type safety.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `VITE_ROLLUP_URL` | `http://localhost:12346` | Sovereign SDK rollup RPC URL |
| `VITE_CHAIN_ID` | `6669` | Chain ID for the rollup network |
| `VITE_CHAIN_NAME` | `Sovereign Rollup` | Display name in MetaMask |

## Learn More

- [EIP-712 Specification](https://eips.ethereum.org/EIPS/eip-712)
- [Sovereign SDK Documentation](https://github.com/Sovereign-Labs/sovereign-sdk)
- [MetaMask Developer Docs](https://docs.metamask.io/)
