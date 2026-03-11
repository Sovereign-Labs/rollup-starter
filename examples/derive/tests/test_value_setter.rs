use sov_modules_api::Spec;
use sov_test_utils::{generate_optimistic_runtime, TestSpec};
use value_setter::{CallMessage, Derive, Match};

type S = TestSpec;

// This macro creates a temporary runtime for testing.
generate_optimistic_runtime!(
    TestRuntime <=
    derive: Derive<S>
);

use sov_test_utils::runtime::genesis::optimistic::HighLevelOptimisticGenesisConfig;
use sov_test_utils::runtime::TestRunner;
use sov_test_utils::TestUser;

// A helper struct to hold our test users, for convenience.
pub struct TestData<S: Spec> {
    pub long_user: TestUser<S>,
    pub short_user: TestUser<S>,
}

pub fn setup() -> (TestData<S>, TestRunner<TestRuntime<S>, S>) {
    // Create a regular user.
    // (The `HighLevelOptimisticGenesisConfig` builder is a convenient way
    // to set up the initial state for core modules.)
    let genesis_config =
        HighLevelOptimisticGenesisConfig::generate().add_accounts_with_default_balance(2);

    let mut users = genesis_config.additional_accounts().to_vec();
    let short_user = users.pop().unwrap();
    let long_user = users.pop().unwrap();

    let test_data = TestData {
        long_user,
        short_user,
    };

    // Build the final genesis config by combining
    // the core config with our module's specific config.
    let genesis = GenesisConfig::from_minimal_config(genesis_config.into(), ());

    // Initialize the TestRunner with the genesis state.
    // The runner gives us a simple way to execute transactions and query state.
    let runner =
        TestRunner::new_with_genesis(genesis.into_genesis_params(), TestRuntime::default());

    (test_data, runner)
}

fn sample_match(test_data: &TestData<S>) -> Match<S> {
    Match {
        id: 1,
        price: 42,
        quantity: 3,
        long_account: test_data.long_user.address(),
        short_account: test_data.short_user.address(),
        timestamp: 123_456,
        long_account_calldata: vec![0xaa],
        short_account_calldata: vec![0xbb],
    }
}

use sov_test_utils::{AsUser, TransactionTestCase};

#[test]
fn test_match_reverts_without_a_long_account_authorizer() {
    let (test_data, mut runner) = setup();
    let long_user = &test_data.long_user;
    let match_msg = sample_match(&test_data);

    runner.execute_transaction(TransactionTestCase {
        input: long_user
            .create_plain_message::<TestRuntime<S>, Derive<S>>(CallMessage::Match(match_msg)),
        assert: Box::new(move |result, _state| {
            assert!(result.tx_receipt.is_reverted());
        }),
    });
}

#[test]
fn test_match_succeeds_with_a_configured_authorizer() {
    let (test_data, mut runner) = setup();
    let long_user = &test_data.long_user;
    let match_msg = sample_match(&test_data);

    runner.execute_transaction(TransactionTestCase {
        input: long_user.create_plain_message::<TestRuntime<S>, Derive<S>>(
            CallMessage::SetAuthorizer(vec![0x00]),
        ),
        assert: Box::new(move |result, _state| {
            assert!(result.tx_receipt.is_successful());
        }),
    });

    runner.execute_transaction(TransactionTestCase {
        input: long_user
            .create_plain_message::<TestRuntime<S>, Derive<S>>(CallMessage::Match(match_msg)),
        assert: Box::new(move |result, _state| {
            assert!(result.tx_receipt.is_successful());
        }),
    });
}

#[test]
fn test_match_reverts_when_authorizer_attempts_a_state_write() {
    let (test_data, mut runner) = setup();
    let long_user = &test_data.long_user;
    let match_msg = sample_match(&test_data);

    runner.execute_transaction(TransactionTestCase {
        input: long_user.create_plain_message::<TestRuntime<S>, Derive<S>>(
            CallMessage::SetAuthorizer(vec![0x60, 0x01, 0x60, 0x00, 0x55]),
        ),
        assert: Box::new(move |result, _state| {
            assert!(result.tx_receipt.is_successful());
        }),
    });

    runner.execute_transaction(TransactionTestCase {
        input: long_user
            .create_plain_message::<TestRuntime<S>, Derive<S>>(CallMessage::Match(match_msg)),
        assert: Box::new(move |result, _state| {
            assert!(result.tx_receipt.is_reverted());
        }),
    });
}
