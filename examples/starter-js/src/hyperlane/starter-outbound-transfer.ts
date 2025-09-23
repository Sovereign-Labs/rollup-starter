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
    console.log("Full response");
    console.log(JSON.stringify(response.response));
    console.log("-------");
} catch (e) {
    console.error("failed to call rollup:", e);
}