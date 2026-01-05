import fs from "fs";
import { AdminClass, RuntimeCall } from "../types";
import {
  defaultGas,
  ETHTEST_DEFAULT_GAS,
  maxU128,
  minterPrivateKey,
} from "./consts";
import { Secp256k1Signer } from "@sovereign-sdk/signers";
import { createStandardRollup } from "@sovereign-sdk/web3";
import { testDataFile, zeroPad20To32 } from "./utils";
function buildCreateWarpRouteCall(): RuntimeCall {
  // Pad it with zeros, as rollup expects.
  const expectedTokenId = zeroPad20To32(
    "0x066756283474e4F0846140C46866B8745cE09363",
  );
  return {
    warp: {
      register: {
        // The deployer can modify the warp route
        admin: {
          InsecureOwner: "0xA6edfca3AA985Dd3CC728BFFB700933a986aC085",
        } as AdminClass,
        ism: {
          MessageIdMultisig: {
            threshold: 2,
            validators: [
              // alias/validator-sepolia-1
              // AWS Account ID: 590183691025
              // Key ID: 37d718ff-72c2-4442-ad27-d01f824a23c2
              "0xf1F77d12A867D37cFb66412320a37557baF5cAAc",
              // alias/validator-sepolia-2
              // AWS Account ID: 455162986047
              // Key ID: a56bdd62-9f61-4687-addc-c8fdc56daeb7
              "0x94280E8DFB6AAAc8231b0038Beeff4c21e44544B",
              // alias/validator-sepolia-3
              // AWS Account ID: 189265240691
              // Key ID: b4de8ee9-0382-41b5-941b-94c89fa5d975
              "0x5A29B58cC4c5330BD953e5b7E4A95A7abf7b4ED9",
            ],
          },
        },
        token_source: {
          Synthetic: {
            remote_token_id: expectedTokenId,
            local_decimals: 18,
            remote_decimals: 18,
          },
        },
        remote_routers: [[11155111, expectedTokenId]],
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
}
function parseWarpRouteResponse(response: any): {
  routeId: string;
  tokenId: string;
} {
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
    (e: any) => e?.key === "Bank/TokenCreated",
  );
  let tokenId: string | undefined;
  if (tokenCreatedEvent) {
    // @ts-ignore
    tokenId = tokenCreatedEvent?.value?.token_created?.coins?.token_id;
    if (tokenId) {
      console.log("[✓] Token created");
      console.log("  Token ID:", tokenId);
    } else {
      console.error(
        "[✗] Bank/TokenCreated event found but token_id is missing!",
      );
      process.exit(1);
    }
  } else {
    console.error("[✗] Bank/TokenCreated event not found!");
    process.exit(1);
  }
  // 3. Find and print route_id from the Warp/RouteRegistered event
  const routeRegisteredEvent = events.find(
    (e: any) => e?.key === "Warp/RouteRegistered",
  );
  let routeId: string | undefined;
  if (routeRegisteredEvent) {
    // @ts-ignore
    routeId = routeRegisteredEvent?.value?.route_registered?.route_id;
    if (routeId) {
      console.log("[✓] Warp route registered");
      console.log("  Route ID:", routeId);
    } else {
      console.error(
        "[✗] Warp/RouteRegistered event found but route_id is missing!",
      );
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
  return { routeId, tokenId };
}
const setRelayerConfig: RuntimeCall = {
  interchain_gas_paymaster: {
    set_relayer_config: {
      beneficiary: "0xA6edfca3AA985Dd3CC728BFFB700933a986aC085",
      default_gas: defaultGas,
      domain_default_gas: [
        {
          default_gas: ETHTEST_DEFAULT_GAS,
          domain: 11155111,
        },
      ],
      domain_oracle_data: [
        {
          // TODO: Dummy values now, need to figure out how to set them up
          data_value: {
            gas_price: 1,
            token_exchange_rate: 1,
          },
          domain: 11155111,
        },
      ],
    },
  },
};
const rollup = await createStandardRollup({
  url: "http://ale-drynet.sovereign-labs.xyz:80",
});
console.log("Rollup client initialized");
const createWarpRoute = buildCreateWarpRouteCall();
let deployerSigner = new Secp256k1Signer(
  "0187c12ea7c12024b3f70ac5d73587463af17c8bce2bd9e6fe87389310196c64",
);
const warpRegisterResponse = await rollup.call(createWarpRoute, {
  signer: deployerSigner, overrides: {
    details: {
      max_fee: 0,
    }
  }
});
console.log("Create warp router response:");
const { routeId, tokenId } = parseWarpRouteResponse(warpRegisterResponse);
console.log("\nSummary:");
console.log(`  Route ID: ${routeId}`);
console.log(`  Token ID: ${tokenId}`);
const minterSigner = new Secp256k1Signer(minterPrivateKey);
const response = await rollup.call(setRelayerConfig, { signer: minterSigner, overrides: {
    details: {
      max_fee: 0,
    }
} });
console.log("Relayer config response");
console.log(JSON.stringify(response.response));
