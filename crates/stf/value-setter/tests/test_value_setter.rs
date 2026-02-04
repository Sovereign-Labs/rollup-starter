use sov_modules_api::Spec;
use sov_test_utils::{generate_optimistic_runtime, TestSpec};
use value_setter::ValueSetter;

type S = TestSpec;

// This macro creates a temporary runtime for testing.
generate_optimistic_runtime!(
    TestRuntime <=
    value_setter: ValueSetter<S>
);

use sov_test_utils::runtime::genesis::optimistic::HighLevelOptimisticGenesisConfig;
use sov_test_utils::runtime::TestRunner;
use sov_test_utils::TestUser;

// A helper struct to hold our test users, for convenience.
pub struct TestData<S: Spec> {
    pub regular_user: TestUser<S>,
}

pub fn setup() -> (TestData<S>, TestRunner<TestRuntime<S>, S>) {
    // Create a regular user.
    // (The `HighLevelOptimisticGenesisConfig` builder is a convenient way
    // to set up the initial state for core modules.)
    let genesis_config =
        HighLevelOptimisticGenesisConfig::generate().add_accounts_with_default_balance(1);

    let mut users = genesis_config.additional_accounts().to_vec();
    let regular_user = users.pop().unwrap();

    let test_data = TestData { regular_user };

    // Build the final genesis config by combining
    // the core config with our module's specific config.
    let genesis = GenesisConfig::from_minimal_config(genesis_config.into(), ());

    // Initialize the TestRunner with the genesis state.
    // The runner gives us a simple way to execute transactions and query state.
    let runner =
        TestRunner::new_with_genesis(genesis.into_genesis_params(), TestRuntime::default());

    (test_data, runner)
}

use sov_test_utils::{AsUser, TransactionTestCase};
use value_setter::CallMessage;

#[test]
fn test_can_set_value() {
    // 1. Setup
    let (test_data, mut runner) = setup();
    let regular_user = &test_data.regular_user;

    let new_value = 42;

    // 2. Execute the transaction
    runner.execute_transaction(TransactionTestCase {
        // The transaction input created by a regular user.
        input: regular_user.create_plain_message::<TestRuntime<S>, ValueSetter<S>>(
            CallMessage::SetValue(new_value),
        ),
        // The assertions to run after execution.
        assert: Box::new(move |result, state| {
            // 3. Assert the outcome
            assert!(result.tx_receipt.is_successful());

            // Assert that the state was updated correctly by querying the module.
            let value_setter = ValueSetter::<S>::default();
            let current_value = value_setter.value.get(state).unwrap();
            assert_eq!(current_value, Some(new_value));
        }),
    });
}
