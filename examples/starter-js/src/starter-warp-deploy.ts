/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {RuntimeCall, AdminClass} from "./types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {maxU128, ANVIL_ADDRESS_0, deployerAddress, deployerPrivateKey} from "./hyperlane-consts";

// Can it be pre-computed?
const ETHTEST_TOKEN_ROUTER_ID: string = "0x00000000000000000000000059b670e9fA9D0A427751Af201D676719a970857b";


export const createWarpRoute: RuntimeCall = {
    warp: {
        register: {
            // The deployer can modify the warp route
            admin: {InsecureOwner: deployerAddress} as AdminClass,
            ism: {
                MessageIdMultisig: {
                    threshold: 1,
                    validators: [ANVIL_ADDRESS_0],
                },
            },
            token_source: {
                Synthetic: {
                    remote_token_id: ETHTEST_TOKEN_ROUTER_ID,
                    local_decimals: 18,
                    remote_decimals: 18,
                },
            },
            remote_routers: [
                // Will be enrolled separately
                // [
                //     ETHTEST_DOMAIN,
                //     ETHTEST_TOKEN_ID,
                // ],
            ],
            // @ts-ignore
            inbound_transferrable_tokens_limit: maxU128,
            // @ts-ignore
            inbound_limit_replenishment_per_slot: maxU128,
            // @ts-ignore
            outbound_transferrable_tokens_limit: maxU128,
            // @ts-ignore
            outbound_limit_replenishment_per_slot: maxU128,
        },
    },
};
console.log("Runtime call:", createWarpRoute);

let signer = new Secp256k1Signer(deployerPrivateKey);
console.log("Signer is done:", signer);
const rollup = await createStandardRollup({
    url: "http://127.0.0.1:12346",
});
console.log("Rollup client initialized");


try {
    const response = await rollup.call(createWarpRoute, {signer});
    console.log("Full response:");
    console.log(JSON.stringify(response.response));
    console.log("-------");

    // 1. Check receipt status
    const receipt = response?.response?.receipt;
    if (!receipt) {
        console.error("Transaction failed: No receipt found!");
        process.exit(1);
    }

    if (receipt.result !== "successful") {
        console.error("Transaction failed!");
        console.error("Receipt:", receipt);
        process.exit(1);
    }
    console.log("✓ Transaction successful");
    // @ts-ignore
    console.log("  Gas used:", receipt.data?.gas_used || "unknown");

    // 2. Find and print token_id from the Bank/TokenCreated event
    const events = response?.response?.events || [];
    const tokenCreatedEvent = events.find(
        (e: any) => e?.key === "Bank/TokenCreated"
    );
    if (tokenCreatedEvent) {
        // @ts-ignore
        const tokenId = tokenCreatedEvent?.value?.token_created?.coins?.token_id;
        if (tokenId) {
            console.log("✓ Token created");
            console.log("  Token ID:", tokenId);
        } else {
            console.error("✗ Bank/TokenCreated event found but token_id is missing!");
        }
    } else {
        console.error("✗ Bank/TokenCreated event not found!");
    }

    // 3. Find and print route_id from the Warp/RouteRegistered event
    const routeRegisteredEvent = events.find(
        (e: any) => e?.key === "Warp/RouteRegistered"
    );
    if (routeRegisteredEvent) {
        // @ts-ignore
        const routeId = routeRegisteredEvent?.value?.route_registered?.route_id;
        if (routeId) {
            console.log("✓ Warp route registered");
            console.log("  Route ID:", routeId);
        } else {
            console.error("✗ Warp/RouteRegistered event found but route_id is missing!");
        }
    } else {
        console.error("✗ Warp/RouteRegistered event not found!");
    }
} catch (e) {
    console.error("failed to call rollup:", e);
}