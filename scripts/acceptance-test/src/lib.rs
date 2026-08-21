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

use crate::fetch_and_compare::{
    compare_against_snapshot, load_snapshot_json, save_slot_snapshot, SlotFetcher,
};
mod config;
mod evm_contracts;
pub mod evm_soak;
pub mod fetch_and_compare;
mod state_consistency;
mod versioned_setup;
pub use config::{
    cleanup_rollup_state_dir, prepare_rollup_state_dir, CommonArgs, ExistingRollupState,
    ResolvedRunSettings, RunProfile, DEFAULT_BLOCKS_PER_VERSION, DEFAULT_FULL_SLOT_SAVE_INTERVAL,
    DEFAULT_POSTGRES_CONTAINER_NAME,
};
pub use versioned_setup::{
    extend_last_stop_height, last_version_soak_config, prepare_acceptance_run_plan,
    prepare_acceptance_run_plan_with_constants, recorded_data_bounds, spawn_rollup_manager,
    write_manager_config, AcceptanceRunPlan, LocalConstantsManifest, RecordedDataBounds,
};

pub const API_URL: &str = "http://127.0.0.1:12348";
pub const API_ADDR: &str = "127.0.0.1:12348";
pub const SETUP_THROUGHPUT_FILE: &str = "acceptance_throughput.json";
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
    pub fn from_settings(settings: &ResolvedRunSettings) -> Result<Self, anyhow::Error> {
        let cwd = std::env::current_dir().context("failed to read current working directory")?;
        let acceptance_test_dir = env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        Self::from_settings_with_acceptance_dir_and_cwd(settings, acceptance_test_dir, cwd)
    }

    pub fn from_settings_with_acceptance_dir(
        settings: &ResolvedRunSettings,
        acceptance_test_dir: PathBuf,
    ) -> Result<Self, anyhow::Error> {
        let cwd = std::env::current_dir().context("failed to read current working directory")?;
        Self::from_settings_with_acceptance_dir_and_cwd(settings, acceptance_test_dir, cwd)
    }

    fn from_settings_with_acceptance_dir_and_cwd(
        settings: &ResolvedRunSettings,
        acceptance_test_dir: PathBuf,
        cwd: PathBuf,
    ) -> Result<Self, anyhow::Error> {
        let acceptance_test_dir = absolutize_from_cwd(acceptance_test_dir, &cwd);
        let acceptance_test_metadata = fs::metadata(&acceptance_test_dir).with_context(|| {
            format!(
                "acceptance test directory {} does not exist",
                acceptance_test_dir.display()
            )
        })?;
        anyhow::ensure!(
            acceptance_test_metadata.is_dir(),
            "acceptance test path {} is not a directory",
            acceptance_test_dir.display()
        );

        let rollup_root = acceptance_test_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let rollup_build_cache_dir = if let Some(path) = settings.binary_cache_dir.clone() {
            absolutize_from_cwd(path, &cwd)
        } else {
            acceptance_test_dir.join("rollup-build-cache")
        };
        fs::create_dir_all(&rollup_build_cache_dir).with_context(|| {
            format!(
                "failed to create rollup build cache directory {}",
                rollup_build_cache_dir.display()
            )
        })?;
        let manager_build_dir = acceptance_test_dir.join("rollup-manager-build");

        let output_root = if let Some(path) = settings.acceptance_data_dir.clone() {
            absolutize_from_cwd(path, &cwd)
        } else {
            acceptance_test_dir.join("acceptance-test-data")
        };
        let output_dir = output_root.join(settings.profile.subdir());
        fs::create_dir_all(&output_dir).with_context(|| {
            format!(
                "failed to create acceptance test data directory {}",
                output_dir.display()
            )
        })?;
        let rollup_data_path = if let Some(path) = settings.rollup_state_dir.clone() {
            absolutize_from_cwd(path, &cwd)
        } else {
            output_dir.join("rollup-starter-data")
        };
        let snapshots_dir = output_dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir).with_context(|| {
            format!(
                "failed to create acceptance snapshot directory {}",
                snapshots_dir.display()
            )
        })?;

        let throughput_root = if let Some(path) = settings.acceptance_throughput_dir.clone() {
            absolutize_from_cwd(path, &cwd)
        } else {
            acceptance_test_dir.join("acceptance-throughput")
        };
        let throughput_dir = throughput_root.join(settings.profile.subdir());
        fs::create_dir_all(&throughput_dir).with_context(|| {
            format!(
                "failed to create acceptance throughput directory {}",
                throughput_dir.display()
            )
        })?;

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
}

fn absolutize_from_cwd(path: PathBuf, cwd: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
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

/// How long to wait for the sequencer to come up on a plain rollup start.
pub const SEQUENCER_READY_STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
/// How long to wait for the sequencer when crossing a version boundary: the rollup manager
/// may first run the new version's db migration, which can take a while on a full data set.
pub const SEQUENCER_READY_HANDOVER_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Wait until the rollup node's ledger API responds. Unlike sequencer readiness (which
/// requires the node to have caught up to the DA head), this only requires the node process to
/// be up: the ledger API serves — and slot subscriptions work — while the node is still
/// resyncing.
pub async fn wait_for_rollup_api(
    shutdown_rx: &mut ShutdownReceiver,
    timeout: Duration,
) -> Result<(), anyhow::Error> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(response) = reqwest::get(format!("{}/ledger/slots/0", API_URL)).await {
            if response.status().is_success() {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "rollup API did not come up within {timeout:?}; if this run crossed a version \
                 boundary, the db migration may still be running or have failed — check the \
                 rollup manager output for migration logs"
            );
        }
        sleep_or_shutdown(Duration::from_millis(100), shutdown_rx).await?;
    }
}

pub async fn wait_for_sequencer_ready(
    shutdown_rx: &mut ShutdownReceiver,
    timeout: Duration,
) -> Result<(), anyhow::Error> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(response) = reqwest::get(format!("{}/sequencer/ready", API_URL)).await {
            if response.status().is_success() {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "sequencer did not become ready within {timeout:?}; if this run crossed a \
                 version boundary, the db migration may still be running or have failed — \
                 check the rollup manager output for migration logs"
            );
        }
        sleep_or_shutdown(Duration::from_millis(100), shutdown_rx).await?;
    }
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

/// Fetch and save snapshots for any slots in `[from_slot, up_to_exclusive)` that don't already
/// have one on disk. Used by `run_soak` in append mode to keep snapshot coverage contiguous
/// across the resync→generation handoff, where the live-tail slot subscription may only start a
/// few slots after the new version began producing. Slots are fetched with children so the
/// snapshot is fully populated.
async fn backfill_missing_snapshots(
    client: &sov_api_spec::Client,
    directories: &Directories,
    from_slot: u64,
    up_to_exclusive: u64,
) -> Result<(), anyhow::Error> {
    for slot_number in from_slot..up_to_exclusive {
        let snapshot_path = directories
            .snapshots_dir
            .join(format!("slot_{:04}_with_children.json", slot_number));
        if snapshot_path.exists() {
            continue;
        }
        let slot = client
            .get_slot_by_id(
                &types::IntOrHash::Integer(slot_number),
                Some(GetSlotByIdChildren::X1),
            )
            .await
            .map_err(|e| anyhow!("failed to backfill snapshot for slot {slot_number}: {e}"))?;
        save_slot_snapshot(&slot.into_inner(), &directories.snapshots_dir)?;
        tracing::info!("Backfilled missing snapshot for slot {slot_number}");
    }
    Ok(())
}

fn ignore_file_not_found<OK: Default>(e: std::io::Error) -> std::io::Result<OK> {
    if e.kind() == std::io::ErrorKind::NotFound {
        Ok(OK::default())
    } else {
        Err(e)
    }
}

/// Copy the durable `persistent_mock_da.sqlite*` files back to `mock_da.sqlite*` so a run can
/// resync against the saved MockDA without growing the persisted database on every run. Stale
/// `mock_da` files from a previous run are removed first (the `-shm`/`-wal` sidecars in
/// particular may not get overwritten by a copy).
pub fn copy_persistent_mock_data(directories: &Directories) -> Result<(), anyhow::Error> {
    tracing::info!("Copying persistent mock data back to mock_da.sqlite");
    for stale_file in [
        directories.output_dir.join("mock_da.sqlite"),
        directories.output_dir.join("mock_da.sqlite-shm"),
        directories.output_dir.join("mock_da.sqlite-wal"),
    ] {
        std::fs::remove_file(&stale_file)
            .or_else(ignore_file_not_found)
            .with_context(|| {
                format!(
                    "failed to remove stale mock DA file {}",
                    stale_file.display()
                )
            })?;
    }

    // Then copy the base file, always.
    let persistent_mock_da = directories.output_dir.join("persistent_mock_da.sqlite");
    let mock_da = directories.output_dir.join("mock_da.sqlite");
    std::fs::copy(&persistent_mock_da, &mock_da).with_context(|| {
        format!(
            "failed to copy persistent mock DA database from {} to {}",
            persistent_mock_da.display(),
            mock_da.display()
        )
    })?;
    // And the dangling wal and shm only if they exist.
    for (persistent_sidecar, mock_da_sidecar) in [
        (
            directories.output_dir.join("persistent_mock_da.sqlite-shm"),
            directories.output_dir.join("mock_da.sqlite-shm"),
        ),
        (
            directories.output_dir.join("persistent_mock_da.sqlite-wal"),
            directories.output_dir.join("mock_da.sqlite-wal"),
        ),
    ] {
        std::fs::copy(&persistent_sidecar, &mock_da_sidecar)
            .or_else(ignore_file_not_found)
            .with_context(|| {
                format!(
                    "failed to copy persistent mock DA sidecar from {} to {}",
                    persistent_sidecar.display(),
                    mock_da_sidecar.display()
                )
            })?;
    }

    tracing::info!("Persistent mock data copied back to mock_da.sqlite");
    Ok(())
}

/// While the last recorded version shuts down at the end of the recorded data, the slot
/// subscription and ledger API can fail before the harness has finished comparing the last few
/// slots. Errors within this many slots of the recorded data's end are treated as handover
/// flakiness rather than test failures.
pub const HANDOVER_FLAKINESS_SLOT_WINDOW: u64 = 15;

/// Resync the rollup against the existing saved snapshots, verifying that each replayed slot
/// matches its snapshot. Finishes when the final recorded snapshot (`last_recorded_slot`) has
/// been verified — the last recorded version stops exactly at the end of the recorded data, so
/// no slot beyond it arrives until the next version takes over. Availability errors within
/// [`HANDOVER_FLAKINESS_SLOT_WINDOW`] slots of that boundary are tolerated as handover
/// flakiness; snapshot content mismatches are always fatal.
///
/// `expected_setup_batches` is a *batch* number (rollup height minus one), not a slot number:
/// slot numbers run ahead of batch numbers because of empty DA slots.
///
/// Returns `(latest_batch_num, first_new_slot)`, where `first_new_slot` is the first slot after
/// the recorded data — the slot where a not-yet-generated next version would begin.
pub async fn resync_and_verify_slots(
    directories: &Directories,
    expected_setup_batches: u64,
    last_recorded_slot: Option<u64>,
    shutdown_rx: &mut ShutdownReceiver,
) -> Result<(u64, u64), anyhow::Error> {
    // Wait for the freshly spawned rollup's API to come up before subscribing.
    wait_for_rollup_api(shutdown_rx, SEQUENCER_READY_STARTUP_TIMEOUT).await?;

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, directories);
    slot_fetcher.subscribe_slots(false).await?;

    let mut checked = 0;
    let client = get_rollup_client()?;
    let mut latest_batch_num = 0;
    let handover_tail_start = last_recorded_slot
        .map(|last_slot| last_slot.saturating_sub(HANDOVER_FLAKINESS_SLOT_WINDOW));
    let in_handover_tail =
        |slot_number: u64| handover_tail_start.is_some_and(|start| slot_number >= start);
    // Recorded data can span multiple rollup versions, and each mid-data version boundary is a
    // legitimate interruption: the outgoing version stops exactly at its boundary and the next
    // one takes over (possibly after running its db migration first). When the subscription or
    // the ledger API drops mid-resync, wait for the incoming version and resume from where we
    // stopped. The no-progress guard turns a repeated drop at the same slot (e.g. a crash-looping
    // rollup) into a hard error instead of an infinite reconnect cycle.
    let mut last_reconnect_at: Option<u64> = None;
    macro_rules! reconnect_or_bail {
        ($reason:expr) => {{
            let reason: String = $reason;
            if last_reconnect_at == Some(checked) {
                anyhow::bail!(
                    "slot subscription dropped again at slot {checked} without progress since \
                     the previous version-handover reconnect ({reason})"
                );
            }
            last_reconnect_at = Some(checked);
            tracing::info!(
                last_compared_slot = checked,
                reason,
                "Slot stream interrupted mid-resync; assuming a version handover and \
                 reconnecting (the incoming version may need to run a db migration first)"
            );
            // Wait for the node API only, NOT sequencer readiness: the incoming version's
            // sequencer won't be ready until it has resynced all the way to the DA head,
            // whereas the slot subscription can (and must) follow that resync live.
            wait_for_rollup_api(shutdown_rx, SEQUENCER_READY_HANDOVER_TIMEOUT).await?;
            slot_fetcher = SlotFetcher::new(get_rollup_client()?, directories);
            slot_fetcher.subscribe_slots(false).await?;
        }};
    }
    let first_new_slot = 'outer: loop {
        let next = tokio::select! {
            slot = slot_fetcher.next_slot() => slot,
            reason = wait_for_shutdown(shutdown_rx) => return Err(shutdown_error(reason)),
        };
        let slot = match next {
            Ok(Some(slot)) => slot,
            other => {
                let reason = match other {
                    Ok(None) => "subscription ended".to_string(),
                    Err(error) => format!("subscription failed: {error:#}"),
                    Ok(Some(_)) => unreachable!("handled above"),
                };
                if in_handover_tail(checked) {
                    tracing::warn!(
                        last_compared_slot = checked,
                        reason,
                        "Slot stream dropped at the end of the recorded data; treating the \
                         remaining recorded slots as shutdown flakiness"
                    );
                    break 'outer boundary_slot(last_recorded_slot);
                }
                reconnect_or_bail!(reason);
                continue 'outer;
            }
        };
        for slot_number in checked..=slot.number {
            let Ok(snapshot) = load_snapshot_json(slot_number, &directories.snapshots_dir) else {
                // We might be missing a few slots at the beginning.
                // If the slot number is less than 10, just ignore the missing snapshot.
                if slot_number < 10 {
                    continue;
                } else if latest_batch_num < expected_setup_batches {
                    panic!("Missing snapshot for slot {}", slot_number);
                } else {
                    // Once we've passed the setup batch count and we find the first missing
                    // snapshot, we're done.
                    tracing::info!(
                        "Missing snapshot found at slot {}. Finished resyncing.",
                        slot_number
                    );
                    break 'outer slot_number;
                }
            };
            let slot_snapshot: Slot = serde_json::from_value(snapshot.clone()).unwrap();
            latest_batch_num = slot_snapshot.batch_range.end.saturating_sub(1);
            let include_children = if slot_snapshot.batches.is_empty() {
                None
            } else {
                Some(GetSlotByIdChildren::X1)
            };
            let slot = match client
                .get_slot_by_id(&types::IntOrHash::Integer(slot_number), include_children)
                .await
            {
                Ok(slot) => slot,
                Err(error) if in_handover_tail(slot_number) => {
                    tracing::warn!(
                        slot_number,
                        %error,
                        "Slot API unavailable at the end of the recorded data; skipping \
                         comparison of the remaining recorded slots as shutdown flakiness"
                    );
                    break 'outer boundary_slot(last_recorded_slot);
                }
                Err(error) => {
                    // `checked` has not advanced yet, so the slots compared in this batch of
                    // the loop are re-verified after the reconnect (comparison is idempotent).
                    reconnect_or_bail!(format!("ledger API error at slot {slot_number}: {error}"));
                    continue 'outer;
                }
            };
            compare_against_snapshot(
                &slot.into_inner(),
                snapshot,
                &format!("slot_{}", slot_number),
                false,
            )?;

            // Once the final recorded snapshot has been verified we're done.
            if Some(slot_number) == last_recorded_slot {
                if latest_batch_num < expected_setup_batches {
                    panic!(
                        "Verified the final recorded snapshot at slot {} but only found {} batches, expected {}",
                        slot_number, latest_batch_num, expected_setup_batches
                    );
                }
                tracing::info!(
                    "Verified the final recorded snapshot at slot {}. Finished resyncing.",
                    slot_number
                );
                break 'outer slot_number + 1;
            }
        }
        checked = slot.number;
    };

    tracing::info!(
        "Rollup resync complete. Found {} batches; the recorded data ends before slot {}.",
        latest_batch_num,
        first_new_slot
    );

    Ok((latest_batch_num, first_new_slot))
}

/// The first slot after the recorded data, used when the resync loop finishes via handover
/// flakiness. Only reachable when `last_recorded_slot` is known (the tail window is derived
/// from it).
fn boundary_slot(last_recorded_slot: Option<u64>) -> u64 {
    last_recorded_slot
        .expect("handover tail tolerance requires known recorded data bounds")
        .saturating_add(1)
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

#[derive(Debug, Clone, Copy)]
pub struct SoakRunOptions {
    pub throughput_start_batch: u64,
    pub rollup_stop_height: u64,
    pub full_slot_save_interval: u64,
    pub save_slot_snapshots: bool,
    /// When `Some(first_slot)` (and `save_slot_snapshots` is set), guarantee contiguous snapshot
    /// coverage starting at `first_slot`. On the first slot this run observes, the gap
    /// `[first_slot, observed_slot)` is backfilled once by fetching those slots directly; the
    /// live-tail subscription is contiguous afterwards. This closes the gap between the end of
    /// resync and the first slot this soak run observes — used by append mode. `None` preserves
    /// the default behavior (save only observed slots).
    pub snapshot_backfill_start: Option<u64>,
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
    options: SoakRunOptions,
    mut shutdown_rx: ShutdownReceiver,
) -> Result<ThroughputReport, anyhow::Error> {
    let SoakRunOptions {
        throughput_start_batch,
        rollup_stop_height,
        full_slot_save_interval,
        save_slot_snapshots,
        snapshot_backfill_start,
    } = options;
    let target_soak_batches = rollup_stop_height.saturating_sub(throughput_start_batch);
    // The first slot of the appended version. On the first slot this run observes, we backfill
    // any gap between it and here, exactly once (see `SoakRunOptions::snapshot_backfill_start`).
    // Only active when snapshot saving was also requested (append mode).
    let mut snapshot_backfill_from = if save_slot_snapshots {
        snapshot_backfill_start
    } else {
        None
    };

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
                // In append mode the live-tail subscription may only start a few slots after the
                // new version began producing. On the first slot we observe, backfill any gap
                // between the resync boundary and here so the new version's snapshots are
                // contiguous; the subscription is contiguous from this point on, so this runs
                // exactly once (`take`).
                if let Some(backfill_from) = snapshot_backfill_from.take() {
                    backfill_missing_snapshots(&client, &directories, backfill_from, slot.number)
                        .await?;
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
                if num_soak_slots % full_slot_save_interval == 0 {
                   match client.get_slot_by_id(&types::IntOrHash::Integer(slot.number), Some(GetSlotByIdChildren::X1)).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_use_profile_subdirectories() {
        let temp = tempfile::tempdir().unwrap();
        let acceptance_test_dir = temp.path().join("scripts/acceptance-test");
        fs::create_dir_all(&acceptance_test_dir).unwrap();

        let settings = ResolvedRunSettings::from_common_args(CommonArgs {
            short: true,
            ..CommonArgs::default()
        });
        let directories =
            Directories::from_settings_with_acceptance_dir(&settings, acceptance_test_dir.clone())
                .unwrap();

        assert_eq!(
            directories.output_dir,
            acceptance_test_dir
                .join("acceptance-test-data")
                .join("short")
        );
        assert_eq!(
            directories.throughput_dir,
            acceptance_test_dir
                .join("acceptance-throughput")
                .join("short")
        );
        assert_eq!(
            directories.rollup_data_path,
            directories.output_dir.join("rollup-starter-data")
        );
    }

    #[test]
    fn explicit_rollup_state_dir_is_used_literally() {
        let temp = tempfile::tempdir().unwrap();
        let acceptance_test_dir = temp.path().join("scripts/acceptance-test");
        fs::create_dir_all(&acceptance_test_dir).unwrap();
        let explicit_state_dir = temp.path().join("custom-state");

        let settings = ResolvedRunSettings::from_common_args(CommonArgs {
            short: true,
            rollup_state_dir: Some(explicit_state_dir.clone()),
            ..CommonArgs::default()
        });
        let directories =
            Directories::from_settings_with_acceptance_dir(&settings, acceptance_test_dir).unwrap();

        assert_eq!(directories.rollup_data_path, explicit_state_dir);
    }

    #[test]
    fn relative_user_paths_are_resolved_against_invocation_directory() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("invocation");
        let acceptance_test_dir = temp.path().join("repo/scripts/acceptance-test");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(&acceptance_test_dir).unwrap();

        let settings = ResolvedRunSettings::from_common_args(CommonArgs {
            acceptance_data_dir: Some(PathBuf::from("acceptance-data")),
            acceptance_throughput_dir: Some(PathBuf::from("throughput-data")),
            rollup_state_dir: Some(PathBuf::from("rollup-state")),
            binary_cache_dir: Some(PathBuf::from("binary-cache")),
            ..CommonArgs::default()
        });
        let directories = Directories::from_settings_with_acceptance_dir_and_cwd(
            &settings,
            acceptance_test_dir,
            cwd.clone(),
        )
        .unwrap();

        assert_eq!(directories.rollup_build_cache_dir, cwd.join("binary-cache"));
        assert_eq!(
            directories.output_dir,
            cwd.join("acceptance-data").join("full")
        );
        assert_eq!(directories.rollup_data_path, cwd.join("rollup-state"));
        assert_eq!(
            directories.throughput_dir,
            cwd.join("throughput-data").join("full")
        );
    }
}
