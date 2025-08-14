use std::process::Command;
use std::thread;
use std::time::Duration;

use acceptance_test::{cleanup_postgres_container, generate_postgres_password, interpolate_config, start_and_wait_for_postgres_ready, Directories, POSTGRES_CONTAINER_NAME};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use sov_api_spec::types::{self, AcceptTxBody, GetSlotByIdChildren, Slot};

use sov_api_spec::ResponseValue;
use tokio_stream::StreamExt;
use futures::stream::Stream;
use serde_json::Value;
use std::path::PathBuf;
use sov_modules_rollup_blueprint::RollupBlueprint;
use sov_modules_api::Spec as SpecT;
use sov_rollup_interface::node::ledger_api::IncludeChildren;
use sov_soak_testing::{run_generator_task_for_bank, run_generator_task_for_bank_and_synthetic_load, TxType, ValidityProfile};
use rollup_starter::rollup::StarterRollup;
use sov_bank::{get_token_id, Amount, CallMessage as BankCallMessage, Coins, TokenId};
use stf_starter::sov_modules_api::capabilities::UniquenessData;
use stf_starter::sov_modules_api::execution_mode::Native;
use stf_starter::sov_modules_api::macros::config_value;
use stf_starter::sov_modules_api::transaction::{PriorityFeeBips, Transaction, UnsignedTransaction};
use stf_starter::sov_modules_api::{CryptoSpec, RawTx};
use stf_starter::RuntimeCall;
use tokio::sync::watch::Receiver;

use tokio::signal::unix::SignalKind;
use tokio::task::JoinSet;
use tracing::info;

type Runtime = <StarterRollup<Native> as RollupBlueprint<Native>>::Runtime;
type Spec = <StarterRollup<Native> as RollupBlueprint<Native>>::Spec;


async fn worker_task(
    client: sov_api_spec::Client,
    rx: Receiver<bool>,
    worker_id: u128,
    num_workers: u32,
) -> anyhow::Result<()> {
	// TODO: Add synthetic load txs
    let result = run_generator_task_for_bank::<Runtime, Spec>(
        client,
        rx,
        worker_id,
        num_workers,
        ValidityProfile::Clean.get_validity(),
		// TxType::Mixed,
    )
    .await;

    if let Err(e) = result {
        tracing::error!("Worker task {worker_id} failed: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

fn get_rollup_client() -> Result<sov_api_spec::Client, anyhow::Error> {
    let reqwest_client = reqwest::ClientBuilder::new()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(120))
        .build()?;
    let client = sov_api_spec::Client::new_with_client(API_URL, reqwest_client);
    Ok(client)
}

fn start_workers() ->  Result<(tokio::sync::watch::Sender<bool>, JoinSet<Result<(), anyhow::Error>>), anyhow::Error> {
	const NUM_WORKERS: u32 = 20;
	let mut worker_set = JoinSet::new();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let client = get_rollup_client()?;

    for i in 0..NUM_WORKERS {
        worker_set.spawn(worker_task(
            client.clone(),
            rx.clone(),
            i as u128,
            NUM_WORKERS,
        ));
    }
	Ok((tx, worker_set))

}

const API_URL: &str = "http://localhost:12348";

fn assert_slots_match_excluding_batches(slot1: &Slot, slot2: &Slot, description: &str) {
    assert_eq!(slot1.batch_range, slot2.batch_range, "{}: batch_range should match", description);
    assert_eq!(slot1.finality_status, slot2.finality_status, "{}: finality_status should match", description);
    assert_eq!(slot1.hash, slot2.hash, "{}: hash should match", description);
    assert_eq!(slot1.number, slot2.number, "{}: number should match", description);
    assert_eq!(slot1.state_root, slot2.state_root, "{}: state_root should match", description);
    assert_eq!(slot1.timestamp, slot2.timestamp, "{}: timestamp should match", description);
    assert_eq!(slot1.type_, slot2.type_, "{}: type should match", description);
}

fn slot_to_json(slot: &Slot, exclude_batches: bool) -> Result<Value, anyhow::Error> {
    let mut json = serde_json::to_value(slot)?;
    if let Value::Object(ref mut map) = json {
        if exclude_batches {
            map.remove("batches");
        }
    }
    Ok(json)
}


fn assert_slots_match_json_excluding_batches(slot1: &Slot, slot2: &Slot, description: &str) -> Result<(), anyhow::Error> {
    let json1 = slot_to_json(slot1, true)?;
    let json2 = slot_to_json(slot2, true)?;
    
    if json1 != json2 {
        println!("❌ {} JSON mismatch:", description);
        println!("Slot 1: {}", serde_json::to_string_pretty(&json1)?);
        println!("Slot 2: {}", serde_json::to_string_pretty(&json2)?);
        anyhow::bail!("{}: JSON comparison failed", description);
    }
    Ok(())
}

fn compare_against_snapshot(slot: &Slot, snapshot_json: &str, description: &str, exclude_batches: bool) -> Result<(), anyhow::Error> {
    let slot_json = slot_to_json(slot, exclude_batches)?;
    let snapshot: Value = serde_json::from_str(snapshot_json)?;
    
    if slot_json != snapshot {
        println!("❌ {} snapshot mismatch:", description);
        println!("Actual: {}", serde_json::to_string_pretty(&slot_json)?);
        println!("Expected: {}", serde_json::to_string_pretty(&snapshot)?);
        anyhow::bail!("{}: Snapshot comparison failed", description);
    }
    Ok(())
}

fn save_slot_snapshot(slot: &Slot, output_dir: &PathBuf) -> Result<(), anyhow::Error> {
    let json = slot_to_json(slot, false)?;
    let snapshot_json = serde_json::to_string_pretty(&json)?;
    let filename = format!("slot_{:04}_with_children.json", slot.number);
    let filepath = output_dir.join(&filename);
    
    std::fs::write(&filepath, snapshot_json)?;
    
    Ok(())
}

fn validate_against_snapshot(slot: &Slot, output_dir: &PathBuf, description: &str) -> Result<(), anyhow::Error> {
    let filename = format!("slot_{:04}_with_children.json", slot.number);
    let filepath = output_dir.join(&filename);
    let snapshot_json = std::fs::read_to_string(&filepath)?;
    compare_against_snapshot(slot, &snapshot_json, description, false)
}

pub enum GetItemBehavior {
    SaveSnapshot,
    CheckAgainstSnapshot,
}

pub struct SlotMonitor {
    slots: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    slots_with_children: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    finalized_slots: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    finalized_slots_with_children: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    prev_slot_with_children: Option<Slot>,
    output_dir: PathBuf,
    expected_slot_number: Option<u64>,
}

impl SlotMonitor {
    pub async fn new(client: &sov_api_spec::Client, output_dir: PathBuf) -> Result<Self, anyhow::Error> {
        let finalized_slots = client.subscribe_finalized_slots().await?;
        let finalized_slots_with_children = client.subscribe_finalized_slots_with_children(IncludeChildren::new(true)).await?;
        let slots = client.subscribe_slots().await?;
        let slots_with_children = client.subscribe_slots_with_children(IncludeChildren::new(true)).await?;

        // Create snapshots directory if it doesn't exist
        let snapshots_dir = output_dir.join("snapshots");
        std::fs::create_dir_all(&snapshots_dir)?;

        Ok(Self {
            slots: Box::new(slots),
            slots_with_children: Box::new(slots_with_children),
            finalized_slots: Box::new(finalized_slots),
            finalized_slots_with_children: Box::new(finalized_slots_with_children),
            prev_slot_with_children: None,
            output_dir: snapshots_dir,
            expected_slot_number: None,
        })
    }

    pub async fn get_next_slot(&mut self, behavior: GetItemBehavior) -> Result<(Slot, Slot, Slot, Slot), anyhow::Error> {
        let next_slot = self.slots.next().await.unwrap().unwrap();
        let next_slot_with_children = self.slots_with_children.next().await.unwrap().unwrap();
        let finalized_next_slot = self.finalized_slots.next().await.unwrap().unwrap();
        let finalized_next_slot_with_children = self.finalized_slots_with_children.next().await.unwrap().unwrap();

        // Validate slot number sequence
        if let Some(expected) = self.expected_slot_number {
            if next_slot_with_children.number != expected {
                anyhow::bail!("Slot number out of sequence! Expected {}, got {}", expected, next_slot_with_children.number);
            }
        } else {
            // First slot - initialize the expected sequence
            self.expected_slot_number = Some(next_slot_with_children.number);
        }
        // Check that slots match (excluding batches field)
        assert_slots_match_excluding_batches(&next_slot, &next_slot_with_children, "Next slot");
        assert_slots_match_json_excluding_batches(&next_slot, &next_slot_with_children, "Next slot JSON")?;

        // Check that finalized_slots_with_children matches finalized_slots (excluding batches field)
        assert_slots_match_excluding_batches(&finalized_next_slot, &finalized_next_slot_with_children, "Finalized slot");
        assert_slots_match_json_excluding_batches(&finalized_next_slot, &finalized_next_slot_with_children, "Finalized slot JSON")?;

        // Check if this slot has been finalized and has batches
        if finalized_next_slot.batch_range.end != finalized_next_slot.batch_range.start {
            if let Some(ref prev_slot_with_children) = self.prev_slot_with_children {
                assert_slots_match_excluding_batches(&finalized_next_slot, prev_slot_with_children, "Next slot with children should match previous slot with children");
                assert_eq!(finalized_next_slot_with_children.batches, prev_slot_with_children.batches, "Previous slot with children should match newly finalized slot with children");
            }
        }

        // Save the next_slot_with_children snapshot
        match behavior {
            GetItemBehavior::SaveSnapshot => {
                save_slot_snapshot(&next_slot_with_children, &self.output_dir)?;
            }
            GetItemBehavior::CheckAgainstSnapshot => {
                validate_against_snapshot(&next_slot_with_children, &self.output_dir, "Next slot with children")?;
            }
        }

        self.prev_slot_with_children = Some(next_slot_with_children.clone());
        
        // Update expected slot number for next iteration
        self.expected_slot_number = Some(next_slot_with_children.number + 1);

        Ok((next_slot, next_slot_with_children, finalized_next_slot, finalized_next_slot_with_children))
    }

    pub fn save_slot_as_snapshot(&self, slot: &Slot) -> Result<String, anyhow::Error> {
        let json = slot_to_json(slot, false)?;
        Ok(serde_json::to_string_pretty(&json)?)
    }
}

pub struct SlotFetcher {
    client: sov_api_spec::Client,
    output_dir: PathBuf,
}

impl SlotFetcher {
    pub fn new(client: sov_api_spec::Client, output_dir: PathBuf) -> Self {
        // Create snapshots directory if it doesn't exist
        let snapshots_dir = output_dir.join("snapshots");
        std::fs::create_dir_all(&snapshots_dir).ok();
        
        Self { 
            client,
            output_dir: snapshots_dir,
        }
    }

    pub async fn fetch_and_compare_slot(&self, slot_number: u64, behavior: GetItemBehavior) -> Result<Slot, anyhow::Error> {
        // Fetch slot in all 4 possible ways
        let slot_with_children = self.client.get_slot_by_id(&types::IntOrHash::Integer(slot_number), Some(GetSlotByIdChildren::_1)).await?;
        let slot_without_children = self.client.get_slot_by_id(&types::IntOrHash::Integer(slot_number), Some(GetSlotByIdChildren::_0)).await?;
        let slot_by_hash = self.client.get_slot_by_id(&types::IntOrHash::Hash(slot_with_children.hash.clone()), None).await?;
        let slot_by_hash_with_children = self.client.get_slot_by_id(&types::IntOrHash::Hash(slot_with_children.hash.clone()), Some(GetSlotByIdChildren::_1)).await?;

        // Compare all variations for consistency
        self.compare_slot_variations(&slot_with_children, &slot_without_children, &slot_by_hash, &slot_by_hash_with_children, slot_number)?;

        // Handle snapshot behavior
        match behavior {
            GetItemBehavior::SaveSnapshot => {
                save_slot_snapshot(&slot_with_children, &self.output_dir)?;
            }
            GetItemBehavior::CheckAgainstSnapshot => {
                validate_against_snapshot(&slot_with_children, &self.output_dir, &format!("Fetched slot {}", slot_number))?;
            }
        }

        // Return the most complete version (with children)
        Ok(slot_with_children.into_inner())
    }

    fn compare_slot_variations(&self, slot_with_children: &Slot, slot_without_children: &Slot, slot_by_hash: &Slot, slot_by_hash_with_children: &Slot, slot_number: u64) -> Result<(), anyhow::Error> {
        let description_prefix = format!("Slot {}", slot_number);

        // Compare slots fetched by number vs by hash (excluding batches)
        assert_slots_match_excluding_batches(slot_with_children, slot_by_hash_with_children, &format!("{}: by number vs by hash (with children)", description_prefix));
        assert_eq!(slot_by_hash_with_children.batches, slot_with_children.batches, "{}: batches should match", description_prefix);
        assert_slots_match_excluding_batches(slot_without_children, slot_by_hash, &format!("{}: by number vs by hash (without children)", description_prefix));
        assert_slots_match_excluding_batches(slot_with_children, slot_without_children, &format!("{}: by hash vs by hash with children", description_prefix));


        // Compare the slots as JSON as well to be extra safe
        assert_slots_match_json_excluding_batches(slot_with_children, slot_by_hash_with_children, &format!("{}: JSON by number vs by hash (with children)", description_prefix))?;
        assert_slots_match_json_excluding_batches(slot_without_children, slot_by_hash, &format!("{}: JSON by number vs by hash (without children)", description_prefix))?;
        assert_slots_match_json_excluding_batches(slot_with_children, slot_without_children, &format!("{}: JSON with vs without children (by number)", description_prefix))?;

        Ok(())
    }
}


#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let directories = Directories::new()?;
	let password = generate_postgres_password()?;
	start_and_wait_for_postgres_ready(POSTGRES_CONTAINER_NAME, &password)?;
	interpolate_config(&password, &directories)?;

	info!("Starting rollup from rollup workspace root: {}", directories.rollup_root.display());
    let rollup: std::process::Child = Command::new("cargo")
        .args(["run", 
			"--release", 
			"--", 
			"--rollup-config-path", 
			&directories.output_dir.join("config.toml").display().to_string(),
			"--genesis-path",
			&directories.acceptance_test_dir.join("genesis.json").display().to_string(),
			"--stop-at-rollup-height",
			"100",
		])
        .current_dir(directories.rollup_root)
        .spawn()
        .expect("Failed to start rollup");
    info!("Rollup started, waiting for sequencer to be ready");
    // Wait up to two minutes for the sequencer to be ready
    for _ in 0..1200 {
        if let Ok(response) = reqwest::get(format!("{}/sequencer/ready", API_URL)).await {
            if response.status().is_success() {
                break;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    // Send the known good txs: Create token, mint token, transfer token
    let client = get_rollup_client()?;

    let mut slot_monitor = SlotMonitor::new(&client, directories.output_dir.clone()).await?;

    let mut sequencer_events = client.subscribe_to_events().await?;
    let mut sequencer_txs = client.subscribe_to_txs(None).await?;

    let [create_token, mint, transfer] = set_txs();

    let response = sign_and_send_tx(create_token, &client).await?;
    println!("Accepted tx: {:?}\n", response);
    println!("Sequencer tx: {:?}\n", sequencer_txs.next().await);
    for _ in 0..response.events.len() {
        println!("Event: {:?}", sequencer_events.next().await);
    }
    println!("\n\n");

    let mut first_subscribed_slot_number = 0;
    // Wait for the first batch to be posted 
    for i in 0..10 {
        let (next_slot, _next_slot_with_children, _finalized_next_slot, _finalized_next_slot_with_children) = slot_monitor.get_next_slot(GetItemBehavior::SaveSnapshot).await?;
        if i == 0 {
            first_subscribed_slot_number = next_slot.number;
        }

        if next_slot.batch_range.end != next_slot.batch_range.start {
            break;
        }
    }

    let response = sign_and_send_tx(mint, &client).await?;
    println!("Accepted tx: {:?}\n", response);
    println!("Sequencer tx: {:?}\n", sequencer_txs.next().await);
    for _ in 0..response.events.len() {
        println!("Event: {:?}", sequencer_events.next().await);
    }
    println!("\n\n");

    let response = sign_and_send_tx(transfer, &client).await?;
    println!("Accepted tx: {:?}\n", response);
    println!("Sequencer tx: {:?}\n", sequencer_txs.next().await);
    for _ in 0..response.events.len() {
        println!("Event: {:?}", sequencer_events.next().await);
    }
    println!("\n\n");

    // Wait for the next txs to post and be finalized. 
    for _ in 0..10 {
        let (_next_slot, _next_slot_with_children, _finalized_next_slot, finalized_next_slot_with_children) = slot_monitor.get_next_slot(GetItemBehavior::SaveSnapshot).await?;
        
        if finalized_next_slot_with_children.batches.len() > 0 {
            let batch = &finalized_next_slot_with_children.batches[0];
            if batch.txs.len() > 0 {
                break
            }
        }
    }

    let last_slot = slot_monitor.prev_slot_with_children.as_ref().unwrap();
    
    let slot_fetcher = SlotFetcher::new(client, directories.output_dir.clone());
    
    for slotnum in 0..first_subscribed_slot_number {
        let _slot = slot_fetcher.fetch_and_compare_slot(slotnum, GetItemBehavior::SaveSnapshot).await?;
    }
    for slotnum in first_subscribed_slot_number..=last_slot.number {
        let _slot = slot_fetcher.fetch_and_compare_slot(slotnum, GetItemBehavior::CheckAgainstSnapshot).await?;
    }

    

	// let (tx, worker_set) = start_workers()?;


    // let mut terminate = tokio::signal::unix::signal(SignalKind::terminate())
    //     .expect("Failed to set up SIGTERM handler");
    // let mut quit =
    //     tokio::signal::unix::signal(SignalKind::quit()).expect("Failed to set up SIGQUIT handler");
    // tokio::select! {
    //     _ = tokio::signal::ctrl_c() => tracing::info!("Received Ctrl+C"),
    //     _ = terminate.recv() => tracing::info!("Received SIGTERM"),
    //     _ = quit.recv() => tracing::info!("Received SIGQUIT"),
    // }

    // tx.send(true)?;
    // _ = worker_set.join_all();

    // Shutdown the rollup )
    info!("Sending SIGINT to rollup process");
    let mut interrupt = Command::new("kill")
        .args(["-s", "SIGINT", &rollup.id().to_string()])
        .spawn()?;
    interrupt.wait()?;
    let output = rollup.wait_with_output()?;
    info!("Rollup process finished");
    println!("{}", String::from_utf8(output.stdout)?);
	cleanup_postgres_container(POSTGRES_CONTAINER_NAME)?;

    Ok(())
}

fn encode_and_sign_tx(msg: RuntimeCall<Spec>) -> Result<RawTx, anyhow::Error> {
    let utx = UnsignedTransaction::<Runtime, Spec>::new(msg, config_value!("CHAIN_ID"), PriorityFeeBips(0), Amount::new(100_000_000), UniquenessData::Generation(0), None);
    let priv_key: <<Spec as SpecT>::CryptoSpec as CryptoSpec>::PrivateKey = serde_json::from_str("\"0d87c12ea7c12024b3f70a26d735874608f17c8bce2b48e6fe87389310191264\"").unwrap();

    let tx = Transaction::new_signed_tx(&priv_key, &<Runtime as sov_modules_stf_blueprint::Runtime<Spec>>::CHAIN_HASH, utx);
    let tx = RawTx::new(borsh::to_vec(&tx).unwrap());

    Ok(tx)
}

async fn sign_and_send_tx(msg: RuntimeCall<Spec>, client: &sov_api_spec::Client) -> Result<ResponseValue<types::TxInfoWithConfirmation>, anyhow::Error> {
    let tx = encode_and_sign_tx(msg)?;
    Ok(client.accept_tx(&AcceptTxBody {
        body: BASE64_STANDARD.encode(tx)
    }).await?)
}


fn set_txs() -> [RuntimeCall<Spec>; 3] {
    let msg1: RuntimeCall<Spec> = RuntimeCall::Bank(BankCallMessage::CreateToken { 
        token_name: "acceptance-test-token".try_into().unwrap(), 
        token_decimals: None, 
        initial_balance: Amount::new(1000), 
        mint_to_address: "0x9b08ce57a93751aE790698A2C9ebc76A78F23E25".parse().unwrap(), 
        admins: vec![
            "0x9b08ce57a93751aE790698A2C9ebc76A78F23E25".parse().unwrap()
        ].try_into().unwrap(), 
        supply_cap: None 
    });


    // Check balance and total supply (1000). Record block height as create_height
    // Wait for next block.

    // Send txs. Record block height
    let token_id = get_token_id::<Spec>("acceptance-test-token", None, &"0x9b08ce57a93751aE790698A2C9ebc76A78F23E25".parse::<<Spec as SpecT>::Address>().unwrap());
    let msg2: RuntimeCall<Spec> = RuntimeCall::Bank(BankCallMessage::Mint { 
        coins: Coins {
            amount: Amount::new(800),
            token_id,
        },
        mint_to_address: "0x9b08ce57a93751aE790698A2C9ebc76A78F23E25".parse().unwrap(), 
    });

    let msg3: RuntimeCall<Spec> = RuntimeCall::Bank(BankCallMessage::Transfer { 
        coins: Coins {
            amount: Amount::new(10),
            token_id,
        },
        to: "0x0000000000000000000000000000000000000000".parse().unwrap(), 
    });


    // loop {
    //     // Query live total supply (1800) and balance of 0x9b08ce57a93751aE790698A2C9ebc76A78F23E25 (1790)
    //     // Query live balance of 0x0000000000000000000000000000000000000000 (10)
    //     // Query historical balance of 0x0000000000000000000000000000000000000000 at H-1 (0)
    //     // Query historical supply at H-1 (1000)
    // }

    [msg1, msg2, msg3]
    
}
