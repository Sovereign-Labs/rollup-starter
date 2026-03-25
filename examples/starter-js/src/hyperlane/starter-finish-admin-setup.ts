// 1000000000000000000 = 1 ETH


/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {CallMessage7Enum, PayeePolicyEnum, RuntimeCall} from "../types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {sequencerPrivateKey} from "./consts";
import { TurnkeySigner } from "./turnkey-signer";
import dotenv from 'dotenv'; 
dotenv.config(); 


const paymasterUpdateCall: RuntimeCall = {
    paymaster: {
        update_policy: {
			payer:   '0x8a7a1774229CDcdE36b1E4A2321e702F25788698', 
            update: {
                default_policy:  null,
                sequencer_update: null,
                updaters_to_add: null,
                updaters_to_remove: null,
                payee_policies_to_delete: null,
                payee_policies_to_set: [ // Outer array reflects that the option is some
                    ["0xF61A305199fa1135d76FFaB3752D42F55cBd775A", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x28c50f9329d14b1ee7a3a38a2064e2348950d6b3", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x246A13358Fb27523642D86367a51C2aEB137Ac6C", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0xf3d63166F0Ca56C3c1A3508FcE03Ff0Cf3Fb691e", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x1D682340264cF209257f24C3EDcb2a9fc0592535", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x0a998d6ae81cee64e15b164910d3e03c5f06d2d0", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x1a3411431be3063d9ae3f52cd5782831eed9f1f9", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x2134c27b7748ee13e9e6ec943153237d85b57559", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x2be3df6c50fb1b11d8189c2d4de5c16f6712fac4", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x5143dceaf7ceeae6bf24e51c23b8e1eee28f4241", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x5ea8cd330cb5ece0dfa91c45b381ef49736eaa13", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x66ac5b03b68ec3af176c4ee8565c6fa0b19dbd4a", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x6d878dc11d1aee73f9c55b324c23dce05fe91a3d", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x8bde0941b569ab33aaa2ea9617dd992a8a1af805", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x9bee034fb873496ade3d654ac59c3b9c8513749e", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0xED1ce6bc5964ff4529A593Bf3ebab2Caf73c31dE", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0x8a7a1774229CDcdE36b1E4A2321e702F25788698", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                    ["0xeE1Bdc7095BD0bE36De7b33b9a32D5aFE86Ff36a", {allow: {max_fee: null, gas_limit: null, transaction_limit: null, max_gas_price: null}}],
                ]
            }
        }
    }
};


// const transferToSecureAddress: RuntimeCall = {
//     bank: {
//         transfer: {
//             coins: {
//                 amount: 80000000000000000, // .08 ETH. Leaves .01 ETH for paymaster
//                 token_id: "token_1g26n9g2wfhs9y4v2a8h2e73yx232m3jl95snr4ndth04ut8a2qfqd5rvh4",
//             },
//             to: "0x7C574cD8A13E8d8fAC75c80b22aD576B234C3974",
//         }
//     }
// };


// const terminateSetupMode: RuntimeCall = {
//     chain_state: CallMessage7Enum.TerminateSetupMode
// };

console.log("Runtime call:", paymasterUpdateCall);



let signer = await TurnkeySigner.create({
    organizationId: process.env.TURNKEY_ORGANIZATION_ID!,
    apiPublicKey: process.env.TURNKEY_API_PUBLIC_KEY!,
    apiPrivateKey: process.env.TURNKEY_API_PRIVATE_KEY!,
    keyId: process.env.TURNKEY_KEY_ID!,
});
const rollup = await createStandardRollup({
    url: "https://rpc.chain.relay.link",
});
// let signer = new Secp256k1Signer(sequencerPrivateKey);
console.log("Rollup client initialized");

try {
    console.log("Setting paymaster policy...");
    const response = await rollup.call(paymasterUpdateCall, {signer});
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
    
    // // Find and display specific events
    // const events = response.response.events;
    
    // // Find Mailbox/DispatchId event
    // // @ts-ignore
    // const dispatchIdEvent = events.find((e: any) => e.key === "SequencerRegistry/Deposited");
    // if (dispatchIdEvent) {
    //     // @ts-ignore
    //     const sequencer = dispatchIdEvent.value.deposited.sequencer;
    //     console.log(`[✓] SequencerRegistry/Deposited: ${sequencer}`);
    // }

    // const transferToSecureAddressResponse = await rollup.call(transferToSecureAddress, {signer, overrides: {
    //     details: {
    //         max_fee: 0,
    //     },
    // }});
    // console.log("Full response:");
    // console.log(JSON.stringify(response.response));
    // console.log("\n-------");
    // // Check receipt result first
    // const transferToSecureAddressReceipt = transferToSecureAddressResponse.response.receipt;
    // // @ts-ignore
    // if (transferToSecureAddressReceipt.result !== "successful") {
    //     // @ts-ignore
    //     console.log("[✗] Receipt result:", transferToSecureAddressReceipt.result);
    //     process.exit(1);
    // }
    
    // console.log("[✓] Receipt result: successful");
    
    // // Find and display specific events
    // const transferToSecureAddressEvents = transferToSecureAddressResponse.response.events;
    
    // // Find Bank/Transfer event
    // // @ts-ignore
    // const transferToSecureAddressEvent = transferToSecureAddressEvents.find((e: any) => e.key.includes("Bank"));
    // if (transferToSecureAddressEvent) {
    //     // @ts-ignore
    //     const transferToSecureAddress = transferToSecureAddressEvent.value.token_transferred.to;
    //     console.log(`[✓] Bank/Transfer: ${transferToSecureAddress}`);
    // }

    // console.log("Terminating setup mode...");
    // const terminateSetupModeResponse = await rollup.call(terminateSetupMode, {signer});
    // console.log("Full response:");
    // console.log(JSON.stringify(terminateSetupModeResponse.response));
    // console.log("\n-------");
    // // Check receipt result first
    // const terminateSetupModeReceipt = terminateSetupModeResponse.response.receipt;
    // // @ts-ignore
    // if (terminateSetupModeReceipt.result !== "successful") {
    //     // @ts-ignore
    //     console.log("[✗] Receipt result:", terminateSetupModeReceipt.result);
    //     process.exit(1);
    // }
    
    // console.log("[✓] Receipt result: successful");
   
} catch (e) {
    console.error("failed to call rollup:", e);
}
