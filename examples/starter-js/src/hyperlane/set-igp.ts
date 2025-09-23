// An example of a transaction to configure the inter-chain gas paymaster for hyperlane.
import {
  RuntimeCall,
  DomainOracleData,
  DomainDefaultGas,
  SetRelayerConfig,
} from "../types";
import {defaultGas, ROLLUP_STARTER_DOMAIN, SOLANA_TESTNET_DOMAIN} from "./consts";

// --- Define constants that were present in the Rust code ---
// These would typically come from your configuration or environment
// --- End of chain-specific constant definitions ---

const domainOracles: DomainOracleData[] = [
  {
    domain: ROLLUP_STARTER_DOMAIN,
    data_value: {
      gas_price: 1,
      token_exchange_rate: 1, // This exchange rate will need to change!
    },
  },
  {
    domain: SOLANA_TESTNET_DOMAIN,
    data_value: {
      gas_price: 1,
      token_exchange_rate: 1, // This exchange rate will need to change!
    },
  },
];

const domainGas: DomainDefaultGas[] = [
  {
    domain: ROLLUP_STARTER_DOMAIN,
    default_gas: defaultGas, // TODO: Set reasonable default gas amount for sov txs
  },
  {
    domain: SOLANA_TESTNET_DOMAIN,
    default_gas: defaultGas, // TODO: Seta reasonable default gas for Solana transactions
  },
];

function setRelayerConfigPayload(relayer_address: string): SetRelayerConfig {
  return {
    domain_oracle_data: domainOracles,
    domain_default_gas: domainGas,
    default_gas: defaultGas,
    beneficiary: relayer_address,
  };
}

export function setIgpCall(relayer_address: string): RuntimeCall {
  return {
    interchain_gas_paymaster: {
      set_relayer_config: setRelayerConfigPayload(relayer_address),
    },
  };
}
