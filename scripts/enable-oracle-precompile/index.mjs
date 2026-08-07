#!/usr/bin/env node
// Enable (or disable) the price-oracle custom precompile on a running rollup
// by sending the admin-only `evm.update_enabled_custom_precompiles` call message.
//
// Usage:
//   npm install
//   ADMIN_PRIVATE_KEY=0x... npm start            # enable
//   ADMIN_PRIVATE_KEY=0x... npm start -- --disable
//
// Environment:
//   ADMIN_PRIVATE_KEY  hex secp256k1 private key of the EVM admin (genesis `evm.admin`)
//   ROLLUP_URL         rollup HTTP endpoint (default http://127.0.0.1:12346)
//
// Requires an SDK build that includes the UpdateEnabledCustomPrecompiles call
// message (sovereign-sdk PR #2898/#2996). Against an older runtime the schema
// fetched from the node won't contain the variant and serialization will fail.

import { keccak_256 } from "@noble/hashes/sha3";
import { getPublicKey } from "@noble/secp256k1";
import { Secp256k1Signer } from "@sovereign-sdk/signers";
import { hexToBytes } from "@sovereign-sdk/utils";
import { createStandardRollup } from "@sovereign-sdk/web3";

// crates/precompiles/price-oracle/src/precompile.rs
const PRICE_ORACLE_PRECOMPILE_ADDRESS =
  "0x0000000000000000000000000000000000010002";
// configs/celestia/genesis.json -> evm.admin
const EXPECTED_ADMIN = "0x8a7a1774229cdcde36b1e4a2321e702f25788698";

const rollupUrl = process.env.ROLLUP_URL ?? "http://127.0.0.1:12346";
const disable = process.argv.includes("--disable");

const toHex = (bytes) =>
  "0x" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

// Probes the precompile through eth_call with empty calldata.
// Disabled: the address is an empty account, so the call succeeds returning 0x.
// Enabled: decode_feed_request rejects the malformed input, so the call errors.
async function probePrecompile() {
  const res = await fetch(`${rollupUrl}/rpc`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_call",
      params: [{ to: PRICE_ORACLE_PRECOMPILE_ADDRESS, data: "0x" }, "latest"],
    }),
  });
  if (!res.ok) {
    throw new Error(`eth_call probe failed: HTTP ${res.status}`);
  }
  const body = await res.json();
  if (body.error) {
    return { enabled: true, detail: body.error.message };
  }
  return { enabled: body.result !== "0x", detail: body.result };
}

async function main() {
    console.log(await probePrecompile());

  process.exit(0);

  const keyHex = process.env.ADMIN_PRIVATE_KEY;
  if (!keyHex) {
    console.error("ADMIN_PRIVATE_KEY is not set");
    process.exit(1);
  }
  const privateKey = hexToBytes(keyHex);

  const uncompressedPubKey = getPublicKey(privateKey, false);
  const senderAddress = toHex(keccak_256(uncompressedPubKey.slice(1)).slice(-20));
  console.log(`rollup:  ${rollupUrl}`);
  console.log(`sender:  ${senderAddress}`);
  console.log(`action:  ${disable ? "disable" : "enable"} ${PRICE_ORACLE_PRECOMPILE_ADDRESS}`);
  if (senderAddress !== EXPECTED_ADMIN) {
    console.warn(
      `warning: sender does not match the genesis EVM admin ${EXPECTED_ADMIN}; the call will be rejected unless the admin has been rotated`,
    );
  }

  try {
    const before = await probePrecompile();
    console.log(`precompile currently ${before.enabled ? "ENABLED" : "DISABLED"} (${before.detail})`);
  } catch (e) {
    console.warn(`could not probe precompile state: ${e.message}`);
  }

  const rollup = await createStandardRollup({ url: rollupUrl });
  const signer = new Secp256k1Signer(privateKey);

  // The variant tag is CamelCase, unlike every other module's call message: the
  // universal-wallet macro drops `#[serde(rename_all = "snake_case")]` when the
  // enum carries a second `#[serde(...)]` attribute (sov-evm's `#[serde(bound)]`),
  // so the schema keeps the Rust variant names. If the SDK ever fixes that
  // (universal-wallet/macro-helpers foreign_attributes.rs), this becomes
  // `update_enabled_custom_precompiles`.
  const runtimeCall = {
    evm: {
      UpdateEnabledCustomPrecompiles: disable
        ? { add: [], remove: [PRICE_ORACLE_PRECOMPILE_ADDRESS] }
        : { add: [PRICE_ORACLE_PRECOMPILE_ADDRESS], remove: [] },
    },
  };

  // Uses generation-based uniqueness by default. For nonce-based dedup use:
  //   const { nonce } = await rollup.dedup(await signer.publicKey());
  //   await rollup.call(runtimeCall, { signer, overrides: { uniqueness: { nonce } } });
  const { response } = await rollup.call(runtimeCall, { signer });
  console.log(`submitted: ${JSON.stringify(response)}`);

  const want = !disable;
  for (let i = 0; i < 15; i++) {
    await new Promise((r) => setTimeout(r, 2000));
    try {
      const now = await probePrecompile();
      if (now.enabled === want) {
        console.log(`precompile is now ${want ? "ENABLED" : "DISABLED"}`);
        return;
      }
    } catch {
      // node briefly unreachable; keep polling
    }
  }
  console.error(
    "timed out waiting for the precompile state to flip; check the transaction result in the ledger (/ledger/txs)",
  );
  process.exit(1);
}

await main();
