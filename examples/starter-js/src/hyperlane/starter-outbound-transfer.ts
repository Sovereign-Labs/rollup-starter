/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {RuntimeCall} from "../types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {ETHTEST_DOMAIN, deployerAddress, minterAddress, deployerPrivateKey} from "./consts";
import {readWarpRouteIdOnRollup, zeroPad20To32} from "./utils";

const OUTBOUND_ADDRESS: string = zeroPad20To32(deployerAddress);
const ROLLUP_WARP_ROUTE_ID: string = readWarpRouteIdOnRollup();

const transferRemote: RuntimeCall = {
    warp: {
        transfer_remote: {
            amount: 123340000000000,
            destination_domain: ETHTEST_DOMAIN,
            gas_payment_limit: 20_000,
            recipient: OUTBOUND_ADDRESS,
            warp_route: ROLLUP_WARP_ROUTE_ID,
            relayer: minterAddress,
        }
    }
};

console.log("Runtime call:", transferRemote);

let signer = new Secp256k1Signer(deployerPrivateKey);
const rollup = await createStandardRollup({
    url: "http://127.0.0.1:12346",
});
console.log("Rollup client initialized");

try {
    const response = await rollup.call(transferRemote, {signer});
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
    const dispatchIdEvent = events.find((e: any) => e.key === "Mailbox/DispatchId");
    if (dispatchIdEvent) {
        // @ts-ignore
        const id = dispatchIdEvent.value.dispatch_id.id;
        console.log(`[✓] Mailbox/DispatchId (HyperlaneId): ${id}`);
    }
    
    // Find Warp/TokenTransferredRemote event
    // @ts-ignore
    const tokenTransferEvent = events.find((e: any) => e.key === "Warp/TokenTransferredRemote");
    if (tokenTransferEvent) {
        const transferred = tokenTransferEvent.value.token_transferred_remote;
        console.log("[✓] Warp/TokenTransferredRemote:");
        // @ts-ignore
        console.log(`    Route ID: ${transferred.route_id}`);
        // @ts-ignore
        console.log(`    To Domain: ${transferred.to_domain}`);
        // @ts-ignore
        console.log(`    Recipient: ${transferred.recipient}`);
        
        // Convert hex amount to decimal
        // @ts-ignore
        const hexAmount = transferred.amount;
        const decimalAmount = BigInt(hexAmount).toString();
        console.log(`    Amount (hex): ${hexAmount}`);
        console.log(`    Amount (decimal): ${decimalAmount}`);
        console.log("Completed");
    }
} catch (e) {
    console.error("failed to call rollup:", e);
}