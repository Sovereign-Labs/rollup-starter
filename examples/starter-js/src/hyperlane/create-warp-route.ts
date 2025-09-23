import { RuntimeCall } from "../types";
import {minterAddress, SOLANA_TESTNET_DOMAIN} from "./consts";

// An example of a call to register a warp route.
export const createWarpRoute: RuntimeCall = {
  warp: {
    register: {
      // The admin can modify the ISM (aka. trusted relayer/validators)
      admin: {
        InsecureOwner: minterAddress,
      },
      ism: {
        // Initially, trust the relayer rather than using an additional validator set for *inbound* messages
        TrustedRelayer: {
          // The hex encoding of the relayer address `0x9b08ce57a93751aE790698A2C9ebc76A78F23E25`
          relayer:
            "0x000000007b758bf2e7670fafaf6bf0015ce0ff5aa802306fc7e3f45762853ffc",
        },
      },
      token_source: {
        Synthetic: {
          remote_token_id:
            "0x264ae4d8bb90248557e7e039afaf384b64fbc821e56f45ebb524d74dfe8cc30d", // The solana program ID of a pre-deployed counterparty. You'll need to replace this in your deployment
          local_decimals: 9,
          remote_decimals: 9,
        },
      },
      remote_routers: [
        [
          SOLANA_TESTNET_DOMAIN,
          "0x264ae4d8bb90248557e7e039afaf384b64fbc821e56f45ebb524d74dfe8cc30d",
        ],
      ],
    },
  },
};
