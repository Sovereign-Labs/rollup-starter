// 1000000000000000000 = 1 ETH


/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {CallMessage7Enum, PayeePolicyEnum, RuntimeCall} from "../types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import { TurnkeySigner } from "./turnkey-signer";
import dotenv from 'dotenv'; 
dotenv.config(); 

const PRICE_ORACLE_PRECOMPILE_ADDRESS = "0x0000000000000000000000000000000000010002";
const evmAdminCall = {
  evm: {
    UpdateEnabledCustomPrecompiles: { add: [PRICE_ORACLE_PRECOMPILE_ADDRESS], remove: [] },
  },
};

console.log("Runtime call:", evmAdminCall);



let signer = await TurnkeySigner.create({
    organizationId: process.env.TURNKEY_ORGANIZATION_ID!,
    apiPublicKey: process.env.TURNKEY_API_PUBLIC_KEY!,
    apiPrivateKey: process.env.TURNKEY_API_PRIVATE_KEY!,
    keyId: process.env.TURNKEY_KEY_ID!,
});
const rollup = await createStandardRollup({
    url: "https://rpc.chain.relay.link",
});
console.log("Rollup client initialized");

try {
    console.log("Running EVM admin call...");
    const response = await rollup.call(evmAdminCall, {signer});
    console.log("Full response:");
    console.log(JSON.stringify(response.response));
    console.log("\n-------");
    // Check receipt result first
    const receipt = response.response.receipt;
    // @ts-ignore
    if (receipt.result !== "successful") {
        // @ts-ignore
        console.log("[✗] Receipt result:", receipt.result);
        process.exit(1);
    }
    
    console.log("[✓] Receipt result: successful");
   
} catch (e) {
    console.error("failed to call rollup:", e);
}
