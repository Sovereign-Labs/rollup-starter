/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {RuntimeCall} from "../types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {minterPrivateKey, ETHTEST_DOMAIN, deployerAddress} from "./consts";
import {readWarpRouteIdOnRollup, zeroPad20To32} from "./utils";

const OUTBOUND_ADDRESS: string = zeroPad20To32(deployerAddress);
const ROLLUP_WARP_ROUTE_ID: string = readWarpRouteIdOnRollup();

const transferRemote: RuntimeCall = {
    warp: {
        transfer_remote: {
            amount: 1233400000000000,
            destination_domain: ETHTEST_DOMAIN,
            gas_payment_limit: 20_000,
            recipient: OUTBOUND_ADDRESS,
            warp_route: ROLLUP_WARP_ROUTE_ID,
            relayer: "0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747"
        }
    }
};

console.log("Runtime call:", transferRemote);

let signer = new Secp256k1Signer(minterPrivateKey);
console.log("Signer is done:", signer);
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