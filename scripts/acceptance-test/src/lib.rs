use rand::distributions::Alphanumeric;
use rand::Rng;
use rollup_starter::rollup::StarterRollup;
use sov_api_spec::types::{self, GetSlotByIdChildren, Slot};
use sov_modules_api::execution_mode::Native;
use sov_modules_api::prelude::serde;
use sov_modules_rollup_blueprint::RollupBlueprint;
use sov_soak_testing_lib::{SoakTestRunner, ValidityProfile};
use std::path::PathBuf;
use std::{env, fs, process::Command, thread, time::Duration};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{debug, info};

use crate::fetch_and_compare::{save_slot_snapshot, SlotFetcher};
pub mod fetch_and_compare;

pub const POSTGRES_CONTAINER_NAME: &str = "postgres-acceptance-test";
pub const API_URL: &str = "http://localhost:12348";

// Save a full snapshot of the slot every N slots
const FULL_SLOT_SAVE_INTERVAL: u64 = 25;
pub const NUM_SOAK_BATCHES: u64 = 1000;

pub type Runtime = <StarterRollup<Native> as RollupBlueprint<Native>>::Runtime;
pub type Spec = <StarterRollup<Native> as RollupBlueprint<Native>>::Spec;

pub fn start_and_wait_for_postgres_ready(
    container_name: &str,
    password: &str,
) -> Result<(), anyhow::Error> {
    info!("Starting postgres container");
    let postgres_env = format!("POSTGRES_PASSWORD={}", password);
    let start_postgres = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            "postgres-acceptance-test",
            "-e",
            &postgres_env,
            "-p",
            "5432:5432",
            "postgres",
        ])
        .output()?;
    assert!(
        start_postgres.status.success(),
        "Failed to start postgres container"
    );

    info!("Waiting for postgres to be ready");
    let max_attempts = 30; // 30 seconds max

    for attempt in 0..max_attempts {
        let ready_check = Command::new("docker")
            .args(["exec", container_name, "pg_isready", "-U", "postgres"])
            .output()?;

        if ready_check.status.success() {
            info!("Postgres is ready");
            return Ok(());
        }

        debug!(
            "Postgres not ready yet, waiting... (attempt {}/{})",
            attempt, max_attempts
        );
        thread::sleep(Duration::from_secs(1));
    }
    Err(anyhow::anyhow!(
        "Postgres failed to become ready after {} seconds",
        max_attempts
    ))
}

pub fn cleanup_postgres_container(container_name: &str) -> Result<(), anyhow::Error> {
    // Cleanup postgres before returning
    info!("Cleaning up postgres container");
    let end_postgres = Command::new("docker")
        .args(["stop", container_name])
        .output()?;
    anyhow::ensure!(
        end_postgres.status.success(),
        "Failed to stop postgres container"
    );
    let remove_postgres = Command::new("docker")
        .args(["rm", "-f", container_name])
        .output()?;
    anyhow::ensure!(
        remove_postgres.status.success(),
        "Failed to remove postgres container"
    );
    Ok(())
}

pub fn generate_postgres_password() -> Result<String, anyhow::Error> {
    let password = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    Ok(password)
}

#[derive(Debug, Clone)]
pub struct Directories {
    pub rollup_root: PathBuf,
    pub acceptance_test_dir: PathBuf,
    pub output_dir: PathBuf,
    pub rollup_data_path: PathBuf,
    pub snapshots_dir: PathBuf,
}

impl Directories {
    pub fn new() -> Result<Self, anyhow::Error> {
        let acceptance_test_dir = env::var("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));

        let rollup_root = acceptance_test_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let output_dir = acceptance_test_dir.join("acceptance-test-data");
        fs::create_dir_all(&output_dir)?;
        let rollup_data_path = output_dir.join("rollup-starter-data");
        fs::create_dir_all(&rollup_data_path)?;

        let snapshots_dir = output_dir.join("snapshots");
        std::fs::create_dir_all(&snapshots_dir).ok();

        Ok(Self {
            rollup_root,
            acceptance_test_dir,
            output_dir,
            rollup_data_path,
            snapshots_dir,
        })
    }
}

pub fn interpolate_config(password: &str, directories: &Directories) -> Result<(), anyhow::Error> {
    // Read and interpolate config file
    let config_path = directories.acceptance_test_dir.join("rollup_config.toml");
    info!("Reading config from: {}", config_path.display());
    let config_content = fs::read_to_string(config_path)?;

    // Make sqlite path absolute
    let sqlite_path = directories.output_dir.join("mock_da.sqlite");
    let sqlite_connection_string = format!("sqlite://{}?mode=rwc", sqlite_path.display());

    let interpolated_config = config_content
        .replace("{password}", &password)
        .replace("{sqlite_connection_string}", &sqlite_connection_string)
        .replace(
            "{rollup_data_path}",
            &directories.rollup_data_path.display().to_string(),
        );

    // Write interpolated config to new file
    let output_path = directories.output_dir.join("config.toml");
    info!("Writing interpolated config to: {}", output_path.display());
    fs::write(output_path, interpolated_config)?;
    Ok(())
}

pub fn get_rollup_client() -> Result<sov_api_spec::Client, anyhow::Error> {
    let reqwest_client = reqwest::ClientBuilder::new()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(120))
        .build()?;
    let client = sov_api_spec::Client::new_with_client(API_URL, reqwest_client);
    Ok(client)
}

pub async fn wait_for_sequencer_ready() -> Result<(), anyhow::Error> {
    // Wait up to two minutes for the sequencer to be ready
    for _ in 0..1200 {
        if let Ok(response) = reqwest::get(format!("{}/sequencer/ready", API_URL)).await {
            if response.status().is_success() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

async fn worker_task(
    client: sov_api_spec::Client,
    rx: watch::Receiver<bool>,
    worker_id: u128,
    num_workers: u32,
) -> anyhow::Result<()> {
    // TODO: Add synthetic load txs
    let runner = SoakTestRunner::<Runtime, Spec>::new().with_bank();
    runner
        .run(
            client,
            rx,
            worker_id,
            num_workers,
            ValidityProfile::Clean.get_validity(),
        )
        .await
}

fn start_workers(
    salt: u32,
) -> Result<
    (
        tokio::sync::watch::Sender<bool>,
        JoinSet<Result<(), anyhow::Error>>,
    ),
    anyhow::Error,
> {
    tracing::info!("Starting {} workers", NUM_WORKERS);
    const NUM_WORKERS: u32 = 20;
    let mut worker_set = JoinSet::new();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let client = get_rollup_client()?;

    for i in 0..NUM_WORKERS {
        worker_set.spawn(worker_task(
            client.clone(),
            rx.clone(),
            (i + salt) as u128,
            NUM_WORKERS,
        ));
    }
    Ok((tx, worker_set))
}

fn save_slot_snapshot_if_needed(
    slot: &Slot,
    directories: &Directories,
    save_slot_snapshots: bool,
) -> Result<(), anyhow::Error> {
    if save_slot_snapshots {
        save_slot_snapshot(slot, &directories.snapshots_dir)?;
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThroughputReport {
    pub num_txs: u64,
    pub num_slots: u64,
}

pub async fn run_soak(
    directories: Directories,
    mut rollup: std::process::Child,
    num_previous_batches: u64,
    save_slot_snapshots: bool,
) -> Result<ThroughputReport, anyhow::Error> {
    let (rollup_tx, mut rollup_rx) = tokio::sync::oneshot::channel();
    let rollup_id = rollup.id();
    // Spawn background task to wait for rollup process
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || rollup.wait()).await;
        let _ = rollup_tx.send(result);
    });

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, &directories);
    slot_fetcher.subscribe_slots(false).await?;
    let (tx, worker_set) = start_workers(num_previous_batches as u32)?;

    use tokio::signal::unix::SignalKind;
    let mut terminate = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("Failed to set up SIGTERM handler");
    let mut quit =
        tokio::signal::unix::signal(SignalKind::quit()).expect("Failed to set up SIGQUIT handler");
    let client = get_rollup_client()?;

    tracing::info!("Workers started. Listening for slots");
    let mut num_soak_txs = 0;
    let mut num_soak_slots = 0;
    let mut num_soak_batches = 0;
    let num_previous_txs = slot_fetcher
        .fetch_batch_without_children(num_previous_batches)
        .await
        .expect("Failed to fetch previous batch")
        .tx_range
        .end;

    loop {
        tokio::select! {
            // On each slot, we update our counters and save a snapshot of the slot.
            // Every N slots, we save a full snapshot of the slot. (This is much more expensive, but also allows more thorough checks)
            new_slot = slot_fetcher.next_slot() => {

                if let Some(slot) = new_slot? {
                    // Get the latest tx number after the slot
                    if slot.batch_range.start != slot.batch_range.end {
                        let batch_num = slot.batch_range.end - 1;
                        match slot_fetcher.fetch_batch_without_children(batch_num).await {
                            Ok(batch) => {
                                num_soak_txs = batch.tx_range.end.saturating_sub(num_previous_txs);
                                // If the slot contains a batch (checked above) and we're into new batches, increment the counter
                                if slot.batch_range.end > num_previous_batches {
                                    num_soak_batches += 1;
                                }
                            }
                            Err(e) => {
                                // If we're very close to the end of the test, the rollup might have shut down before we could finish querying.
                                // The test shouldn't fail for this reason, so we just skip the batch.
                                if num_soak_batches + 15 > NUM_SOAK_BATCHES {
                                    tracing::warn!("Encountered an error very near the end of the test. Assuming the rollup shut down.");
                                    break;
                                } else {
                                    anyhow::bail!("Failed to fetch batch {}: {}", batch_num, e);
                                }
                            }
                        }
                    }
                    // If we haven't started processing any txs yet skip the rest of the loop. Don't forget to save the slot snapshot before we do though!
                    if num_soak_batches == 0 {
                        save_slot_snapshot_if_needed(&slot, &directories, save_slot_snapshots)?;
                        continue;
                    }

                    // Otherwise, we need to do some accounting
                    num_soak_slots += 1;
                    info!("Received new slot. Rollup has processed {} txs in {} slots. Average throughput: {} txs/slot", num_soak_txs, num_soak_slots, num_soak_txs as f64 / num_soak_slots as f64);
                    // Every N slots, we save a full snapshot of the slot. (This is much more expensive, but also allows more thorough checks)
                    if num_soak_slots % FULL_SLOT_SAVE_INTERVAL == 0 {
                       match client.get_slot_by_id(&types::IntOrHash::Integer(slot.number), Some(GetSlotByIdChildren::_1)).await {
                            Ok(full_slot) => {
                                save_slot_snapshot_if_needed(&full_slot, &directories, save_slot_snapshots)?;
                            }
                            Err(e) => {
                                tracing::error!("Failed to fetch full slot {}: {}.", slot.number, e);
                                save_slot_snapshot_if_needed(&slot, &directories, save_slot_snapshots)?;
                            }
                        }
                    } else {
                        save_slot_snapshot_if_needed(&slot, &directories, save_slot_snapshots)?;
                    }
                }
            }
            // Signal handlers
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C, shutting down rollup");
                // Shutdown the rollup immediately
                if let Ok(mut interrupt) = Command::new("kill")
                    .args(["-s", "SIGINT", &rollup_id.to_string()])
                    .spawn() {
                    let _ = interrupt.wait();
                }
                break;
            },
            _ = terminate.recv() => {
                tracing::info!("Received SIGTERM, shutting down rollup");
                // Shutdown the rollup immediately
                if let Ok(mut interrupt) = Command::new("kill")
                    .args(["-s", "SIGINT", &rollup_id.to_string()])
                    .spawn() {
                    let _ = interrupt.wait();
                }
                break;
            },
            _ = quit.recv() => {
                tracing::info!("Received SIGQUIT, shutting down rollup");
                // Shutdown the rollup immediately
                if let Ok(mut interrupt) = Command::new("kill")
                    .args(["-s", "SIGINT", &rollup_id.to_string()])
                    .spawn() {
                    let _ = interrupt.wait();
                }
                break;
            },
            // Rollup shutdown
            rollup_result = &mut rollup_rx => {
                match rollup_result {
                    Ok(Ok(exit_status)) => {
                        tracing::info!("Rollup process finished with status: {:?}", exit_status);
                    },
                    Ok(Err(e)) => {
                        tracing::error!("Rollup process failed: {}", e);
                    },
                    Err(_) => {
                        tracing::error!("Failed to receive rollup process result");
                    }
                }
                break;
            }
        }
    }

    tx.send(true)?;
    _ = worker_set.join_all();

    // Wait for rollup to finish if it hasn't already
    if let Ok(rollup_result) = rollup_rx.try_recv() {
        match rollup_result {
            Ok(_) => info!("Rollup process finished successfully"),
            Err(e) => {
                tracing::error!("Rollup process failed: {}", e);
                panic!("Rollup process failed");
            }
        }
    }
    info!(
        "Rollup process finished. Processed {} txs in  {} slots. Average throughput: {} txs/slot",
        num_soak_txs,
        num_soak_slots,
        num_soak_txs as f64 / num_soak_slots as f64
    );
    Ok(ThroughputReport {
        num_txs: num_soak_txs,
        num_slots: num_soak_slots,
    })
}
