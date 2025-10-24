import { createStandardRollup } from "@sovereign-sdk/web3";
import { Secp256k1Signer } from "@sovereign-sdk/signers";
import { computeAddress } from "ethers";
import { RuntimeCall } from "./types";

// Instantiate the rollup client
console.log("Initializing rollup client...");
// defaults to http://localhost:12346, or pass url: "custom-endpoint"
const rollup = await createStandardRollup({url: "http://127.0.0.1:12346"});
console.log("Rollup client initialized.");

console.log("Initializing signer...");
// Private key taken from test-data/keys/minter_private_key.json
// We use this private key because we happen to know that it's pre-configured with tokens in the default
// genesis.json. We could generate a new key instead, but then we'd have to modify the genesis config to
// get tokens for it.
const privKey =
  "0d87c12ea7c12024b3f70a26d735874608f17c8bce2b48e6fe87389310191264";
let signer = new Secp256k1Signer(privKey);
console.log("Signer initialized.");

// Get the Ethereum address from the private key
const signerAddress = computeAddress(`0x${privKey}`).toLowerCase();
console.log("Signer address:", signerAddress);
// Should be 0x9b08ce57a93751aE790698A2C9ebc76A78F23E25

// Declare a call message (to send as a transaction)
let callMessage: RuntimeCall = {
  bank: {
    create_token: {
      admins: [],
      token_decimals: 8,
      supply_cap: 100000000000,
      token_name: "Example Token",
      initial_balance: 1000000000,
      mint_to_address: signerAddress,
    },
  },
};

/* 
// SUBSCRIPTION BLOCK -- REMOVE COMMENT BLOCK TO SUBSCRIBE TO EVENTS USING A WEBSOCKET
const wait = (seconds: number) => 
  new Promise((resolve) => setTimeout(resolve, seconds * 1000));
// Subscribe to events from the rollup
console.log("Subscribing to rollup events...");
async function handleNewEvent(event: any): Promise<void> {
  console.log(event);
}
const subscription = rollup.subscribe("events", handleNewEvent); // Subscribe to events from the rollup
console.log("Subscribed.");
*/

// Send the transaction
console.log("Sending create token transaction...");
let tx_response = await rollup.call(callMessage, { signer }); // Send our transaction
console.log("Tx sent successfully. Response:");
console.dir(tx_response.response, { depth: null, colors: true });

/*
// SUBSCRIPTION BLOCK -- REMOVE COMMENT BLOCK TO SUBSCRIBE TO EVENTS USING A WEBSOCKET
await wait(2); // Wait for a couple seconds to get the events back before exit
// Unsubscribe if needed
console.log(
  "Create token event should have been above. Unsubscribing to rollup events and exiting example script.",
);
subscription.unsubscribe();
*/
