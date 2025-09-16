use std::process::Command;

use acceptance_test::fetch_and_compare::{GetItemBehavior, SlotFetcher};
use acceptance_test::{
    cleanup_postgres_container, generate_postgres_password, get_rollup_client, interpolate_config,
    run_soak, start_and_wait_for_postgres_ready, wait_for_sequencer_ready, Directories, Runtime,
    Spec, API_URL, NUM_SOAK_BATCHES, POSTGRES_CONTAINER_NAME,
};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use sov_api_spec::types::{self, AcceptTxBody};

use acceptance_test::fetch_and_compare::SlotMonitor;
use sov_api_spec::ResponseValue;
use sov_bank::{get_token_id, Amount, CallMessage as BankCallMessage, Coins, TokenId};
use sov_modules_api::Spec as SpecT;
use stf_starter::sov_modules_api::capabilities::UniquenessData;
use stf_starter::sov_modules_api::macros::config_value;
use stf_starter::sov_modules_api::transaction::{
    PriorityFeeBips, Transaction, UnsignedTransaction,
};
use stf_starter::sov_modules_api::{CryptoSpec, RawTx};
use stf_starter::RuntimeCall;
use tokio_stream::StreamExt;

use tracing::info;

fn compare_tx_info_and_accepted_tx(
    tx_info: &types::TxInfoWithConfirmation,
    accepted_tx: &types::ApiAcceptedTx,
    description: &str,
) {
    // Compare shared fields
    assert_eq!(
        tx_info.events, accepted_tx.events,
        "{}: events should match",
        description
    );
    assert_eq!(
        tx_info.id, accepted_tx.id,
        "{}: id should match",
        description
    );
    assert_eq!(
        tx_info.tx_number,
        Some(accepted_tx.tx_number),
        "{}: tx_number should match",
        description
    );

    // TxInfoWithConfirmation has receipt wrapped in Option, ApiAcceptedTx has it directly
    if let Some(ref receipt) = tx_info.receipt {
        assert_eq!(
            receipt, &accepted_tx.receipt,
            "{}: receipt should match",
            description
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Initialize tracing subscriber with RUST_LOG environment variable, fallback to info
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let directories = Directories::new()?;
    let password = generate_postgres_password()?;
    start_and_wait_for_postgres_ready(POSTGRES_CONTAINER_NAME, &password)?;
    interpolate_config(&password, &directories)?;

    info!(
        "Starting rollup from rollup workspace root: {}",
        directories.rollup_root.display()
    );
    let rollup = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "--rollup-config-path",
            &directories
                .output_dir
                .join("config.toml")
                .display()
                .to_string(),
            "--genesis-path",
            &directories
                .acceptance_test_dir
                .join("genesis.json")
                .display()
                .to_string(),
            "--stop-at-rollup-height",
            &(NUM_SOAK_BATCHES + 10).to_string(),
        ])
        .current_dir(directories.rollup_root.clone())
        .env("RUST_LOG", "info")
        .stdout(std::fs::File::create(
            directories.output_dir.join("rollup.log"),
        )?)
        .spawn()
        .expect("Failed to start rollup");

    // First, run some manual setup. This creates and checks some very simple state with expensive consistency checks.
    do_manual_setup(directories.clone()).await?;
    let throughput_report = run_soak(directories.clone(), rollup, 3, true).await?;
    std::fs::write(
        directories.output_dir.join("throughput_report.json"),
        serde_json::to_string(&throughput_report)?,
    )?;
    save_mock_data(directories.clone())?;
    cleanup_postgres_container(POSTGRES_CONTAINER_NAME)?;
    Ok(())
}

/// Runs a sequence of two batches, one with a create token, and one with a mint and transfer.
/// Since we know exactly what state will be generated, we can make fine-grained assertions about the state using this manual setup.
async fn do_manual_setup(directories: Directories) -> Result<(), anyhow::Error> {
    info!("Rollup started, waiting for sequencer to be ready");
    wait_for_sequencer_ready().await?;
    info!("Sequencer is ready, sending txs");

    // Send the known good txs: Create token, mint token, transfer token
    let client = get_rollup_client()?;
    let http_client = reqwest::Client::new();

    let mut slot_monitor = SlotMonitor::new(&client, &directories).await?;

    let mut sequencer_events = client.subscribe_to_events().await?;
    let mut sequencer_txs = client.subscribe_to_txs(None).await?;

    let ([create_token, mint, transfer], token_id) = set_txs();
    let initial_supply = get_supply(&http_client, token_id).await?;
    assert_eq!(initial_supply, Amount::ZERO);

    // Create the token and check consistency between the sequencer and ledger
    let response = sign_and_send_tx(create_token, &client).await?;
    assert_eq!(response.events.len(), 1);
    assert_eq!(
        response.events[0],
        sequencer_events.next().await.unwrap().unwrap()
    );
    let accepted_tx = sequencer_txs.next().await.unwrap().unwrap();
    compare_tx_info_and_accepted_tx(&response, &accepted_tx, "Create token transaction");

    let new_supply = get_supply(&http_client, token_id).await?;
    assert_eq!(new_supply, Amount::new(1000));

    info!("First tx sent, waiting for first batch to be posted");
    let mut first_subscribed_slot_number = 0;
    let mut first_non_empty_slot_number = 0;
    // Wait for the first batch to be posted
    for i in 0..10 {
        let (
            next_slot,
            next_slot_with_children,
            _finalized_next_slot,
            _finalized_next_slot_with_children,
        ) = slot_monitor
            .get_next_slot(GetItemBehavior::SaveSnapshot)
            .await?;
        if i == 0 {
            first_subscribed_slot_number = next_slot.number;
        }

        if next_slot_with_children.batches.len() > 0 {
            let batch = &next_slot_with_children.batches[0];
            if batch.txs.len() > 0 {
                first_non_empty_slot_number = next_slot.number;
                assert_eq!(batch.txs[0].events.len(), 1);
                assert_eq!(batch.txs[0].events[0], response.events[0]);
                break;
            }
        }
    }
    info!("First batch posted, sending mint and transfer txs");
    let response = sign_and_send_tx(mint, &client).await?;
    assert_eq!(response.events.len(), 1);
    assert_eq!(
        response.events[0],
        sequencer_events.next().await.unwrap().unwrap()
    );
    let accepted_tx = sequencer_txs.next().await.unwrap().unwrap();
    compare_tx_info_and_accepted_tx(&response, &accepted_tx, "Mint transaction");
    let new_supply = get_supply(&http_client, token_id).await?;
    assert_eq!(new_supply, Amount::new(1800));

    let response = sign_and_send_tx(transfer, &client).await?;
    assert_eq!(response.events.len(), 1);
    assert_eq!(
        response.events[0],
        sequencer_events.next().await.unwrap().unwrap()
    );
    let accepted_tx = sequencer_txs.next().await.unwrap().unwrap();
    compare_tx_info_and_accepted_tx(&response, &accepted_tx, "Transfer transaction");
    let new_supply = get_supply(&http_client, token_id).await?;
    assert_eq!(new_supply, Amount::new(1800));

    info!("Mint and transfer txs sent, waiting for next batch to be posted");
    // Wait for the next txs to post and be finalized.
    let mut second_non_empty_slot_number = 0;
    for _ in 0..10 {
        let (
            _next_slot,
            _next_slot_with_children,
            _finalized_next_slot,
            finalized_next_slot_with_children,
        ) = slot_monitor
            .get_next_slot(GetItemBehavior::SaveSnapshot)
            .await?;

        if finalized_next_slot_with_children.batches.len() > 0 {
            let batch = &finalized_next_slot_with_children.batches[0];
            let last_tx = batch.txs.iter().find(|tx| tx.number == 2);
            if let Some(last_tx) = last_tx {
                assert_eq!(last_tx.events.len(), 1);
                assert_eq!(last_tx.events[0], response.events[0]);
                second_non_empty_slot_number = finalized_next_slot_with_children.number;
                break;
            }
        }
    }
    info!("Next batch posted, fetching and comparing slots");

    let last_slot = slot_monitor.prev_slot_with_children.as_ref().unwrap();
    let slot_fetcher = SlotFetcher::new(client, &directories);
    for slotnum in 0..first_subscribed_slot_number {
        let _slot = slot_fetcher
            .fetch_and_compare_slot(slotnum, GetItemBehavior::SaveSnapshot)
            .await?;
    }
    for slotnum in first_subscribed_slot_number..=last_slot.number {
        let _slot = slot_fetcher
            .fetch_and_compare_slot(slotnum, GetItemBehavior::CheckAgainstSnapshot)
            .await?;
    }

    for slot_num in 0..=last_slot.number {
        let supply = get_supply_archival(&http_client, token_id, Some(slot_num)).await?;
        if slot_num < first_non_empty_slot_number {
            assert_eq!(
                supply,
                Amount::ZERO,
                "Supply should be zero for slot {}. First non-empty slot was {}",
                slot_num,
                first_non_empty_slot_number
            );
        } else if slot_num < second_non_empty_slot_number {
            assert_eq!(
                supply,
                Amount::new(1000),
                "Supply should be 1000 for slot {}. First non-empty slot was {}. Last slot is {}",
                slot_num,
                first_non_empty_slot_number,
                second_non_empty_slot_number
            );
        } else {
            assert_eq!(
                supply,
                Amount::new(1800),
                "Supply should be 1800 for slot {}. second_non_empty_slot_number is {}",
                slot_num,
                second_non_empty_slot_number
            );
        }
    }
    info!("Manual setup complete");

    Ok(())
}

/// Rename the `mock_da.sqlite` files to `persistent_mock_da.sqlite` so that they can be used across runs.
/// We'll copy them back to `mock_da.sqlite` as part of the acceptance tests.
fn save_mock_data(directories: Directories) -> Result<(), anyhow::Error> {
    for input in ["mock_da.sqlite", "mock_da.sqlite-shm", "mock_da.sqlite-wal"] {
        let mut target = "persistent_".to_string();
        target.push_str(input);
        if let Err(err) = std::fs::rename(
            directories.output_dir.join(input),
            directories.output_dir.join(target),
        ) {
            if input == "mock_da.sqlite" {
                tracing::error!(
                    "Failed to rename {} for persistence accross runs: {}",
                    input,
                    err
                );
                return Err(anyhow::anyhow!("Failed to rename {}: {}", input, err));
            } else {
                tracing::warn!(
                    "Failed to rename {} for persistence accross runs: {}. Ignoring.",
                    input,
                    err
                );
            }
        }
    }
    Ok(())
}

fn encode_and_sign_tx(msg: RuntimeCall<Spec>) -> Result<RawTx, anyhow::Error> {
    let utx = UnsignedTransaction::<Runtime, Spec>::new(
        msg,
        config_value!("CHAIN_ID"),
        PriorityFeeBips(0),
        Amount::new(100_000_000),
        UniquenessData::Generation(0),
        None,
    );
    let priv_key: <<Spec as SpecT>::CryptoSpec as CryptoSpec>::PrivateKey = serde_json::from_str(
        "\"0d87c12ea7c12024b3f70a26d735874608f17c8bce2b48e6fe87389310191264\"",
    )
    .unwrap();

    let tx = Transaction::new_signed_tx(
        &priv_key,
        &<Runtime as sov_modules_stf_blueprint::Runtime<Spec>>::CHAIN_HASH,
        utx,
    );
    let tx = RawTx::new(borsh::to_vec(&tx).unwrap());

    Ok(tx)
}

async fn sign_and_send_tx(
    msg: RuntimeCall<Spec>,
    client: &sov_api_spec::Client,
) -> Result<ResponseValue<types::TxInfoWithConfirmation>, anyhow::Error> {
    let tx = encode_and_sign_tx(msg)?;
    Ok(client
        .accept_tx(&AcceptTxBody {
            body: BASE64_STANDARD.encode(tx),
        })
        .await?)
}

fn set_txs() -> ([RuntimeCall<Spec>; 3], TokenId) {
    let msg1: RuntimeCall<Spec> = RuntimeCall::Bank(BankCallMessage::CreateToken {
        token_name: "acceptance-test-token".try_into().unwrap(),
        token_decimals: None,
        initial_balance: Amount::new(1000),
        mint_to_address: "0x9b08ce57a93751aE790698A2C9ebc76A78F23E25"
            .parse()
            .unwrap(),
        admins: vec!["0x9b08ce57a93751aE790698A2C9ebc76A78F23E25"
            .parse()
            .unwrap()]
        .try_into()
        .unwrap(),
        supply_cap: None,
    });

    // Check balance and total supply (1000). Record block height as create_height
    // Wait for next block.

    // Send txs. Record block height
    let token_id = get_token_id::<Spec>(
        "acceptance-test-token",
        None,
        &"0x9b08ce57a93751aE790698A2C9ebc76A78F23E25"
            .parse::<<Spec as SpecT>::Address>()
            .unwrap(),
    );
    let msg2: RuntimeCall<Spec> = RuntimeCall::Bank(BankCallMessage::Mint {
        coins: Coins {
            amount: Amount::new(800),
            token_id,
        },
        mint_to_address: "0x9b08ce57a93751aE790698A2C9ebc76A78F23E25"
            .parse()
            .unwrap(),
    });

    let msg3: RuntimeCall<Spec> = RuntimeCall::Bank(BankCallMessage::Transfer {
        coins: Coins {
            amount: Amount::new(10),
            token_id,
        },
        to: "0x0000000000000000000000000000000000000000"
            .parse()
            .unwrap(),
    });

    ([msg1, msg2, msg3], token_id)
}

async fn get_supply(client: &reqwest::Client, token_id: TokenId) -> Result<Amount, anyhow::Error> {
    get_supply_archival(client, token_id, None).await
}

async fn get_supply_archival(
    client: &reqwest::Client,
    token_id: TokenId,
    slot_number: Option<u64>,
) -> Result<Amount, anyhow::Error> {
    let url = if let Some(slot_number) = slot_number {
        format!(
            "modules/bank/tokens/{}/total-supply?slot_number={}",
            token_id, slot_number
        )
    } else {
        format!("modules/bank/tokens/{}/total-supply", token_id)
    };
    let Some(supply) = get_from_base_url(client, &url).await? else {
        return Ok(Amount::ZERO);
    };
    let supply = supply["amount"]
        .as_str()
        .expect(&format!("Supply not found in {}", supply.to_string()));
    let supply = u128::from_str_radix(supply, 10)?;
    Ok(Amount::new(supply))
}

async fn get_from_base_url(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    let url = format!("{}/{}", API_URL, url);
    get(client, &url).await
}

async fn get(client: &reqwest::Client, url: &str) -> anyhow::Result<Option<serde_json::Value>> {
    let response = client.get(url).send().await?;
    if response.status().is_success() {
        Ok(Some(response.json::<serde_json::Value>().await?))
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        Ok(None)
    } else {
        return Err(anyhow::anyhow!("Failed to get {}", url));
    }
}
