/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {RuntimeCall} from "./types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import {
    deployerPrivateKey,
    defaultGas,
    ETHTEST_DOMAIN, ETHTEST_DEFAULT_GAS
} from "./hyperlane-consts";

const setRelayerConfig: RuntimeCall = {
    interchain_gas_paymaster: {
        set_relayer_config: {
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

console.log("Runtime call:", setRelayerConfig);

let signer = new Secp256k1Signer(deployerPrivateKey);
console.log("Signer is done:", signer);
const rollup = await createStandardRollup({
    url: "http://127.0.0.1:12346",
});
console.log("Rollup client initialized");

try {
    const response = await rollup.call(setRelayerConfig, {signer});
    console.log("Full response");
    console.log(JSON.stringify(response.response));
    console.log("-------");
} catch (e) {
    console.error("failed to call rollup:", e);
}