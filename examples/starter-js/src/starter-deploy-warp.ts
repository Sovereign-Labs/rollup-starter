import {createStandardRollup} from "@sovereign-sdk/web3";
import {RuntimeCall, AdminEnum} from "./types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {maxU128, ETHTEST_DOMAIN, ANVIL_ADDRESS_0} from "./hyperlane_consts";


// TODO: Update after warp deployed on
const ETHTEST_TOKEN_ID: string = "0x00000000000000000000000059b670e9fA9D0A427751Af201D676719a970857b";

export const createWarpRoute: RuntimeCall = {
    warp: {
        register: {
            // The warp route cannot be modified
            admin: AdminEnum.None,
            ism: {
                MessageIdMultisig: {
                    threshold: 1,
                    validators: [ANVIL_ADDRESS_0],
                },
            },
            token_source: {
                Synthetic: {
                    remote_token_id: ETHTEST_TOKEN_ID,
                    local_decimals: 18,
                    remote_decimals: 18,
                },
            },
            remote_routers: [
                [
                    ETHTEST_DOMAIN,
                    // What is this??
                    "0x00000000000000000000000059b670e9fA9D0A427751Af201D676719a970857b",
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
};
console.log("Runtime call:", createWarpRoute);

// tx_signer_private_key.json
const privKey =
    "0d87c12ea7c12024b3f70a26d735874608f17c8bce2b48e6fe87389310191264";
let signer = new Secp256k1Signer(privKey);
console.log("Signer is done:", signer);
const rollup = await createStandardRollup({
    url: "http://127.0.0.1:12346",
});
console.log("Rollup client initialized");

try {
    const response = await rollup.call(createWarpRoute, {signer});
    console.log("Full response");
    console.log(JSON.stringify(response.response));
    console.log("-------");
} catch (e) {
    console.error("failed to call rollup:", e);
}