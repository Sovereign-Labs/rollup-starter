use super::test_helpers::{read_private_keys, start_rollup};
use anyhow::Context;
use futures::StreamExt;
use sov_address::{EthereumAddress, EvmCryptoSpec};
use sov_cli::NodeClient;
use sov_mock_da::{BlockProducingConfig, MockAddress, MockDaConfig, MockDaSpec};
use sov_mock_zkvm::MockZkvm;
use sov_modules_api::capabilities::UniquenessData;
use sov_modules_api::configurable_spec::ConfigurableSpec;
use sov_modules_api::execution_mode::Native;
use sov_modules_api::macros::config_value;
use sov_modules_api::transaction::{PriorityFeeBips, Transaction, UnsignedTransaction};
use sov_modules_api::{Amount, Spec};
use sov_modules_rollup_blueprint::logging::default_rust_log_value;
use sov_rollup_interface::common::SafeVec;
use sov_rollup_interface::da::DaSpec;
use sov_rollup_interface::zk::CryptoSpec;
use sov_state::nomt::prover_storage::NomtProverStorage;
use sov_state::DefaultStorageSpec;
use std::env;
use std::str::FromStr;
use stf_starter::Runtime;
use stf_starter::RuntimeCall;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

const TOKEN_NAME: &str = "sov-token";
const TOKEN_DECIMALS: u8 = 6;
const MAX_TX_FEE: Amount = Amount::new(100_000_000);

type Hasher = <EvmCryptoSpec as CryptoSpec>::Hasher;
type NomtStorage = NomtProverStorage<DefaultStorageSpec<Hasher>, <MockDaSpec as DaSpec>::SlotHash>;
type TestSpec = ConfigurableSpec<
    MockDaSpec,
    MockZkvm,
    MockZkvm,
    EthereumAddress,
    Native,
    EvmCryptoSpec,
    NomtStorage,
>;

#[tokio::test(flavor = "multi_thread")]
async fn bank_tx_tests() -> Result<(), anyhow::Error> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_str(
            &env::var("RUST_LOG").unwrap_or_else(|_| default_rust_log_value().to_string()),
        )?)
        .init();
    let (rest_port_tx, rest_port_rx) = tokio::sync::oneshot::channel();

    let rollup_task = tokio::spawn(async {
        start_rollup(
            rest_port_tx,
            std::path::PathBuf::from_str("../../configs/mock/genesis.json")
                .expect("Failed to build genesis config path"),
            None,
            MockDaConfig {
                connection_string: MockDaConfig::sqlite_in_memory(),
                sender_address: MockAddress::new([0; 32]),
                finalization_blocks: 3,
                block_producing: BlockProducingConfig::Periodic { block_time_ms: 300 },
                da_layer: None,
                randomization: None,
            },
        )
        .await;
    });
    let rest_port = rest_port_rx.await?.port();
    let client = NodeClient::new_at_localhost(rest_port).await?;

    // If the rollup throws an error, return it and stop trying to send the transaction
    tokio::select! {
        err = rollup_task => err?,
        res = send_test_create_token_tx(&client) => res?,
    }
    Ok(())
}

async fn send_test_create_token_tx(client: &NodeClient) -> Result<(), anyhow::Error> {
    let key_and_address = read_private_keys::<TestSpec>("tx_signer_private_key.json");
    let key = key_and_address.private_key;
    let user_address: <TestSpec as Spec>::Address = key_and_address.address;

    let token_id =
        sov_bank::get_token_id::<TestSpec>(TOKEN_NAME, Some(TOKEN_DECIMALS), &user_address);
    let initial_balance = Amount::new(1000);

    let msg = RuntimeCall::<TestSpec>::Bank(sov_bank::CallMessage::<TestSpec>::CreateToken {
        token_name: TOKEN_NAME.try_into().unwrap(),
        token_decimals: Some(TOKEN_DECIMALS),
        initial_balance,
        mint_to_address: user_address,
        admins: SafeVec::default(),
        supply_cap: None,
    });
    let chain_id = config_value!("CHAIN_ID");
    let generation = 0;
    let max_priority_fee = PriorityFeeBips::ZERO;
    let gas_limit = None;
    let tx = Transaction::<Runtime<TestSpec>, TestSpec>::new_signed_tx(
        &key,
        &<Runtime<TestSpec> as sov_modules_stf_blueprint::Runtime<TestSpec>>::CHAIN_HASH,
        UnsignedTransaction::new(
            msg,
            chain_id,
            max_priority_fee,
            MAX_TX_FEE,
            UniquenessData::Generation(generation),
            gas_limit,
        ),
    );

    let mut slot_subscription = client
        .client
        .subscribe_slots()
        .await
        .context("Failed to subscribe to slots!")?;

    // Wait till rollup is ready
    let _slot_number = slot_subscription
        .next()
        .await
        .transpose()?
        .map(|slot| slot.number)
        .unwrap_or_default();

    client.client.send_txs_to_sequencer(&[tx]).await?;

    // Wait until the rollup has processed the next slot
    let _slot_number = slot_subscription
        .next()
        .await
        .transpose()?
        .map(|slot| slot.number)
        .unwrap_or_default();

    let balance = client
        .get_balance::<TestSpec>(&user_address, &token_id, None)
        .await?;
    assert_eq!(initial_balance, balance);

    Ok(())
}
