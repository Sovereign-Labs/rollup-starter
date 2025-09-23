import fs from 'fs';
import {AdminClass, RuntimeCall} from "../types";
import {
    ANVIL_ADDRESS_1, defaultGas,
    deployerAddress,
    deployerPrivateKey,
    ETHTEST_DEFAULT_GAS,
    ETHTEST_DOMAIN,
    maxU128, minterPrivateKey
} from "./consts";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {createStandardRollup} from "@sovereign-sdk/web3";
import {readWarpRouteConfig, testDataFile, zeroPad20To32} from "./utils";

function buildCreateWarpRouteCall(domain: number, tokenId: string): RuntimeCall {
    // Pad it with zeros, as rollup expects.
    const expectedTokenId = zeroPad20To32(tokenId);
    return {
        warp: {
            register: {
                // The deployer can modify the warp route
                admin: {InsecureOwner: deployerAddress} as AdminClass,
                ism: {
                    MessageIdMultisig: {
                        threshold: 1,
                        // Should match what is used in ../../../../integrations/hyperlane/docker-compose.hyp-evm.yml for ethtest validator
                        validators: [ANVIL_ADDRESS_1],
                    },
                },
                token_source: {
                    Synthetic: {
                        remote_token_id: expectedTokenId,
                        local_decimals: 18,
                        remote_decimals: 18,
                    },
                },
                remote_routers: [
                    [
                        domain,
                        expectedTokenId,
                    ],
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
    }
}

function parseWarpRouteResponse(response: any): { routeId: string; tokenId: string } {
    // 1. Check receipt status
    const receipt = response?.response?.receipt;
    if (!receipt) {
        console.error("[✗] Transaction failed: No receipt found!");
        process.exit(1);
    }

    if (receipt.result !== "successful") {
        console.error("[✗] Transaction ${response.id} failed!");
        console.error("Receipt:", receipt);
        process.exit(1);
    }
    console.log(`[✓] Transaction successful: ${response.id}`);
    // @ts-ignore
    console.log("  Gas used:", receipt.data?.gas_used || "unknown");

    // 2. Find and print token_id from the Bank/TokenCreated event
    const events = response?.response?.events || [];
    const tokenCreatedEvent = events.find(
        (e: any) => e?.key === "Bank/TokenCreated"
    );

    let tokenId: string | undefined;
    if (tokenCreatedEvent) {
        // @ts-ignore
        tokenId = tokenCreatedEvent?.value?.token_created?.coins?.token_id;
        if (tokenId) {
            console.log("[✓] Token created");
            console.log("  Token ID:", tokenId);
        } else {
            console.error("[✗] Bank/TokenCreated event found but token_id is missing!");
            process.exit(1);
        }
    } else {
        console.error("[✗] Bank/TokenCreated event not found!");
        process.exit(1);
    }

    // 3. Find and print route_id from the Warp/RouteRegistered event
    const routeRegisteredEvent = events.find(
        (e: any) => e?.key === "Warp/RouteRegistered"
    );

    let routeId: string | undefined;
    if (routeRegisteredEvent) {
        // @ts-ignore
        routeId = routeRegisteredEvent?.value?.route_registered?.route_id;
        if (routeId) {
            console.log("[✓] Warp route registered");
            console.log("  Route ID:", routeId);
        } else {
            console.error("[✗] Warp/RouteRegistered event found but route_id is missing!");
            process.exit(1);
        }
    } else {
        console.error("[✗] Warp/RouteRegistered event not found!");
        process.exit(1);
    }

    // Write route ID to test data file
    try {
        const testData = {
            warp_route_id: routeId,
            warp_token_id: tokenId,
        };

        fs.writeFileSync(testDataFile, JSON.stringify(testData, null, 2));
        console.log(`[✓] Wrote route ID to ${testDataFile}`);
    } catch (error) {
        console.error(`[✗] Failed to write test data file: ${error}`);
        process.exit(1);
    }

    return {routeId, tokenId};
}

const setRelayerConfig: RuntimeCall = {
    interchain_gas_paymaster: {
        set_relayer_config: {
            beneficiary: deployerAddress,
            default_gas: defaultGas,
            domain_default_gas: [
                {
                    default_gas: ETHTEST_DEFAULT_GAS,
                    domain: ETHTEST_DOMAIN
                },
            ],
            domain_oracle_data: [
                {
                    // TODO: Dummy values now, need to figure out how to set them up
                    data_value: {
                        gas_price: 1,
                        token_exchange_rate: 1
                    },
                    domain: ETHTEST_DOMAIN
                }
            ]
        }
    }
}

const rollup = await createStandardRollup({
    url: "http://127.0.0.1:12346",
});
console.log("Rollup client initialized");

const ethtestTokenId = readWarpRouteConfig();
const createWarpRoute = buildCreateWarpRouteCall(ETHTEST_DOMAIN, ethtestTokenId);

let deployerSigner = new Secp256k1Signer(deployerPrivateKey);

const warpRegisterResponse = await rollup.call(createWarpRoute, {signer: deployerSigner});
console.log("Create warp router response:");

const {routeId, tokenId} = parseWarpRouteResponse(warpRegisterResponse);
console.log("\nSummary:");
console.log(`  Route ID: ${routeId}`);
console.log(`  Token ID: ${tokenId}`);


const minterSigner = new Secp256k1Signer(minterPrivateKey);

const response = await rollup.call(setRelayerConfig, {signer: minterSigner});
console.log("Relayer config response");
console.log(JSON.stringify(response.response));
