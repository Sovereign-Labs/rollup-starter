/// Updating WARP route wit
import {createStandardRollup} from "@sovereign-sdk/web3";
import {RuntimeCall} from "./types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {ETHTEST_DOMAIN, deployerPrivateKey} from "./hyperlane-consts";


// Can it be pre-computed?
// Not, it should be zero padded to be 32 bytes, instead of 20, so prepend 12 "00"
const ETHTEST_TOKEN_ROUTER_ID: string = "0x00000000000000000000000059b670e9fA9D0A427751Af201D676719a970857b";
// address

// This is taken from the `starter-warp-deploy.ts` output (route_id)
const ROLLUP_WARP_ROUTE_ID: string = "0x1383103db8d7d56968f9b1c69a7cd1379ef0f2df41ed3a728489ae26a7cdf151";

export const enrollRemoteRouter: RuntimeCall = {
    warp: {
        enroll_remote_router: {
            remote_domain: ETHTEST_DOMAIN,
            remote_router_address: ETHTEST_TOKEN_ROUTER_ID,
            warp_route: ROLLUP_WARP_ROUTE_ID,
        }
    }
};

console.log("Runtime call:", enrollRemoteRouter);

let signer = new Secp256k1Signer(deployerPrivateKey);
console.log("Signer is done:", signer);
const rollup = await createStandardRollup({
    url: "http://127.0.0.1:12346",
});
console.log("Rollup client initialized");

try {
    const response = await rollup.call(enrollRemoteRouter, {signer});
    console.log("Full response");
    console.log(JSON.stringify(response.response));
    console.log("-------");
} catch (e) {
    console.error("failed to call rollup:", e);
}