#![cfg(feature = "native")]

use std::sync::LazyLock;

use alloy_primitives::{keccak256, B256};
use borsh::BorshDeserialize;
use bytes::Bytes;
use price_oracle::{
    FeedKey, PriceOraclePrecompile, PriceReports, UsedFeedKeys, PRICE_ORACLE_PRECOMPILE_BASE_GAS,
    PRICE_ORACLE_PRECOMPILE_WORD_GAS,
};
use sov_evm::precompiles::{EvmPrecompile, EvmPrecompileEnv, PrecompileError, PrecompileResult};
use sov_modules_api::capabilities::mocks::MockKernel;
use sov_modules_api::transaction::Credentials;
use sov_modules_api::{Context, DaSpec, ExecutionContext, SequencerType, Spec, StateCheckpoint};
use sov_test_utils::storage::SimpleStorageManager;
use sov_test_utils::{TestSpec, TestStorageSpec};

type S = TestSpec;

const GAS_LIMIT: u64 = 1_000_000;

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

fn reports_with(entries: &[(FeedKey, &[u8])]) -> PriceReports {
    PriceReports(
        entries
            .iter()
            .map(|(k, p)| (*k, Bytes::copy_from_slice(p)))
            .collect(),
    )
}

fn expected_gas(payload_len: usize) -> u64 {
    PRICE_ORACLE_PRECOMPILE_BASE_GAS
        + PRICE_ORACLE_PRECOMPILE_WORD_GAS * payload_len.div_ceil(32) as u64
}

fn context(reports: Option<&PriceReports>) -> Context<S> {
    let sequencing_data = reports.map(|u| {
        sov_rollup_interface::Bytes::from(borsh::to_vec(u).expect("encode sequencing data"))
    });
    context_with_raw_sequencing_data(sequencing_data)
}

fn context_with_raw_sequencing_data(
    sequencing_data: Option<sov_rollup_interface::Bytes>,
) -> Context<S> {
    let addr = <S as Spec>::Address::from([7u8; 28]);
    let da_addr = <<S as Spec>::Da as DaSpec>::Address::from([9u8; 32]);
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

fn run(sov_context: Option<&Context<S>>, input: &[u8], gas_limit: u64) -> PrecompileResult {
    let storage_manager = SimpleStorageManager::<TestStorageSpec>::new();
    let storage = storage_manager.create_storage();
    let mut state =
        StateCheckpoint::<S>::new(storage, &MockKernel::<S>::default()).to_working_set_unmetered();
    let mut env = EvmPrecompileEnv {
        state: &mut state,
        sov_context,
    };
    PriceOraclePrecompile::<S>::default().execute(input, gas_limit, &mut env)
}

fn used_feed_keys(ctx: &Context<S>) -> Vec<FeedKey> {
    ctx.sequencing_scratchpad().with_value(|scratchpad| {
        let bytes = scratchpad
            .as_deref()
            .expect("scratchpad should be populated");
        UsedFeedKeys::try_from_slice(bytes)
            .expect("decode used feed keys")
            .0
            .into_iter()
            .collect()
    })
}

#[test]
fn present_feed_returns_payload_and_gas() {
    let payload = b"signed-update-bytes".to_vec();
    let reports = reports_with(&[(FeedKey::new(*PROVIDER_ID, feed_id(1)), &payload)]);
    let ctx = context(Some(&reports));

    let output = run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT)
        .expect("present feed should resolve");

    assert_eq!(output.bytes.as_ref(), payload.as_slice());
    assert_eq!(output.gas_used, expected_gas(payload.len()));
}

#[test]
fn missing_feed_is_invalid_input() {
    let reports = reports_with(&[(FeedKey::new(*PROVIDER_ID, feed_id(1)), b"present")]);
    let ctx = context(Some(&reports));

    let err = run(Some(&ctx), &request(*PROVIDER_ID, feed_id(2)), GAS_LIMIT).unwrap_err();
    assert!(matches!(err, PrecompileError::InvalidInput(_)));
}

#[test]
fn empty_reports_is_invalid_input() {
    let ctx = context(Some(&reports_with(&[])));

    let err = run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT).unwrap_err();
    assert!(matches!(err, PrecompileError::InvalidInput(_)));
}

#[test]
fn wrong_length_request_is_invalid_input() {
    let reports = reports_with(&[(FeedKey::new(*PROVIDER_ID, feed_id(1)), b"present")]);
    let ctx = context(Some(&reports));

    let err = run(Some(&ctx), &[0u8; 10], GAS_LIMIT).unwrap_err();
    assert!(matches!(err, PrecompileError::InvalidInput(_)));
}

#[test]
fn missing_context_is_state_error() {
    let err = run(None, &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT).unwrap_err();
    assert!(matches!(err, PrecompileError::State(_)));
}

#[test]
fn missing_sequencing_data_is_state_error() {
    let ctx = context(None);

    let err = run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT).unwrap_err();
    assert!(matches!(err, PrecompileError::State(_)));
}

#[test]
fn undecodable_sequencing_data_is_state_error() {
    let ctx =
        context_with_raw_sequencing_data(Some(sov_rollup_interface::Bytes::from(vec![0xff; 8])));

    let err = run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT).unwrap_err();
    assert!(matches!(err, PrecompileError::State(_)));
}

#[test]
fn insufficient_base_gas_is_out_of_gas() {
    let reports = reports_with(&[(FeedKey::new(*PROVIDER_ID, feed_id(1)), b"present")]);
    let ctx = context(Some(&reports));

    let err = run(
        Some(&ctx),
        &request(*PROVIDER_ID, feed_id(1)),
        PRICE_ORACLE_PRECOMPILE_BASE_GAS - 1,
    )
    .unwrap_err();
    assert!(matches!(err, PrecompileError::OutOfGas));
}

#[test]
fn payload_gas_over_limit_is_out_of_gas() {
    let payload = vec![0xab; 64];
    let reports = reports_with(&[(FeedKey::new(*PROVIDER_ID, feed_id(1)), &payload)]);
    let ctx = context(Some(&reports));

    let one_word_short = expected_gas(payload.len()) - PRICE_ORACLE_PRECOMPILE_WORD_GAS;
    let err = run(
        Some(&ctx),
        &request(*PROVIDER_ID, feed_id(1)),
        one_word_short,
    )
    .unwrap_err();
    assert!(matches!(err, PrecompileError::OutOfGas));
}

#[test]
fn exact_gas_limit_succeeds() {
    let payload = b"signed-update-bytes".to_vec();
    let reports = reports_with(&[(FeedKey::new(*PROVIDER_ID, feed_id(1)), &payload)]);
    let ctx = context(Some(&reports));

    let exact = expected_gas(payload.len());
    let output = run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), exact)
        .expect("exact gas limit should resolve");
    assert_eq!(output.gas_used, exact);
}

#[test]
fn charges_gas_per_word() {
    for (len, words) in [(0usize, 0u64), (32, 1), (33, 2)] {
        let payload = vec![0xab; len];
        let reports = reports_with(&[(FeedKey::new(*PROVIDER_ID, feed_id(1)), &payload)]);
        let ctx = context(Some(&reports));

        let output = run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT)
            .expect("present feed should resolve");

        assert_eq!(
            output.gas_used,
            PRICE_ORACLE_PRECOMPILE_BASE_GAS + PRICE_ORACLE_PRECOMPILE_WORD_GAS * words,
            "payload of {len} bytes should cost {words} word(s)"
        );
    }
}

#[test]
fn records_used_feed_key() {
    let key = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let reports = reports_with(&[(key, b"present")]);
    let ctx = context(Some(&reports));

    run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT)
        .expect("present feed should resolve");

    assert_eq!(used_feed_keys(&ctx), vec![key]);
}

#[test]
fn accumulates_used_feed_keys() {
    let key1 = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let key2 = FeedKey::new(*PROVIDER_ID, feed_id(2));
    let reports = reports_with(&[(key1, b"first"), (key2, b"second")]);
    let ctx = context(Some(&reports));

    run(Some(&ctx), &request(*PROVIDER_ID, feed_id(1)), GAS_LIMIT)
        .expect("first feed should resolve");
    run(Some(&ctx), &request(*PROVIDER_ID, feed_id(2)), GAS_LIMIT)
        .expect("second feed should resolve");

    assert_eq!(used_feed_keys(&ctx), vec![key1, key2]);
}

#[test]
fn records_used_feed_key_even_when_out_of_gas() {
    // A feed read here still runs out of gas, but it must be recorded so the
    // sequencer keeps it and replay from the DA layer reaches the same outcome.
    let payload = vec![0xab; 64];
    let key = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let reports = reports_with(&[(key, &payload)]);
    let ctx = context(Some(&reports));

    let one_word_short = expected_gas(payload.len()) - PRICE_ORACLE_PRECOMPILE_WORD_GAS;
    let err = run(
        Some(&ctx),
        &request(*PROVIDER_ID, feed_id(1)),
        one_word_short,
    )
    .unwrap_err();
    assert!(matches!(err, PrecompileError::OutOfGas));

    assert_eq!(used_feed_keys(&ctx), vec![key]);
}
