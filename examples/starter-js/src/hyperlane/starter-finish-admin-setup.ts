// 1000000000000000000 = 1 ETH


/// Initial register of WARP route.
import {createStandardRollup} from "@sovereign-sdk/web3";
import {CallMessage7Enum, PayeePolicyEnum, RuntimeCall} from "../types";
import {Secp256k1Signer} from "@sovereign-sdk/signers";
import { TurnkeySigner } from "./turnkey-signer";
import dotenv from 'dotenv'; 
dotenv.config(); 


const evmAdminCall: RuntimeCall = {
    // @ts-ignore
    evm: {
        UpdateRuntimeConfig: {
            new_contract_creation_policy: {
                Allowlist: {
                    add: [
                        // "0xeE1Bdc7095BD0bE36De7b33b9a32D5aFE86Ff36a",
                        // "0xB5f248f687C6A969E6AC9CeA1e3D65381b1F5d19",
                        // "0x748F61A525B5D796d12B6E4D92b37b6c68A0A50c",
                        // "0x6BC717Cf27F85df0446e76Bc65eE602E001e0eb0",
                        // "0x3ecc9b73705b3BB0486Ad24CFBD499ED0A8dBccA",
                        // "0x806991eFd4A67D0178387D0fAf04945422FbBc2e",
                        // "0xA60Ea19dac2ea8d9830eA2cE9d6F4AfB487F4Cc8",
                        // "0x6f7294a553c4e9474f01B1be7cF72ab1938F8A0e",
                        // "0x9f9365A88b571593dB1e0CA05cEEFd2F3E020f8e",
                        // "0x6c942525544eAaE2e3361704B2f596122b718D8E",
                        // "0xf8ffaA5c3812659D832Ea866526F76A3B7F3b298"
                      ],
                    remove: [],
                },
            },
            chain_spec_update: null,
            new_admin: null,
            new_hardfork: null,
        },
    },
};


console.log("Runtime call:", evmAdminCall);



let signer = await TurnkeySigner.create({
    organizationId: process.env.TURNKEY_ORGANIZATION_ID!,
    apiPublicKey: process.env.TURNKEY_API_PUBLIC_KEY!,
    apiPrivateKey: process.env.TURNKEY_API_PRIVATE_KEY!,
    keyId: process.env.TURNKEY_KEY_ID!,
});
const rollup = await createStandardRollup({
    url: "https://rpc.chain.relay.link",
});
console.log("Rollup client initialized");

try {
    console.log("Running EVM admin call...");
    const response = await rollup.call(evmAdminCall, {signer});
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
   
} catch (e) {
    console.error("failed to call rollup:", e);
}
