use std::marker::PhantomData;

use alloy_primitives::{Address, Bytes};
use borsh::BorshDeserialize;
use sov_evm::precompiles::{
    EvmPrecompile, EvmPrecompileEnv, PrecompileError, PrecompileOutput, PrecompileResult,
};
use sov_modules_api::{Spec, TxState};

use crate::prices::lookup_feed_report;
use crate::types::PriceReports;

/// Precompile address 0x0000000000000000000000000000000000010002.
pub const PRICE_ORACLE_PRECOMPILE_ADDRESS: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0x00, 0x02,
]);

pub const PRICE_ORACLE_PRECOMPILE_BASE_GAS: u64 = 3_000;
pub const PRICE_ORACLE_PRECOMPILE_WORD_GAS: u64 = 16;

#[derive(Clone, Default)]
pub struct PriceOraclePrecompile<S>(PhantomData<S>);

impl<S: Spec> EvmPrecompile<S> for PriceOraclePrecompile<S> {
    const ADDRESS: Address = PRICE_ORACLE_PRECOMPILE_ADDRESS;

    fn execute<ST: TxState<S>>(
        &self,
        input: &[u8],
        gas_limit: u64,
        env: &mut EvmPrecompileEnv<'_, S, ST>,
    ) -> PrecompileResult {
        if PRICE_ORACLE_PRECOMPILE_BASE_GAS > gas_limit {
            return Err(PrecompileError::OutOfGas);
        }

        let (provider_id, feed_id) = decode_feed_request(input)?;

        let context = env
            .sov_context
            .ok_or_else(|| PrecompileError::State("price oracle data unavailable".to_string()))?;
        let sequencing_data = context
            .sequencing_data()
            .as_ref()
            .ok_or_else(|| PrecompileError::State("price oracle data unavailable".to_string()))?;
        let reports = PriceReports::try_from_slice(sequencing_data).map_err(|err| {
            PrecompileError::State(format!("could not decode sequencing data: {err}"))
        })?;

        let payload = lookup_feed_report(&reports, provider_id, feed_id).ok_or_else(|| {
            PrecompileError::InvalidInput(format!(
                "no price report for provider {provider_id} feed {feed_id}"
            ))
        })?;

        // Record the feed before the gas check. The payload length affects gas,
        // so a feed read here must be kept even if the call then runs out of gas,
        // otherwise replay from the DA layer would diverge.
        #[cfg(feature = "native")]
        crate::sequencing::record_used_feed_key(
            context,
            crate::types::FeedKey::new(provider_id, feed_id),
        )
        .map_err(|err| PrecompileError::State(format!("could not record used feed key: {err}")))?;

        let words = payload.len().div_ceil(32) as u64;
        let gas_used = PRICE_ORACLE_PRECOMPILE_BASE_GAS + PRICE_ORACLE_PRECOMPILE_WORD_GAS * words;
        if gas_used > gas_limit {
            return Err(PrecompileError::OutOfGas);
        }

        Ok(PrecompileOutput {
            gas_used,
            bytes: Bytes::from(payload.clone()),
        })
    }
}

pub fn decode_feed_request(
    input: &[u8],
) -> Result<(alloy_primitives::B256, alloy_primitives::B256), PrecompileError> {
    if input.len() != 64 {
        return Err(PrecompileError::InvalidInput(format!(
            "expected 64 byte input with provider id and feed id, got {}",
            input.len()
        )));
    }
    let provider_id = alloy_primitives::B256::from_slice(&input[0..32]);
    let feed_id = alloy_primitives::B256::from_slice(&input[32..64]);
    Ok((provider_id, feed_id))
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use alloy_primitives::{keccak256, B256};

    use super::*;

    static PROVIDER_ID: LazyLock<B256> = LazyLock::new(|| keccak256("chainlink"));

    fn feed_id(suffix: u8) -> B256 {
        let mut bytes = [0u8; 32];
        bytes[1] = 0x03;
        bytes[31] = suffix;
        B256::from(bytes)
    }

    fn request(provider_id: B256, feed_id: B256) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(provider_id.as_slice());
        bytes.extend_from_slice(feed_id.as_slice());
        bytes
    }

    #[test]
    fn decode_splits_provider_and_feed() {
        let (provider, feed) =
            decode_feed_request(&request(*PROVIDER_ID, feed_id(1))).expect("decode");
        assert_eq!(provider, *PROVIDER_ID);
        assert_eq!(feed, feed_id(1));
    }

    #[test]
    fn decode_rejects_non_64_byte_input() {
        for len in [0usize, 3, 32, 63, 65, 68, 96] {
            let err = decode_feed_request(&vec![0u8; len]).unwrap_err();
            assert!(
                matches!(err, PrecompileError::InvalidInput(_)),
                "input length {len} should be rejected as invalid"
            );
        }
    }
}
