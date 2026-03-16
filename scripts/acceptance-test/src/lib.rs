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
use std::path::PathBuf;
use std::{
    env, fs,
    process::{Command as StdCommand, Output},
    thread,
    time::Duration,
};
use std::{fmt, future::Future};
use tokio::process::Child;
use tokio::sync::{oneshot, watch};
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
const ROLLUP_FORCED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const TOP_LEVEL_SHUTDOWN_ABORT_TIMEOUT: Duration = Duration::from_secs(90);

pub type Runtime = <StarterRollup<Native> as RollupBlueprint<Native>>::Runtime;
pub type Spec = <StarterRollup<Native> as RollupBlueprint<Native>>::Spec;
pub type ShutdownReceiver = watch::Receiver<Option<ShutdownReason>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownReason {
    SigInt,
    SigTerm,
    SigQuit,
    SigHup,
}

impl fmt::Display for ShutdownReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SigInt => "Ctrl+C (SIGINT)",
            Self::SigTerm => "SIGTERM",
            Self::SigQuit => "SIGQUIT",
            Self::SigHup => "SIGHUP",
        })
    }
}

pub fn shutdown_error(reason: ShutdownReason) -> anyhow::Error {
    anyhow!("Received {reason}, shutting down")
}

#[derive(Debug)]
pub struct PostgresContainerGuard {
    container_name: String,
}

impl PostgresContainerGuard {
    pub fn start(container_name: &str, password: &str) -> Result<Self, anyhow::Error> {
        start_and_wait_for_postgres_ready(container_name, password)?;
        Ok(Self {
            container_name: container_name.to_owned(),
        })
    }
}

impl Drop for PostgresContainerGuard {
    fn drop(&mut self) {
        if let Err(e) = cleanup_postgres_container(&self.container_name) {
            tracing::warn!(
                container = %self.container_name,
                "Failed to cleanup postgres container during drop: {e}"
            );
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
    StdCommand::new("docker")
        .args(args)
        .output()
        .with_context(|| format!("failed to run docker {}", args.join(" ")))
}

pub async fn wait_for_shutdown(shutdown_rx: &mut ShutdownReceiver) -> ShutdownReason {
    loop {
        if let Some(reason) = *shutdown_rx.borrow() {
            return reason;
        }
        shutdown_rx
            .changed()
            .await
            .expect("shutdown sender dropped unexpectedly");
    }
}

pub async fn sleep_or_shutdown(
    duration: Duration,
    shutdown_rx: &mut ShutdownReceiver,
) -> Result<(), anyhow::Error> {
    tokio::select! {
        _ = tokio::time::sleep(duration) => Ok(()),
        reason = wait_for_shutdown(shutdown_rx) => Err(shutdown_error(reason)),
    }
}

fn flatten_top_level_task_result<T>(
    result: Result<Result<T, anyhow::Error>, tokio::task::JoinError>,
) -> Result<T, anyhow::Error> {
    match result {
        Ok(result) => result,
        Err(e) => Err(anyhow!("acceptance test task panicked: {e}")),
    }
}

pub async fn run_until_shutdown_signal<T, F, Fut>(run: F) -> Result<T, anyhow::Error>
where
    T: Send + 'static,
    F: FnOnce(ShutdownReceiver) -> Fut,
    Fut: Future<Output = Result<T, anyhow::Error>> + Send + 'static,
{
    use tokio::signal::unix::{signal, SignalKind};

    let mut terminate =
        signal(SignalKind::terminate()).context("failed to register SIGTERM handler")?;
    let mut quit = signal(SignalKind::quit()).context("failed to register SIGQUIT handler")?;
    let mut hup = signal(SignalKind::hangup()).context("failed to register SIGHUP handler")?;

    let (shutdown_tx, shutdown_rx) = watch::channel(None);
    let mut run_handle = tokio::spawn(run(shutdown_rx));

    let shutdown_reason = tokio::select! {
        result = &mut run_handle => return flatten_top_level_task_result(result),
        _ = tokio::signal::ctrl_c() => ShutdownReason::SigInt,
        _ = terminate.recv() => ShutdownReason::SigTerm,
        _ = quit.recv() => ShutdownReason::SigQuit,
        _ = hup.recv() => ShutdownReason::SigHup,
    };

    tracing::info!("Received {shutdown_reason}, requesting graceful shutdown");
    let _ = shutdown_tx.send(Some(shutdown_reason));

    match tokio::time::timeout(TOP_LEVEL_SHUTDOWN_ABORT_TIMEOUT, &mut run_handle).await {
        Ok(result) => {
            let run_result = flatten_top_level_task_result(result);
            match run_result {
                Ok(_) => Err(shutdown_error(shutdown_reason)),
                Err(e) => Err(e),
            }
        }
        Err(_) => {
            tracing::warn!(
                "Timed out waiting {:?} for top-level shutdown after {shutdown_reason}; aborting task",
                TOP_LEVEL_SHUTDOWN_ABORT_TIMEOUT
            );
            run_handle.abort();
            match run_handle.await {
                Ok(Ok(_)) => Err(shutdown_error(shutdown_reason)),
                Ok(Err(e)) => Err(e),
                Err(e) if e.is_cancelled() => Err(shutdown_error(shutdown_reason)),
                Err(e) => Err(anyhow!(
                    "acceptance test task panicked during shutdown: {e}"
                )),
            }
        }
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

pub async fn wait_for_sequencer_ready(
    shutdown_rx: &mut ShutdownReceiver,
) -> Result<(), anyhow::Error> {
    // Wait up to a minute for the sequencer to be ready
    for _ in 0..600 {
        if let Ok(response) = reqwest::get(format!("{}/sequencer/ready", API_URL)).await {
            if response.status().is_success() {
                break;
            }
        }
        sleep_or_shutdown(Duration::from_millis(100), shutdown_rx).await?;
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
    child: Child,
}

impl ManagedRollupProcess {
    pub fn new(child: Child) -> Self {
        Self { child }
    }

    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    pub fn request_shutdown(&self) {
        if let Some(rollup_id) = self.child.id() {
            send_rollup_sigterm(rollup_id);
        }
    }

    pub async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait().await
    }

    async fn wait_for_exit(
        &mut self,
        timeout_duration: Duration,
    ) -> Result<Option<std::process::ExitStatus>, anyhow::Error> {
        let rollup_id = self.child.id();
        match tokio::time::timeout(timeout_duration, self.wait()).await {
            Ok(Ok(exit_status)) => Ok(Some(exit_status)),
            Ok(Err(e)) => match rollup_id {
                Some(rollup_id) => Err(anyhow!(
                    "Failed to wait for rollup process {rollup_id}: {e}"
                )),
                None => Err(anyhow!(
                    "Failed to wait for already-exited rollup process: {e}"
                )),
            },
            Err(_) => Ok(None),
        }
    }

    pub async fn shutdown(&mut self) -> Result<(), anyhow::Error> {
        if self.child.id().is_none() {
            return Ok(());
        }

        self.request_shutdown();
        if let Some(exit_status) = self.wait_for_exit(ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT).await? {
            return ensure_rollup_exit_status(exit_status);
        }

        let Some(rollup_id) = self.child.id() else {
            return Ok(());
        };
        tracing::warn!(
            "Timed out waiting {:?} for rollup process {} to exit after SIGTERM. Sending SIGKILL.",
            ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT,
            rollup_id
        );
        send_rollup_sigkill(rollup_id);
        if let Some(exit_status) = self.wait_for_exit(ROLLUP_FORCED_SHUTDOWN_TIMEOUT).await? {
            return ensure_rollup_exit_status(exit_status);
        }

        Err(anyhow!(
            "Rollup process {} did not terminate within {:?} after SIGKILL",
            rollup_id,
            ROLLUP_FORCED_SHUTDOWN_TIMEOUT
        ))
    }

    pub async fn ensure_stopped(&mut self) -> Result<(), anyhow::Error> {
        if self.child.id().is_none() {
            return Ok(());
        }

        match self.wait_for_exit(ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT).await? {
            Some(exit_status) => ensure_rollup_exit_status(exit_status),
            None => {
                let Some(rollup_id) = self.child.id() else {
                    return Ok(());
                };
                tracing::warn!(
                    "Timed out waiting {:?} for rollup process {} to exit naturally. Sending shutdown signal.",
                    ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT,
                    rollup_id
                );
                self.shutdown().await
            }
        }
    }
}

impl Drop for ManagedRollupProcess {
    fn drop(&mut self) {
        if let Some(rollup_id) = self.child.id() {
            send_rollup_sigkill(rollup_id);
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

fn send_rollup_process_group_signal(rollup_id: u32, signal: libc::c_int, signal_name: &str) {
    let rollup_pid: libc::pid_t = rollup_id
        .try_into()
        .expect("rollup pid must fit in libc::pid_t");
    let process_group = -rollup_pid;
    // SAFETY: `libc::kill` is an FFI call. We pass a valid `pid_t` derived from the child pid
    // and a signal number from libc; any operational failure is reported via the return value and
    // `errno`, which we handle below.
    let rc = unsafe { libc::kill(process_group, signal) };
    if rc == 0 {
        tracing::info!(
            "Sent {signal_name} to rollup manager process group {process_group} (leader pid {rollup_id})"
        );
        return;
    }

    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        tracing::debug!(
            "Rollup process group {process_group} no longer exists while sending {signal_name}"
        );
    } else {
        tracing::error!(
            "Failed to send {signal_name} to rollup manager process group {process_group}: {err}"
        );
    }
}

fn send_rollup_sigterm(rollup_id: u32) {
    send_rollup_process_group_signal(rollup_id, libc::SIGTERM, "SIGTERM");
}

fn send_rollup_sigkill(rollup_id: u32) {
    send_rollup_process_group_signal(rollup_id, libc::SIGKILL, "SIGKILL");
}

fn ensure_rollup_exit_status(exit_status: std::process::ExitStatus) -> anyhow::Result<()> {
    anyhow::ensure!(
        exit_status.success(),
        "Rollup process exited with non-zero status: {exit_status}"
    );
    Ok(())
}

fn combine_soak_errors(
    primary_error: Option<anyhow::Error>,
    additional_errors: Vec<anyhow::Error>,
) -> Option<anyhow::Error> {
    let additional_len = additional_errors.len();
    match (primary_error, additional_len) {
        (None, 0) => None,
        (Some(err), 0) => Some(err),
        (None, 1) => Some(
            additional_errors
                .into_iter()
                .next()
                .expect("single additional error must exist"),
        ),
        (primary_error, _) => {
            let mut messages = Vec::new();
            if let Some(err) = primary_error {
                messages.push(format!("Primary error: {err:#}"));
            }
            for (idx, err) in additional_errors.into_iter().enumerate() {
                messages.push(format!("Additional error {}: {err:#}", idx + 1));
            }
            Some(anyhow!(messages.join("\n")))
        }
    }
}

pub async fn run_soak(
    directories: Directories,
    mut rollup: ManagedRollupProcess,
    soak_config: SoakManagerConfig,
    throughput_start_batch: u64,
    rollup_stop_height: u64,
    save_slot_snapshots: bool,
    mut shutdown_rx: ShutdownReceiver,
) -> Result<ThroughputReport, anyhow::Error> {
    let target_soak_batches = rollup_stop_height.saturating_sub(throughput_start_batch);

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, &directories);
    slot_fetcher.subscribe_slots(false).await?;
    let mut background_tasks = JoinSet::new();

    // Keep the sender alive so the coordinator's shutdown receiver stays pending until the task
    // is aborted during teardown.
    let (_soak_shutdown_tx, soak_shutdown_rx) = oneshot::channel();
    background_tasks.spawn(async move {
        run_soak_coordinator(&soak_config, API_URL, soak_shutdown_rx)
            .await
            .map_err(|e| anyhow!("background soak coordinator failed: {e}"))
    });

    // Start state validation worker
    let state_validator_client = get_rollup_client()?;
    background_tasks.spawn(state_validation_worker(
        state_validator_client,
        rollup_stop_height,
    ));

    let evm_contracts = load_state_consistency_contracts(&directories)?;
    for (idx, address) in evm_contracts.pinned.into_iter().enumerate() {
        let worker_key = pinned_worker_key(idx)?;
        background_tasks.spawn(evm_state_consistency_worker(address, worker_key, "pinned"));
    }

    for (idx, address) in evm_contracts.unpinned.into_iter().enumerate() {
        let worker_key = unpinned_worker_key(idx)?;
        background_tasks.spawn(evm_state_consistency_worker(
            address, worker_key, "unpinned",
        ));
    }

    let client = get_rollup_client()?;

    tracing::info!("Background tasks started. Listening for slots");
    let mut num_soak_txs = 0;
    let mut num_soak_slots = 0;
    let mut num_soak_batches = 0;
    let mut num_previous_txs: Option<u64> = None;

    let run_result: anyhow::Result<()> = async {
        loop {
            tokio::select! {
            biased;
            // Rollup shutdown
            rollup_result = rollup.wait() => {
                let exit_status = rollup_result
                    .map_err(|e| anyhow!("Failed to wait for rollup process: {e}"))?;
                ensure_rollup_exit_status(exit_status)?;
                tracing::info!("Rollup process finished with successful status");
                break Ok(());
            }
            // Background task failure
            Some(task_result) = background_tasks.join_next() => {
                match task_result {
                    Ok(Ok(())) => {
                        // Background task completed successfully, continue monitoring.
                    }
                    Ok(Err(e)) => {
                        if is_very_close_to_soak_test_end(num_soak_batches, target_soak_batches) {
                            tracing::debug!("Background task failed near the end of the test; num_soak_batches: {num_soak_batches}, target_soak_batches: {target_soak_batches}, rollup_stop_height: {rollup_stop_height}, err: {e}");
                            tracing::warn!("Background task failed very near the end of the test. Assuming the rollup shut down.");
                        } else {
                            tracing::error!("Background task failed: {}", e);
                            break Err(e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Background task panicked: {}", e);
                        break Err(e.into());
                    }
                }
            }
            // On each slot, we update our counters and save a snapshot of the slot.
            // Every N slots, we save a full snapshot of the slot. (This is much more expensive, but also allows more thorough checks)
            new_slot = slot_fetcher.next_slot() => {
                let Some(slot) = new_slot? else {
                    match rollup.child.try_wait() {
                        Ok(Some(exit_status)) => {
                            ensure_rollup_exit_status(exit_status)?;
                            break Ok(());
                        }
                        Ok(None) => {
                            if is_very_close_to_soak_test_end(num_soak_batches, target_soak_batches)
                            {
                                tracing::warn!(
                                    "Slot stream closed near expected test end while rollup manager pid={:?} was still running. Treating this as shutdown and proceeding to teardown.",
                                    rollup.id()
                                );
                                break Ok(());
                            }
                            tracing::warn!(
                                "Slot stream closed before rollup manager exited (pid={:?})",
                                rollup.id()
                            );
                            break Err(anyhow!(
                                "Slot stream closed before rollup manager exited (pid={:?})",
                                rollup.id()
                            ));
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to query rollup manager status after slot stream closed (pid={:?}): {e}",
                                rollup.id()
                            );
                            break Err(anyhow!(
                                "Failed to query rollup manager status after slot stream closed (pid={:?}): {e}",
                                rollup.id()
                            ));
                        }
                    }
                };

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
            shutdown_reason = wait_for_shutdown(&mut shutdown_rx) => {
                tracing::info!("Received {shutdown_reason}, initiating soak shutdown");
                break Err(shutdown_error(shutdown_reason));
            },
        }
        }
    }
    .await;

    let primary_error = run_result.err();
    let mut additional_errors = Vec::new();

    background_tasks.abort_all();
    while let Some(task_result) = background_tasks.join_next().await {
        match task_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if is_very_close_to_soak_test_end(num_soak_batches, target_soak_batches) {
                    tracing::warn!(
                        "Ignoring background task failure during shutdown very near the end of the test: {e}"
                    );
                } else {
                    additional_errors.push(e);
                }
            }
            Err(e) if e.is_cancelled() => {}
            Err(e) => {
                additional_errors.push(anyhow!("Background task panicked during shutdown: {e}"));
            }
        }
    }

    let rollup_shutdown_result = if primary_error.is_some() {
        rollup.shutdown().await
    } else {
        rollup.ensure_stopped().await
    };

    match rollup_shutdown_result {
        Ok(()) => {}
        Err(e) => {
            additional_errors.push(e);
        }
    }

    if let Some(err) = combine_soak_errors(primary_error, additional_errors) {
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
