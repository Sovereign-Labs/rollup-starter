use anyhow::{anyhow, Context};
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
use sov_soak_manager::{run_soak_coordinator, SoakManagerConfig};
use state_consistency::state_validation_worker;
use std::future::Future;
use std::path::PathBuf;
use std::{
    env, fs,
    process::{Child, Command, ExitStatus, Output},
    thread,
    time::Duration,
};
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;
use tokio::task::JoinSet;
use tracing::{debug, info};

use crate::fetch_and_compare::{save_slot_snapshot, SlotFetcher};
mod evm_contracts;
pub mod evm_soak;
pub mod fetch_and_compare;
mod state_consistency;
mod versioned_setup;
pub use versioned_setup::{
    extend_last_stop_height, prepare_acceptance_run_plan, spawn_rollup_manager,
    write_manager_config, AcceptanceRunPlan,
};

pub const POSTGRES_CONTAINER_NAME: &str = "postgres-acceptance-test";
pub const API_URL: &str = "http://127.0.0.1:12348";
pub const API_ADDR: &str = "127.0.0.1:12348";
pub const SETUP_THROUGHPUT_FILE: &str = "acceptance_throughput.json";

// Save a full snapshot of the slot every N slots
const FULL_SLOT_SAVE_INTERVAL: u64 = 25;
// Run each version of a multi-version rollup for this many blocks.
pub const BLOCKS_PER_VERSION: u64 = 1000;
const ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
const ROLLUP_FORCED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);
const SOAK_COORDINATOR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

type RollupWaitResult = Result<std::process::ExitStatus, std::io::Error>;
type RollupJoinResult = Result<RollupWaitResult, tokio::task::JoinError>;
type RollupExitReceiver = oneshot::Receiver<RollupJoinResult>;
type SoakJoinResult = Result<(), sov_soak_manager::SoakManagerError>;

pub type Runtime = <StarterRollup<Native> as RollupBlueprint<Native>>::Runtime;
pub type Spec = <StarterRollup<Native> as RollupBlueprint<Native>>::Spec;

#[derive(Debug)]
pub struct PostgresContainerGuard {
    container_name: String,
    cleaned_up: bool,
}

impl PostgresContainerGuard {
    pub fn start(container_name: &str, password: &str) -> Result<Self, anyhow::Error> {
        start_and_wait_for_postgres_ready(container_name, password)?;
        Ok(Self {
            container_name: container_name.to_owned(),
            cleaned_up: false,
        })
    }

    pub fn cleanup(&mut self) -> Result<(), anyhow::Error> {
        if self.cleaned_up {
            return Ok(());
        }
        cleanup_postgres_container(&self.container_name)?;
        self.cleaned_up = true;
        Ok(())
    }
}

impl Drop for PostgresContainerGuard {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            tracing::warn!(
                container = %self.container_name,
                "Failed to cleanup postgres container during drop: {e}"
            );
        }
    }
}

#[derive(Debug)]
struct RollupCleanupGuard {
    rollup_id: u32,
    exited: bool,
}

impl RollupCleanupGuard {
    fn new(rollup_id: u32) -> Self {
        Self {
            rollup_id,
            exited: false,
        }
    }

    fn rollup_id(&self) -> u32 {
        self.rollup_id
    }

    fn request_shutdown(&self) {
        if !self.exited {
            kill_rollup(self.rollup_id);
        }
    }

    fn mark_exited(&mut self) {
        self.exited = true;
    }
}

impl Drop for RollupCleanupGuard {
    fn drop(&mut self) {
        if !self.exited {
            kill_rollup(self.rollup_id);
        }
    }
}

#[derive(Debug)]
struct SoakCoordinatorGuard {
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<SoakJoinResult>>,
}

impl SoakCoordinatorGuard {
    fn new(shutdown_tx: oneshot::Sender<()>, handle: JoinHandle<SoakJoinResult>) -> Self {
        Self {
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    fn request_shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    async fn shutdown_and_wait(&mut self, timeout: Duration) -> Result<(), anyhow::Error> {
        self.request_shutdown();
        let Some(mut handle) = self.handle.take() else {
            return Ok(());
        };

        match tokio::time::timeout(timeout, &mut handle).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(e))) => Err(anyhow!("soak coordinator failed: {e}")),
            Ok(Err(e)) => Err(anyhow!("soak coordinator task panicked: {e}")),
            Err(_) => {
                tracing::warn!(
                    "Timed out waiting {:?} for soak coordinator shutdown; aborting task",
                    timeout
                );
                handle.abort();
                Ok(())
            }
        }
    }
}

impl Drop for SoakCoordinatorGuard {
    fn drop(&mut self) {
        self.request_shutdown();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

pub fn start_and_wait_for_postgres_ready(
    container_name: &str,
    password: &str,
) -> Result<(), anyhow::Error> {
    // Remove any stale container from interrupted prior runs.
    cleanup_postgres_container(container_name)?;

    info!("Starting postgres container");
    let postgres_env = format!("POSTGRES_PASSWORD={}", password);
    let start_postgres = docker_output(&[
        "run",
        "-d",
        "--name",
        container_name,
        "-e",
        &postgres_env,
        "-p",
        "5432:5432",
        "postgres",
    ])?;
    anyhow::ensure!(
        start_postgres.status.success(),
        "Failed to start postgres container {container_name}: {}",
        String::from_utf8_lossy(&start_postgres.stderr)
    );

    info!("Waiting for postgres to be ready");
    let max_attempts = 30; // 30 seconds max

    for attempt in 0..max_attempts {
        let ready_check = docker_output(&["exec", container_name, "pg_isready", "-U", "postgres"])?;

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

    let _ = cleanup_postgres_container(container_name);
    Err(anyhow!(
        "Postgres failed to become ready after {} seconds",
        max_attempts
    ))
}

pub fn cleanup_postgres_container(container_name: &str) -> Result<(), anyhow::Error> {
    info!("Cleaning up postgres container");
    let remove_postgres = docker_output(&["rm", "-f", container_name])?;
    if !remove_postgres.status.success() {
        let stderr = String::from_utf8_lossy(&remove_postgres.stderr);
        if !stderr.contains("No such container") {
            anyhow::bail!("Failed to remove postgres container {container_name}: {stderr}");
        }
    }
    Ok(())
}

fn docker_output(args: &[&str]) -> Result<Output, anyhow::Error> {
    Command::new("docker")
        .args(args)
        .output()
        .with_context(|| format!("failed to run docker {}", args.join(" ")))
}

pub async fn run_until_shutdown_signal<T, Fut>(fut: Fut) -> Result<T, anyhow::Error>
where
    Fut: Future<Output = Result<T, anyhow::Error>>,
{
    use tokio::signal::unix::{signal, SignalKind};

    let mut terminate =
        signal(SignalKind::terminate()).context("failed to register SIGTERM handler")?;
    let mut quit = signal(SignalKind::quit()).context("failed to register SIGQUIT handler")?;

    tokio::select! {
        result = fut => result,
        _ = tokio::signal::ctrl_c() => Err(anyhow!("Received Ctrl+C (SIGINT), shutting down")),
        _ = terminate.recv() => Err(anyhow!("Received SIGTERM, shutting down")),
        _ = quit.recv() => Err(anyhow!("Received SIGQUIT, shutting down")),
    }
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
    pub rollup_build_cache_dir: PathBuf,
    pub manager_build_dir: PathBuf,
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

        let rollup_build_cache_dir = acceptance_test_dir.join("rollup-build-cache");
        fs::create_dir_all(&rollup_build_cache_dir)?;
        let manager_build_dir = acceptance_test_dir.join("rollup-manager-build");

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
            rollup_build_cache_dir,
            manager_build_dir,
            output_dir,
            rollup_data_path,
            snapshots_dir,
            throughput_dir,
        })
    }

    pub fn set_rollup_build_cache_dir(&mut self, path: PathBuf) -> Result<(), anyhow::Error> {
        fs::create_dir_all(&path)?;
        self.rollup_build_cache_dir = path;
        Ok(())
    }
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

#[derive(Debug)]
pub struct ManagedRollupProcess {
    child: Option<Child>,
}

impl ManagedRollupProcess {
    pub fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    pub fn id(&self) -> u32 {
        self.child
            .as_ref()
            .expect("managed rollup process child is missing")
            .id()
    }

    pub fn into_child(mut self) -> Option<Child> {
        self.child.take()
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };

        match child.try_wait()? {
            Some(status) => {
                // Child has exited and was reaped by try_wait.
                self.child.take();
                Ok(Some(status))
            }
            None => Ok(None),
        }
    }

    pub fn wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let Some(mut child) = self.child.take() else {
            return Ok(None);
        };
        let status = child.wait()?;
        Ok(Some(status))
    }
}

impl Drop for ManagedRollupProcess {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };

        kill_rollup(child.id());
        if let Err(e) = child.wait() {
            tracing::warn!("Failed to wait for rollup process during cleanup: {e}");
        }
    }
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

fn is_very_close_to_soak_test_end(num_soak_batches: u64, target_soak_batches: u64) -> bool {
    num_soak_batches.saturating_add(15) > target_soak_batches
}

/// Send SIGTERM to the rollup manager process group to gracefully shut it down.
/// If the process group doesn't respond within 10 seconds, send SIGKILL.
pub fn kill_rollup(rollup_id: u32) {
    let process_group = format!("-{rollup_id}");
    tracing::info!(
        "Sending SIGTERM to rollup manager process group {} (leader pid {})",
        process_group,
        rollup_id
    );

    // Send SIGTERM to the full process group (manager + rollup children).
    if let Err(e) = Command::new("kill")
        .args(["-s", "SIGTERM", &process_group])
        .status()
    {
        tracing::error!("Failed to send SIGTERM: {}", e);
        return;
    }

    // Wait up to 10 seconds for graceful shutdown.
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(100));

        // Check if the process group still exists using kill -0.
        match Command::new("kill").args(["-0", &process_group]).status() {
            Ok(status) if !status.success() => {
                tracing::info!("Rollup process group {process_group} shut down gracefully");
                return;
            }
            Err(_) => {
                tracing::info!(
                    "Unable to check rollup process group {process_group} status; assuming it no longer exists"
                );
                return;
            }
            Ok(_) => {
                // Process group still exists, continue waiting.
            }
        }
    }

    // Process group didn't respond to SIGTERM, force kill.
    tracing::warn!(
        "Rollup process group {process_group} didn't respond to SIGTERM after 10s, sending SIGKILL"
    );
    if let Err(e) = Command::new("kill").args(["-9", &process_group]).status() {
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
    soak_config: SoakManagerConfig,
    throughput_start_batch: u64,
    rollup_stop_height: u64,
    save_slot_snapshots: bool,
) -> Result<ThroughputReport, anyhow::Error> {
    let (rollup_tx, mut rollup_rx) = oneshot::channel();
    let rollup_id = rollup.id();
    let mut rollup_guard = RollupCleanupGuard::new(rollup_id);
    let mut rollup_exited = false;
    let target_soak_batches = rollup_stop_height.saturating_sub(throughput_start_batch);

    // Spawn background task to wait for rollup process
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || rollup.wait()).await;
        let _ = rollup_tx.send(result);
    });

    // Start soak manager process orchestration.
    let (soak_shutdown_tx, soak_shutdown_rx) = oneshot::channel();
    let soak_handle =
        tokio::spawn(
            async move { run_soak_coordinator(&soak_config, API_URL, soak_shutdown_rx).await },
        );
    let mut soak_guard = SoakCoordinatorGuard::new(soak_shutdown_tx, soak_handle);

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, &directories);
    slot_fetcher.subscribe_slots(false).await?;
    let (tx, _rx) = watch::channel(false);
    let mut worker_set = JoinSet::new();

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
    let mut num_previous_txs: Option<u64> = None;

    let run_result: anyhow::Result<()> = async {
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
                                if batch_num >= throughput_start_batch && num_previous_txs.is_none() {
                                    let reference_batch = slot_fetcher
                                        .fetch_batch_without_children(throughput_start_batch)
                                        .await
                                        .map_err(|e| anyhow!("failed to fetch throughput start batch {throughput_start_batch}: {e}"))?;
                                    num_previous_txs = Some(reference_batch.tx_range.end);
                                }

                                // Count throughput from the first batch after `throughput_start_batch`.
                                if batch_num > throughput_start_batch {
                                    if let Some(previous_txs) = num_previous_txs {
                                        num_soak_txs = batch.tx_range.end.saturating_sub(previous_txs);
                                        num_soak_batches += 1;
                                    }
                                }
                            }
                            Err(e) => {
                                // If we're very close to the end of the test, the rollup might have shut down before we could finish querying.
                                // The test shouldn't fail for this reason, so we just skip the batch.
                                if is_very_close_to_soak_test_end(num_soak_batches, target_soak_batches) {
                                    tracing::debug!("Soak slot fetcher encountered an error near the end of the test; num_soak_batches: {num_soak_batches}, target_soak_batches: {target_soak_batches}, slot number: {}, rollup_stop_height: {rollup_stop_height}", slot.number);
                                    tracing::warn!("Encountered an error very near the end of the test. Assuming the rollup shut down.");
                                    break Ok(());
                                } else {
                                    break Err(anyhow!("Failed to fetch batch {}: {}", batch_num, e));
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
                    info!("Received new slot {}, with batch {}. Rollup has processed {} txs in {} slots. Average throughput: {} txs/slot", slot.number, slot.batch_range.start, num_soak_txs, num_soak_slots, num_soak_txs as f64 / num_soak_slots as f64);
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
                tracing::info!("Received Ctrl+C, initiating soak shutdown");
                rollup_guard.request_shutdown();
                break Err(anyhow!("Received Ctrl+C (SIGINT), shutting down soak run"));
            },
            _ = terminate.recv() => {
                tracing::info!("Received SIGTERM, initiating soak shutdown");
                rollup_guard.request_shutdown();
                break Err(anyhow!("Received SIGTERM, shutting down soak run"));
            },
            _ = quit.recv() => {
                tracing::info!("Received SIGQUIT, initiating soak shutdown");
                rollup_guard.request_shutdown();
                break Err(anyhow!("Received SIGQUIT, shutting down soak run"));
            },
            // Rollup shutdown
            rollup_result = &mut rollup_rx => {
                match rollup_result {
                    Ok(rollup_result) => {
                        ensure_rollup_exit_result(rollup_result)?;
                        tracing::info!("Rollup process finished with successful status");
                        rollup_exited = true;
                        rollup_guard.mark_exited();
                    },
                    Err(e) => {
                        break Err(anyhow!("Failed to receive rollup process result: {e}"));
                    },
                }
                break Ok(());
            }
            // Worker task failure
            Some(worker_result) = worker_set.join_next() => {
                match worker_result {
                    Ok(Ok(())) => {
                        // Worker completed successfully, continue monitoring
                    }
                    Ok(Err(e)) => {
                        if is_very_close_to_soak_test_end(num_soak_batches, target_soak_batches) {
                            tracing::debug!("Worker task failed near the end of the test; num_soak_batches: {num_soak_batches}, target_soak_batches: {target_soak_batches}, rollup_stop_height: {rollup_stop_height}, err: {e}");
                            tracing::warn!("Worker task failed very near the end of the test. Assuming the rollup shut down.");
                        } else {
                            tracing::error!("Worker task failed: {}", e);
                            rollup_guard.request_shutdown();
                            break Err(e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Worker task panicked: {}", e);
                        rollup_guard.request_shutdown();
                        break Err(e.into());
                    }
                }
            }
        }
        }
    }
    .await;

    let mut final_error = run_result.err();

    soak_guard.request_shutdown();

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

    if let Err(e) = soak_guard
        .shutdown_and_wait(SOAK_COORDINATOR_SHUTDOWN_TIMEOUT)
        .await
    {
        if final_error.is_none() {
            final_error = Some(e);
        } else {
            tracing::warn!("Ignoring soak coordinator shutdown failure: {e}");
        }
    }

    if !rollup_exited {
        match wait_for_rollup_exit_with_timeout(rollup_guard.rollup_id(), &mut rollup_rx).await {
            Ok(()) => {
                rollup_guard.mark_exited();
            }
            Err(e) => {
                if final_error.is_none() {
                    final_error = Some(e);
                } else {
                    tracing::warn!("Ignoring additional rollup shutdown failure: {e}");
                }
            }
        }
    }

    if let Some(err) = final_error {
        return Err(err);
    }

    let average_throughput = if num_soak_slots == 0 {
        0.0
    } else {
        num_soak_txs as f64 / num_soak_slots as f64
    };

    info!(
        "Rollup process finished. Processed {} txs in  {} slots. Average throughput: {} txs/slot",
        num_soak_txs, num_soak_slots, average_throughput
    );
    Ok(ThroughputReport {
        num_txs: num_soak_txs,
        num_slots: num_soak_slots,
    })
}
