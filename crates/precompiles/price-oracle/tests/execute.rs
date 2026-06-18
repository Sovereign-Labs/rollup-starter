#![cfg(feature = "native")]

use alloy_primitives::{keccak256, B256};
use borsh::BorshDeserialize;
use price_oracle::{
    FeedKey, PriceOraclePrecompile, SerializedPriceUpdates, UsedFeedKeys,
    PRICE_ORACLE_PRECOMPILE_BASE_GAS, PRICE_ORACLE_PRECOMPILE_WORD_GAS,
};
use sov_evm::precompiles::{EvmPrecompile, EvmPrecompileEnv, PrecompileError};
use sov_modules_api::capabilities::mocks::MockKernel;
use sov_modules_api::transaction::Credentials;
use sov_modules_api::{
    Context, DaSpec, ExecutionContext, SequencerType, Spec, StateCheckpoint, TxState,
};
use sov_test_utils::storage::SimpleStorageManager;
use sov_test_utils::{TestSpec, TestStorageSpec};

type S = TestSpec;

const GAS_LIMIT: u64 = 1_000_000;

fn provider_id() -> B256 {
    keccak256("chainlink")
}

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

fn updates_with(entries: &[(FeedKey, &[u8])]) -> SerializedPriceUpdates {
    SerializedPriceUpdates(entries.iter().map(|(k, p)| (*k, p.to_vec())).collect())
}

fn expected_gas(payload_len: usize) -> u64 {
    PRICE_ORACLE_PRECOMPILE_BASE_GAS
        + PRICE_ORACLE_PRECOMPILE_WORD_GAS * payload_len.div_ceil(32) as u64
}

fn fresh_state() -> (SimpleStorageManager<TestStorageSpec>, impl TxState<S>) {
    let storage_manager = SimpleStorageManager::<TestStorageSpec>::new();
    let storage = storage_manager.create_storage();
    let working_set =
        StateCheckpoint::<S>::new(storage, &MockKernel::<S>::default()).to_working_set_unmetered();
    (storage_manager, working_set)
}

fn context(updates: Option<&SerializedPriceUpdates>) -> Context<S> {
    let addr = <S as Spec>::Address::from([7u8; 28]);
    let da_addr = <<S as Spec>::Da as DaSpec>::Address::from([9u8; 32]);
    let sequencing_data = updates.map(|u| {
        sov_rollup_interface::Bytes::from(borsh::to_vec(u).expect("encode sequencing data"))
    });
    Context::<S>::new(
        addr,
        Credentials::default(),
        addr,
        da_addr,
        sequencing_data,
        ExecutionContext::Node,
        SequencerType::Preferred,
    )
}

#[test]
fn returns_payload_and_charges_base_plus_word_gas() {
    let payload = b"signed-update-bytes".to_vec();
    let updates = updates_with(&[(FeedKey::new(provider_id(), feed_id(1)), &payload)]);
    let ctx = context(Some(&updates));
    let (_storage, mut state) = fresh_state();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context: Some(&ctx),
    };

    let output = PriceOraclePrecompile::<S>::default()
        .execute(&request(provider_id(), feed_id(1)), GAS_LIMIT, &mut env)
        .expect("present feed should resolve");

    assert_eq!(output.bytes.as_ref(), payload.as_slice());
    assert_eq!(output.gas_used, expected_gas(payload.len()));
}

#[test]
fn missing_feed_is_invalid_input() {
    let updates = updates_with(&[(FeedKey::new(provider_id(), feed_id(1)), b"present")]);
    let ctx = context(Some(&updates));
    let (_storage, mut state) = fresh_state();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context: Some(&ctx),
    };

    let err = PriceOraclePrecompile::<S>::default()
        .execute(&request(provider_id(), feed_id(2)), GAS_LIMIT, &mut env)
        .unwrap_err();
    assert!(matches!(err, PrecompileError::InvalidInput(_)));
}

#[test]
fn wrong_length_input_is_invalid_input() {
    let updates = updates_with(&[(FeedKey::new(provider_id(), feed_id(1)), b"present")]);
    let ctx = context(Some(&updates));
    let (_storage, mut state) = fresh_state();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context: Some(&ctx),
    };

    let err = PriceOraclePrecompile::<S>::default()
        .execute(&[0u8; 10], GAS_LIMIT, &mut env)
        .unwrap_err();
    assert!(matches!(err, PrecompileError::InvalidInput(_)));
}

#[test]
fn missing_sequencing_context_is_state_error() {
    let ctx = context(None);
    let (_storage, mut state) = fresh_state();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context: Some(&ctx),
    };

    let err = PriceOraclePrecompile::<S>::default()
        .execute(&request(provider_id(), feed_id(1)), GAS_LIMIT, &mut env)
        .unwrap_err();
    assert!(matches!(err, PrecompileError::State(_)));
}

#[test]
fn insufficient_base_gas_is_out_of_gas() {
    let payload = b"present".to_vec();
    let updates = updates_with(&[(FeedKey::new(provider_id(), feed_id(1)), &payload)]);
    let ctx = context(Some(&updates));
    let (_storage, mut state) = fresh_state();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context: Some(&ctx),
    };

    let err = PriceOraclePrecompile::<S>::default()
        .execute(
            &request(provider_id(), feed_id(1)),
            PRICE_ORACLE_PRECOMPILE_BASE_GAS - 1,
            &mut env,
        )
        .unwrap_err();
    assert!(matches!(err, PrecompileError::OutOfGas));
}

#[test]
fn payload_gas_exceeding_limit_is_out_of_gas() {
    let payload = vec![0xab; 64];
    let updates = updates_with(&[(FeedKey::new(provider_id(), feed_id(1)), &payload)]);
    let ctx = context(Some(&updates));
    let (_storage, mut state) = fresh_state();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context: Some(&ctx),
    };

    let gas_limit = PRICE_ORACLE_PRECOMPILE_BASE_GAS + PRICE_ORACLE_PRECOMPILE_WORD_GAS;
    let err = PriceOraclePrecompile::<S>::default()
        .execute(&request(provider_id(), feed_id(1)), gas_limit, &mut env)
        .unwrap_err();
    assert!(matches!(err, PrecompileError::OutOfGas));
}

#[test]
fn records_used_feed_key_in_scratchpad() {
    let payload = b"present".to_vec();
    let key = FeedKey::new(provider_id(), feed_id(1));
    let updates = updates_with(&[(key, &payload)]);
    let ctx = context(Some(&updates));
    let (_storage, mut state) = fresh_state();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context: Some(&ctx),
    };

    PriceOraclePrecompile::<S>::default()
        .execute(&request(provider_id(), feed_id(1)), GAS_LIMIT, &mut env)
        .expect("present feed should resolve");

    let recorded = ctx.sequencing_scratchpad().with_value(|scratchpad| {
        let bytes = scratchpad
            .as_deref()
            .expect("scratchpad should be populated");
        UsedFeedKeys::try_from_slice(bytes).expect("decode used feed keys")
    });
    assert_eq!(recorded.0, vec![key]);
}
