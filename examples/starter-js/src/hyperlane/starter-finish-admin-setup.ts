// 1000000000000000000 = 1 ETH


/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {CallMessage7Enum, RuntimeCall} from "../types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {sequencerPrivateKey} from "./consts";
import { TurnkeySigner } from "./turnkey-signer";
import dotenv from 'dotenv'; 
dotenv.config(); 


const depositSequencer: RuntimeCall = {
    sequencer_registry: {
        deposit: {
			amount:   100000000000000000, // .1 ETH
			da_address: "0000000000000000000000000000000000000000000000000000000000000000", // DEPLOYMENT: Replace with DA address of the sequencer 
        }
    }
};

const terminateSetupMode: RuntimeCall = {
    chain_state: CallMessage7Enum.TerminateSetupMode
};

console.log("Runtime call:", depositSequencer);


let signer = await TurnkeySigner.create({
    organizationId: process.env.TURNKEY_ORGANIZATION_ID!,
    apiPublicKey: process.env.TURNKEY_API_PUBLIC_KEY!,
    apiPrivateKey: process.env.TURNKEY_API_PRIVATE_KEY!,
    keyId: process.env.TURNKEY_KEY_ID!,
});
const rollup = await createStandardRollup({
    url: "http://127.0.0.1:12346",
});
console.log("Rollup client initialized");

try {
    console.log("Depositing sequencer funds...");
    const response = await rollup.call(depositSequencer, {signer, overrides: {
        details: {
            max_fee: 0,
        },
    }});
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
    
    // Find and display specific events
    const events = response.response.events;
    
    // Find Mailbox/DispatchId event
    // @ts-ignore
    const dispatchIdEvent = events.find((e: any) => e.key === "SequencerRegistry/Deposited");
    if (dispatchIdEvent) {
        // @ts-ignore
        const sequencer = dispatchIdEvent.value.deposited.sequencer;
        console.log(`[✓] SequencerRegistry/Deposited: ${sequencer}`);
    }

    console.log("Terminating setup mode...");
    const terminateSetupModeResponse = await rollup.call(terminateSetupMode, {signer});
    console.log("Full response:");
    console.log(JSON.stringify(terminateSetupModeResponse.response));
    console.log("\n-------");
    // Check receipt result first
    const terminateSetupModeReceipt = terminateSetupModeResponse.response.receipt;
    // @ts-ignore
    if (terminateSetupModeReceipt.result !== "successful") {
        // @ts-ignore
        console.log("[✗] Receipt result:", terminateSetupModeReceipt.result);
        process.exit(1);
    }
    
    console.log("[✓] Receipt result: successful");
   
} catch (e) {
    console.error("failed to call rollup:", e);
}
