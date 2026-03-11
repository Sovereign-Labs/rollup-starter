import { createStandardRollup } from "@sovereign-sdk/web3";
import { Secp256k1Signer } from "@sovereign-sdk/signers";
import { Wallet } from "ethers";
import { Buffer } from "node:buffer";
import { minterPrivateKey } from "../hyperlane/consts";

const ROLLUP_URL = "http://127.0.0.1:12346";
const BOOTSTRAP_PRIVATE_KEY = minterPrivateKey;
const DEFAULT_FUNDING_AMOUNT = 1_000_000;
const DEFAULT_RUNTIME_BYTECODE = "6080604052348015600e575f5ffd5b5061092f8061001c5f395ff3fe608060405234801561000f575f5ffd5b505f3660605f8383810190610024919061059c565b905061002f8161004c565b60405180602001604052805f815250915050915050805190602001f35b5f816040015167ffffffffffffffff160361009c576040517fb460515d0000000000000000000000000000000000000000000000000000000081526004016100939061063d565b60405180910390fd5b5f816020015167ffffffffffffffff16036100ec576040517fb460515d0000000000000000000000000000000000000000000000000000000081526004016100e3906106a5565b60405180910390fd5b6004815f015167ffffffffffffffff161461013c576040517fb460515d0000000000000000000000000000000000000000000000000000000081526004016101339061070d565b60405180910390fd5b6103e8816020015182604001516101539190610758565b67ffffffffffffffff16111561019e576040517fb460515d000000000000000000000000000000000000000000000000000000008152600401610195906107de565b60405180910390fd5b5f8160c00151511180156101f757505f60f81b8160c001515f815181106101c8576101c76107fc565b5b602001015160f81c60f81b7effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916145b15610237576040517fb460515d00000000000000000000000000000000000000000000000000000000815260040161022e90610873565b60405180910390fd5b5f8160e001515111801561029057505f60f81b8160e001515f81518110610261576102606107fc565b5b602001015160f81c60f81b7effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916145b156102d0576040517fb460515d0000000000000000000000000000000000000000000000000000000081526004016102c7906108db565b60405180910390fd5b50565b5f604051905090565b5f5ffd5b5f5ffd5b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b61032e826102e8565b810181811067ffffffffffffffff8211171561034d5761034c6102f8565b5b80604052505050565b5f61035f6102d3565b905061036b8282610325565b919050565b5f5ffd5b5f67ffffffffffffffff82169050919050565b61039081610374565b811461039a575f5ffd5b50565b5f813590506103ab81610387565b92915050565b5f5ffd5b5f5ffd5b5f67ffffffffffffffff8211156103d3576103d26102f8565b5b6103dc826102e8565b9050602081019050919050565b828183375f83830152505050565b5f610409610404846103b9565b610356565b905082815260208101848484011115610425576104246103b5565b5b6104308482856103e9565b509392505050565b5f82601f83011261044c5761044b6103b1565b5b813561045c8482602086016103f7565b91505092915050565b5f610100828403121561047b5761047a6102e4565b5b610486610100610356565b90505f6104958482850161039d565b5f8301525060206104a88482850161039d565b60208301525060406104bc8482850161039d565b604083015250606082013567ffffffffffffffff8111156104e0576104df610370565b5b6104ec84828501610438565b606083015250608082013567ffffffffffffffff8111156105105761050f610370565b5b61051c84828501610438565b60808301525060a06105308482850161039d565b60a08301525060c082013567ffffffffffffffff81111561055457610553610370565b5b61056084828501610438565b60c08301525060e082013567ffffffffffffffff81111561058457610583610370565b5b61059084828501610438565b60e08301525092915050565b5f602082840312156105b1576105b06102dc565b5b5f82013567ffffffffffffffff8111156105ce576105cd6102e0565b5b6105da84828501610465565b91505092915050565b5f82825260208201905092915050565b7f7175616e74697479206973207a65726f000000000000000000000000000000005f82015250565b5f6106276010836105e3565b9150610632826105f3565b602082019050919050565b5f6020820190508181035f8301526106548161061b565b9050919050565b7f7072696365206973207a65726f000000000000000000000000000000000000005f82015250565b5f61068f600d836105e3565b915061069a8261065b565b602082019050919050565b5f6020820190508181035f8301526106bc81610683565b9050919050565b7f4f6e6c79206173736574203420697320616c6c6f7765640000000000000000005f82015250565b5f6106f76017836105e3565b9150610702826106c3565b602082019050919050565b5f6020820190508181035f830152610724816106eb565b9050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f61076282610374565b915061076d83610374565b925082820261077b81610374565b915080821461078d5761078c61072b565b5b5092915050565b7f4d617820627564676574206973203130303020746f6b656e73000000000000005f82015250565b5f6107c86019836105e3565b91506107d382610794565b602082019050919050565b5f6020820190508181035f8301526107f5816107bc565b9050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52603260045260245ffd5b7f6c6f6e6720736964652072656a656374656400000000000000000000000000005f82015250565b5f61085d6012836105e3565b915061086882610829565b602082019050919050565b5f6020820190508181035f83015261088a81610851565b9050919050565b7f73686f727420736964652072656a6563746564000000000000000000000000005f82015250565b5f6108c56013836105e3565b91506108d082610891565b602082019050919050565b5f6020820190508181035f8301526108f2816108b9565b905091905056fea2646970667358221220d4d74082ba21b1123a1c1f5c716213511717173a596ba95693172fb11da6009964736f6c634300081f0033";

type RollupCall =
  | {
      bank: {
        transfer: {
          to: string;
          coins: {
            amount: number;
            token_id: string;
          };
        };
      };
    }
  | {
      derive: {
        set_authorizer?: number[];
        match?: {
          id: number;
          price: number;
          quantity: number;
          long_account: string;
          short_account: string;
          timestamp: number;
          long_account_calldata: number[];
          short_account_calldata: number[];
        };
      };
    };

type RollupResponse = {
  response: {
    receipt: {
      result?: string;
    };
    events?: Array<{
      key: string;
      value: unknown;
    }>;
  };
};

type GeneratedAccount = {
  label: string;
  address: string;
  privateKey: string;
  signer: Secp256k1Signer;
};

function normalizeHex(hex: string): string {
  const stripped = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (stripped.length === 0) {
    return "";
  }
  if (stripped.length % 2 !== 0) {
    throw new Error(`Hex string must have an even number of characters: ${hex}`);
  }
  if (!/^[0-9a-fA-F]+$/.test(stripped)) {
    throw new Error(`Invalid hex string: ${hex}`);
  }
  return stripped.toLowerCase();
}

function hexToBytes(hex: string): number[] {
  const normalized = normalizeHex(hex);
  if (normalized.length === 0) {
    return [];
  }

  const bytes: number[] = [];
  for (let i = 0; i < normalized.length; i += 2) {
    bytes.push(Number.parseInt(normalized.slice(i, i + 2), 16));
  }
  return bytes;
}

function createGeneratedAccount(label: string): GeneratedAccount {
  const wallet = Wallet.createRandom();
  const privateKey = wallet.privateKey.slice(2);

  return {
    label,
    address: wallet.address,
    privateKey,
    signer: new Secp256k1Signer(privateKey),
  };
}

async function getGasTokenId(rollupUrl: string): Promise<string> {
  const response = await fetch(`${rollupUrl}/modules/bank/tokens/gas_token`);
  if (!response.ok) {
    throw new Error(`Failed to fetch gas token id: ${response.status} ${response.statusText}`);
  }

  const body = (await response.json()) as { token_id?: string };
  if (!body.token_id) {
    throw new Error("Gas token query succeeded but the response did not contain token_id");
  }

  return body.token_id;
}

function requireSuccess(step: string, response: RollupResponse): void {
  const result = response.response.receipt?.result;
  if (result !== "successful") {
    throw new Error(`${step} failed with receipt result ${String(result)}`);
  }
}

async function submitCall(
  rollup: Awaited<ReturnType<typeof createStandardRollup>>,
  signer: Secp256k1Signer,
  label: string,
  call: RollupCall,
): Promise<RollupResponse> {
  console.log(`Submitting ${label}...`);
  const response = (await rollup.call(call as any, { signer })) as RollupResponse;
  requireSuccess(label, response);

  console.log(`${label} receipt: successful`);
  if (response.response.events && response.response.events.length > 0) {
    for (const event of response.response.events) {
      console.log(`  event ${event.key}: ${JSON.stringify(event.value)}`);
    }
  }

  return response;
}

const bootstrapSigner = new Secp256k1Signer(BOOTSTRAP_PRIVATE_KEY);
const maker = createGeneratedAccount("maker");
const taker = createGeneratedAccount("taker");
const admin = createGeneratedAccount("admin");

console.log("Generated accounts:");
for (const account of [maker, taker, admin]) {
  console.log(`  ${account.label}: ${account.address}`);
  console.log(`    private key: 0x${account.privateKey}`);
}

console.log("Initializing rollup client...");
const rollup = await createStandardRollup({ url: ROLLUP_URL });
console.log("Rollup client initialized.");

const gasTokenId = await getGasTokenId(ROLLUP_URL);
console.log(`Gas token id: ${gasTokenId}`);

const makerAuthorizerBytecode = hexToBytes(
  process.env.MAKER_AUTHORIZER_RUNTIME_BYTECODE ?? DEFAULT_RUNTIME_BYTECODE,
);
const takerAuthorizerBytecode = hexToBytes(
  process.env.TAKER_AUTHORIZER_RUNTIME_BYTECODE ?? DEFAULT_RUNTIME_BYTECODE,
);

console.log("Authorizer runtime bytecode:");
console.log(
  `  maker: 0x${Buffer.from(makerAuthorizerBytecode).toString("hex") || DEFAULT_RUNTIME_BYTECODE}`,
);
console.log(
  `  taker: 0x${Buffer.from(takerAuthorizerBytecode).toString("hex") || DEFAULT_RUNTIME_BYTECODE}`,
);
console.log(
  "Replace the default STOP runtime with your compiled Solidity runtime bytecode, for example via MAKER_AUTHORIZER_RUNTIME_BYTECODE/TAKER_AUTHORIZER_RUNTIME_BYTECODE.",
);

for (const account of [maker, taker, admin]) {
  await submitCall(rollup, bootstrapSigner, `fund ${account.label}`, {
    bank: {
      transfer: {
        to: account.address,
        coins: {
          amount: DEFAULT_FUNDING_AMOUNT,
          token_id: gasTokenId,
        },
      },
    },
  });
}

await submitCall(rollup, maker.signer, "set maker authorizer", {
  derive: {
    set_authorizer: makerAuthorizerBytecode,
  },
});

await submitCall(rollup, taker.signer, "set taker authorizer", {
  derive: {
    set_authorizer: takerAuthorizerBytecode,
  },
});

const matchCall: RollupCall = {
  derive: {
    match: {
      id: 1,
      price: 42,
      quantity: 3,
      long_account: maker.address,
      short_account: taker.address,
      timestamp: Math.floor(Date.now() / 1000),
      long_account_calldata: [0x01],
      short_account_calldata: [0x01],
    },
  },
};

console.log("Submitting exchange match:");
console.log(JSON.stringify(matchCall, null, 2));

await submitCall(rollup, admin.signer, "submit match", matchCall);

console.log("Completed authorized match flow.");
