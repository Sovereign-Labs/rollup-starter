import {createStandardRollup} from "@sovereign-sdk/web3";
import {RuntimeCall, AdminEnum} from "./types";
import {Ed25519Signer, Secp256k1Signer} from "@sovereign-sdk/signers";
import {maxU128} from "./hyperlane_consts";


console.log("Starting....");
export const createWarpRoute: RuntimeCall = {
    warp: {
        Register: {
            // The warp route cannot be modified
            admin: AdminEnum.None,
            ism: {
                MessageIdMultisig: {
                    threshold: 1,
                    // The validators address, always ethereum style. Anvil account 0
                    validators: ["0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"],
                },
            },
            token_source: {
                Synthetic: {
                    remote_token_id: "0x00000000000000000000000059b670e9fA9D0A427751Af201D676719a970857b",
                    local_decimals: 18,
                    remote_decimals: 18,
                },
            },
            remote_routers: [
                [
                    31337,
                    // What is this??
                    "0x00000000000000000000000059b670e9fA9D0A427751Af201D676719a970857b",
                ],
            ],
            inbound_transferrable_tokens_limit: maxU128,
            inbound_limit_replenishment_per_slot: maxU128,
            outbound_transferrable_tokens_limit: maxU128,
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
    console.log("")
    const response = await rollup.call(createWarpRoute, { signer });

    console.log(JSON.stringify(response.response));
} catch (e) {
    console.error("failed to call rollup:", e);
}