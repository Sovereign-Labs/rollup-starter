use evm_soak::{
    evm_state_consistency_worker, load_state_consistency_contracts, pinned_worker_key,
    unpinned_worker_key,
};
use rand::distributions::Alphanumeric;
use rand::Rng;
use rollup_starter::rollup::StarterRollup;
use sov_api_spec::types::{self, GetSlotByIdChildren, Slot};
use sov_modules_api::execution_mode::Native;
use sov_modules_api::prelude::serde;
use sov_modules_rollup_blueprint::RollupBlueprint;
use sov_soak_testing_lib::{SoakTestRunner, ValidityProfile};
use state_consistency::state_validation_worker;
use std::path::PathBuf;
use std::{env, fs, process::Command, thread, time::Duration};
use tokio::sync::{oneshot, watch};
use tokio::task::JoinSet;
use tracing::{debug, info};

use crate::fetch_and_compare::{save_slot_snapshot, SlotFetcher};
mod evm_contracts;
pub mod evm_soak;
pub mod fetch_and_compare;
mod state_consistency;

pub const POSTGRES_CONTAINER_NAME: &str = "postgres-acceptance-test";
pub const API_URL: &str = "http://127.0.0.1:12348";
pub const API_ADDR: &str = "127.0.0.1:12348";
pub const SETUP_THROUGHPUT_FILE: &str = "acceptance_throughput.json";

// Save a full snapshot of the slot every N slots
const FULL_SLOT_SAVE_INTERVAL: u64 = 25;
pub const NUM_SOAK_BATCHES: u64 = 1000;
const ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
const ROLLUP_FORCED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

type RollupWaitResult = Result<std::process::ExitStatus, std::io::Error>;
type RollupJoinResult = Result<RollupWaitResult, tokio::task::JoinError>;
type RollupExitReceiver = oneshot::Receiver<RollupJoinResult>;

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
    pub throughput_dir: PathBuf,
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

        let throughput_dir = acceptance_test_dir.join("acceptance-throughput");
        // Only create throughput_dir if it doesn't exist - this directory persists across runs
        if !throughput_dir.exists() {
            fs::create_dir_all(&throughput_dir)?;
        }

        Ok(Self {
            rollup_root,
            acceptance_test_dir,
            output_dir,
            rollup_data_path,
            snapshots_dir,
            throughput_dir,
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
    // Wait up to a minute for the sequencer to be ready
    for _ in 0..600 {
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
    let runner = SoakTestRunner::<Runtime, Spec>::new()
        .with_bank()
        .with_state_consistency();
    runner
        .run(
            client,
            rx,
            worker_id,
            num_workers,
            ValidityProfile::Clean.get_validity(),
            None,
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

impl ThroughputReport {
    pub fn throughput(&self) -> f64 {
        self.num_txs as f64 / self.num_slots as f64
    }
}

pub fn build_rollup(root_dir: PathBuf) -> anyhow::Result<()> {
    let build_status = Command::new("cargo")
        .args(["build", "--release", "--features", "acceptance-testing"])
        .current_dir(root_dir)
        .status()
        .expect("Failed to execute cargo build");

    if build_status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to build rollup with exit code: {:?}",
            build_status.code()
        ))
    }
}

fn is_very_close_to_soak_test_end(num_soak_batches: u64) -> bool {
    num_soak_batches.saturating_add(15) > NUM_SOAK_BATCHES
}

/// Send SIGINT to the rollup process to gracefully shut it down.
/// If the process doesn't respond within 10 seconds, send SIGKILL.
pub fn kill_rollup(rollup_id: u32) {
    tracing::info!("Sending SIGINT to rollup process {}", rollup_id);

    // Send SIGINT
    if let Err(e) = Command::new("kill")
        .args(["-s", "SIGINT", &rollup_id.to_string()])
        .status()
    {
        tracing::error!("Failed to send SIGINT: {}", e);
        return;
    }

    // Wait up to 10 seconds for graceful shutdown
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(100));

        // Check if process still exists using kill -0
        match Command::new("kill")
            .args(["-0", &rollup_id.to_string()])
            .status()
        {
            Ok(status) if !status.success() => {
                // Process doesn't exist anymore
                tracing::info!("Rollup process {rollup_id} shut down gracefully");
                return;
            }
            Err(_) => {
                // Error running kill command, assume process is gone
                tracing::info!("Unable to check rollup process {rollup_id} status; assuming it no longer exists");
                return;
            }
            Ok(_) => {
                // Process still exists, continue waiting
            }
        }
    }

    // Process didn't respond to SIGINT, force kill
    tracing::warn!(
        "Rollup process {rollup_id} didn't respond to SIGINT after 10s, sending SIGKILL"
    );
    if let Err(e) = Command::new("kill")
        .args(["-9", &rollup_id.to_string()])
        .status()
    {
        tracing::error!("Failed to send SIGKILL: {e}");
    }
}

fn ensure_rollup_exit_result(rollup_result: RollupJoinResult) -> anyhow::Result<()> {
    match rollup_result {
        Ok(Ok(exit_status)) => {
            anyhow::ensure!(
                exit_status.success(),
                "Rollup process exited with non-zero status: {exit_status}"
            );
            Ok(())
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("Rollup process wait failed: {e}")),
        Err(e) => Err(anyhow::anyhow!("Rollup wait task failed: {e}")),
    }
}

async fn wait_for_rollup_exit_with_timeout(
    rollup_id: u32,
    rollup_rx: &mut RollupExitReceiver,
) -> anyhow::Result<()> {
    match tokio::time::timeout(ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT, &mut *rollup_rx).await {
        Ok(Ok(rollup_result)) => return ensure_rollup_exit_result(rollup_result),
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!(
                "Failed to receive rollup process result while waiting for graceful shutdown: {e}"
            ));
        }
        Err(_) => {
            tracing::warn!(
                "Timed out waiting {:?} for rollup process {} to exit gracefully. Sending shutdown signal.",
                ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT,
                rollup_id
            );
            kill_rollup(rollup_id);
        }
    }

    match tokio::time::timeout(ROLLUP_FORCED_SHUTDOWN_TIMEOUT, &mut *rollup_rx).await {
        Ok(Ok(rollup_result)) => ensure_rollup_exit_result(rollup_result),
        Ok(Err(e)) => Err(anyhow::anyhow!(
            "Failed to receive rollup process result after forcing shutdown: {e}"
        )),
        Err(_) => Err(anyhow::anyhow!(
            "Rollup process {} did not terminate within {:?} after forced shutdown",
            rollup_id,
            ROLLUP_FORCED_SHUTDOWN_TIMEOUT
        )),
    }
}

pub async fn run_soak(
    directories: Directories,
    mut rollup: std::process::Child,
    num_previous_batches: u64,
    rollup_stop_height: u64,
    save_slot_snapshots: bool,
) -> Result<ThroughputReport, anyhow::Error> {
    let (rollup_tx, mut rollup_rx) = oneshot::channel();
    let rollup_id = rollup.id();
    let mut rollup_exited = false;
    // Spawn background task to wait for rollup process
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || rollup.wait()).await;
        let _ = rollup_tx.send(result);
    });

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, &directories);
    slot_fetcher.subscribe_slots(false).await?;
    let (tx, mut worker_set) = start_workers(num_previous_batches as u32)?;

    // Start state validation worker
    let state_validator_client = get_rollup_client()?;
    let state_validator_rx = tx.subscribe();
    worker_set.spawn(state_validation_worker(
        state_validator_client,
        rollup_stop_height,
        state_validator_rx,
    ));

    let evm_contracts = load_state_consistency_contracts(&directories)?;
    for (idx, address) in evm_contracts.pinned.into_iter().enumerate() {
        let evm_worker_rx = tx.subscribe();
        let worker_key = pinned_worker_key(idx)?;
        worker_set.spawn(evm_state_consistency_worker(
            address,
            worker_key,
            "pinned",
            evm_worker_rx,
        ));
    }

    for (idx, address) in evm_contracts.unpinned.into_iter().enumerate() {
        let evm_worker_rx = tx.subscribe();
        let worker_key = unpinned_worker_key(idx)?;
        worker_set.spawn(evm_state_consistency_worker(
            address,
            worker_key,
            "unpinned",
            evm_worker_rx,
        ));
    }

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
                                if is_very_close_to_soak_test_end(num_soak_batches) {
                                    tracing::debug!("Soak slot fetcher encountered an error near the end of the test; num_soak_batches: {num_soak_batches}, NUM_SOAK_BATCHES: {NUM_SOAK_BATCHES}, slot number: {}, rollup_stop_height: {rollup_stop_height}", slot.number);
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
                kill_rollup(rollup_id);
                break;
            },
            _ = terminate.recv() => {
                tracing::info!("Received SIGTERM, shutting down rollup");
                kill_rollup(rollup_id);
                break;
            },
            _ = quit.recv() => {
                tracing::info!("Received SIGQUIT, shutting down rollup");
                kill_rollup(rollup_id);
                break;
            },
            // Rollup shutdown
            rollup_result = &mut rollup_rx => {
                match rollup_result {
                    Ok(rollup_result) => {
                        ensure_rollup_exit_result(rollup_result)?;
                        tracing::info!("Rollup process finished with successful status");
                        rollup_exited = true;
                    },
                    Err(e) => {
                        return Err(anyhow::anyhow!("Failed to receive rollup process result: {e}"));
                    },
                }
                break;
            }
            // Worker task failure
            Some(worker_result) = worker_set.join_next() => {
                match worker_result {
                    Ok(Ok(())) => {
                        // Worker completed successfully, continue monitoring
                    }
                    Ok(Err(e)) => {
                        if is_very_close_to_soak_test_end(num_soak_batches) {
                            tracing::debug!("Worker task failed near the end of the test; num_soak_batches: {num_soak_batches}, NUM_SOAK_BATCHES: {NUM_SOAK_BATCHES}, rollup_stop_height: {rollup_stop_height}, err: {e}");
                            tracing::warn!("Worker task failed very near the end of the test. Assuming the rollup shut down.");
                        } else {
                            tracing::error!("Worker task failed: {}", e);
                            kill_rollup(rollup_id);
                            return Err(e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Worker task panicked: {}", e);
                        kill_rollup(rollup_id);
                        return Err(e.into());
                    }
                }
            }
        }
    }

    if tx.send(true).is_err() {
        debug!("Soak worker channel closed; workers already shut down");
    }
    let worker_errors: Vec<_> = worker_set
        .join_all()
        .await
        .into_iter()
        .filter_map(Result::err)
        .collect();
    if !worker_errors.is_empty() {
        for (idx, err) in worker_errors.iter().enumerate() {
            tracing::warn!(
                "Ignoring worker task failure during shutdown ({}): {}",
                idx + 1,
                err
            );
        }
    }

    // Make sure the rollup process has fully exited before allowing postgres cleanup.
    if !rollup_exited {
        wait_for_rollup_exit_with_timeout(rollup_id, &mut rollup_rx).await?;
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
