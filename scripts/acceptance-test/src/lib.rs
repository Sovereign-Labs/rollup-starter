use anyhow::{anyhow, Context};
use evm_soak::{
    evm_state_consistency_worker, load_state_consistency_contracts, pinned_worker_key,
    unpinned_worker_key,
};
use rand::distributions::Alphanumeric;
use rand::Rng;
use rollup_starter::rollup::StarterRollup;
use sov_api_spec::types::{self, GetSlotByIdChildren, Slot};
use sov_api_spec::ClientInfo;
use sov_modules_api::execution_mode::Native;
use sov_modules_api::prelude::serde;
use sov_modules_rollup_blueprint::RollupBlueprint;
use sov_soak_manager::{run_soak_coordinator, SoakManagerConfig};
use state_consistency::state_validation_worker;
use std::path::{Path, PathBuf};
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
use toml_edit::{value, DocumentMut, Table};
use tracing::{debug, info};

use crate::fetch_and_compare::{save_slot_snapshot, SlotFetcher};
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
    extend_last_stop_height, latest_version_restart_manager_versions, prepare_acceptance_run_plan,
    prepare_acceptance_run_plan_with_constants, spawn_rollup_manager, with_last_stop_height,
    write_manager_config, AcceptanceRunPlan, LocalConstantsManifest,
};

pub const API_URL: &str = "http://127.0.0.1:12348";
pub const API_ADDR: &str = "127.0.0.1:12348";
pub const SETUP_THROUGHPUT_FILE: &str = "acceptance_throughput.json";
const ROLLUP_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
const ROLLUP_FORCED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const TOP_LEVEL_SHUTDOWN_ABORT_TIMEOUT: Duration = Duration::from_secs(90);
const EVM_SOAK_SAFETY_STOP_BLOCKS: u64 = 20;

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

#[derive(Debug, Clone)]
pub struct SoakRunOptions {
    pub throughput_start_batch: u64,
    pub rollup_stop_height: u64,
    pub full_slot_save_interval: u64,
    pub save_slot_snapshots: bool,
    pub nomt_bucket_growth: Option<NomtBucketGrowthConfig>,
}

#[derive(Debug, Clone)]
pub struct NomtBucketGrowthConfig {
    pub rollup_config_path: PathBuf,
    pub restart_manager_binary: PathBuf,
    pub restart_manager_config_path: PathBuf,
    pub initial_rollup_stop_height: u64,
    pub interval_blocks: u64,
    pub kernel_bucket_growth_numerator: u64,
    pub kernel_bucket_growth_denominator: u64,
    pub user_bucket_increment: u64,
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

#[derive(Debug, Clone, Copy)]
struct NomtBucketSizes {
    kernel: u64,
    user: u64,
}

#[derive(Debug, Clone, Copy)]
struct NomtBucketGrowth {
    before: NomtBucketSizes,
    after: NomtBucketSizes,
}

struct SoakBackgroundTasks {
    tasks: JoinSet<anyhow::Result<()>>,
    soak_shutdown_tx: Option<oneshot::Sender<()>>,
    evm_shutdown_tx: Option<watch::Sender<bool>>,
}

fn read_bucket_count(storage: &Table, key: &str, config_path: &Path) -> anyhow::Result<u64> {
    let item = storage.get(key).ok_or_else(|| {
        anyhow!(
            "missing storage.{key} in rollup config {}",
            config_path.display()
        )
    })?;
    let raw = item.as_integer().ok_or_else(|| {
        anyhow!(
            "storage.{key} in rollup config {} is not an integer",
            config_path.display()
        )
    })?;
    u64::try_from(raw).with_context(|| {
        format!(
            "storage.{key} in rollup config {} must be non-negative",
            config_path.display()
        )
    })
}

fn write_bucket_count(
    storage: &mut Table,
    key: &str,
    buckets: u64,
    config_path: &Path,
) -> anyhow::Result<()> {
    let buckets_i64 = i64::try_from(buckets).with_context(|| {
        format!(
            "new storage.{key} value {buckets} does not fit in TOML integer range for {}",
            config_path.display()
        )
    })?;
    storage.insert(key, value(buckets_i64));
    Ok(())
}

fn grow_nomt_buckets(
    config_path: &Path,
    kernel_bucket_growth_numerator: u64,
    kernel_bucket_growth_denominator: u64,
    user_bucket_increment: u64,
) -> anyhow::Result<NomtBucketGrowth> {
    anyhow::ensure!(
        kernel_bucket_growth_numerator > 0,
        "kernel bucket growth numerator must be greater than zero"
    );
    anyhow::ensure!(
        kernel_bucket_growth_denominator > 0,
        "kernel bucket growth denominator must be greater than zero"
    );

    let raw = fs::read_to_string(config_path)
        .with_context(|| format!("failed to read rollup config {}", config_path.display()))?;
    let mut doc = raw.parse::<DocumentMut>().with_context(|| {
        format!(
            "failed to parse rollup config {} as TOML",
            config_path.display()
        )
    })?;
    let storage = doc
        .get_mut("storage")
        .and_then(|item| item.as_table_mut())
        .ok_or_else(|| {
            anyhow!(
                "missing [storage] table in rollup config {}",
                config_path.display()
            )
        })?;

    let before = NomtBucketSizes {
        kernel: read_bucket_count(storage, "kernel_hashtable_buckets", config_path)?,
        user: read_bucket_count(storage, "user_hashtable_buckets", config_path)?,
    };
    let scaled_kernel_buckets = before
        .kernel
        .checked_mul(kernel_bucket_growth_numerator)
        .ok_or_else(|| {
            anyhow!(
                "kernel_hashtable_buckets overflow while multiplying {} by {}",
                before.kernel,
                kernel_bucket_growth_numerator
            )
        })?;
    let rounded_scaled_kernel_buckets = scaled_kernel_buckets
        .checked_add(kernel_bucket_growth_denominator - 1)
        .ok_or_else(|| {
            anyhow!(
                "kernel_hashtable_buckets overflow while rounding {} / {} up",
                scaled_kernel_buckets,
                kernel_bucket_growth_denominator
            )
        })?;
    let after = NomtBucketSizes {
        kernel: rounded_scaled_kernel_buckets / kernel_bucket_growth_denominator,
        user: before
            .user
            .checked_add(user_bucket_increment)
            .ok_or_else(|| {
                anyhow!(
                    "user_hashtable_buckets overflow while adding {} to {}",
                    user_bucket_increment,
                    before.user
                )
            })?,
    };

    write_bucket_count(
        storage,
        "kernel_hashtable_buckets",
        after.kernel,
        config_path,
    )?;
    write_bucket_count(storage, "user_hashtable_buckets", after.user, config_path)?;
    fs::write(config_path, doc.to_string())
        .with_context(|| format!("failed to write rollup config {}", config_path.display()))?;

    Ok(NomtBucketGrowth { before, after })
}

fn soak_config_for_generation(
    base: &SoakManagerConfig,
    generation: u32,
    stop_height_override: Option<u64>,
) -> SoakManagerConfig {
    let mut config = base.clone();
    config.config.salt = config
        .config
        .salt
        .saturating_add(config.config.num_workers.saturating_mul(generation));
    if let Some(stop_height) = stop_height_override {
        for (_, version_stop_height) in &mut config.versions {
            *version_stop_height = stop_height;
        }
    }
    config
}

#[derive(serde::Deserialize)]
struct ChainStateValueResponse<T> {
    value: T,
}

async fn query_current_rollup_height(client: &sov_api_spec::Client) -> reqwest::Result<u64> {
    let current_heights_url = format!("{}/modules/chain-state/state/current-heights/", API_URL);
    client
        .client()
        .get(current_heights_url)
        .send()
        .await?
        .json::<ChainStateValueResponse<(u64, u64)>>()
        .await
        .map(|resp| resp.value.0)
}

async fn request_evm_soak_shutdown_before_stop_height(
    rollup_stop_height: u64,
    evm_shutdown_tx: watch::Sender<bool>,
) -> anyhow::Result<()> {
    let evm_stop_height = rollup_stop_height.saturating_sub(EVM_SOAK_SAFETY_STOP_BLOCKS);
    let client = get_rollup_client()?;

    loop {
        match query_current_rollup_height(&client).await {
            Ok(current_height) if current_height >= evm_stop_height => {
                info!(
                    current_height,
                    evm_stop_height,
                    rollup_stop_height,
                    "Stopping EVM soak workers before rollup stop height"
                );
                let _ = evm_shutdown_tx.send(true);
                return Ok(());
            }
            Ok(_) => {}
            Err(e) => {
                debug!("Failed to query rollup height for EVM pre-stop coordination: {e}");
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn spawn_soak_background_tasks(
    directories: &Directories,
    soak_config: SoakManagerConfig,
    rollup_stop_height: u64,
) -> anyhow::Result<SoakBackgroundTasks> {
    let mut tasks = JoinSet::new();

    let (soak_shutdown_tx, soak_shutdown_rx) = oneshot::channel();
    tasks.spawn(async move {
        run_soak_coordinator(&soak_config, API_URL, soak_shutdown_rx)
            .await
            .map_err(|e| anyhow!("background soak coordinator failed: {e}"))
    });

    let state_validator_client = get_rollup_client()?;
    tasks.spawn(state_validation_worker(
        state_validator_client,
        rollup_stop_height,
    ));

    let (evm_shutdown_tx, evm_shutdown_rx) = watch::channel(false);
    tasks.spawn(request_evm_soak_shutdown_before_stop_height(
        rollup_stop_height,
        evm_shutdown_tx.clone(),
    ));

    let evm_contracts = load_state_consistency_contracts(directories)?;
    for (idx, address) in evm_contracts.pinned.into_iter().enumerate() {
        let worker_key = pinned_worker_key(idx)?;
        tasks.spawn(evm_state_consistency_worker(
            address,
            worker_key,
            "pinned",
            evm_shutdown_rx.clone(),
        ));
    }

    for (idx, address) in evm_contracts.unpinned.into_iter().enumerate() {
        let worker_key = unpinned_worker_key(idx)?;
        tasks.spawn(evm_state_consistency_worker(
            address,
            worker_key,
            "unpinned",
            evm_shutdown_rx.clone(),
        ));
    }

    Ok(SoakBackgroundTasks {
        tasks,
        soak_shutdown_tx: Some(soak_shutdown_tx),
        evm_shutdown_tx: Some(evm_shutdown_tx),
    })
}

async fn stop_background_tasks(
    mut background_tasks: SoakBackgroundTasks,
    ignore_errors: bool,
) -> Vec<anyhow::Error> {
    if let Some(soak_shutdown_tx) = background_tasks.soak_shutdown_tx.take() {
        let _ = soak_shutdown_tx.send(());
    }
    if let Some(evm_shutdown_tx) = background_tasks.evm_shutdown_tx.take() {
        let _ = evm_shutdown_tx.send(true);
    }
    background_tasks.tasks.abort_all();

    let mut errors = Vec::new();
    while let Some(task_result) = background_tasks.tasks.join_next().await {
        match task_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) if ignore_errors => {
                debug!("Ignoring background task error during planned restart: {e}");
            }
            Ok(Err(e)) => errors.push(e),
            Err(e) if e.is_cancelled() => {}
            Err(e) if ignore_errors => {
                debug!("Ignoring background task panic during planned restart: {e}");
            }
            Err(e) => errors.push(anyhow!("Background task panicked during shutdown: {e}")),
        }
    }
    errors
}

fn write_manager_config_stop_height(
    manager_config_path: &Path,
    stop_height: u64,
) -> anyhow::Result<()> {
    let raw = fs::read_to_string(manager_config_path).with_context(|| {
        format!(
            "failed to read restart manager config {}",
            manager_config_path.display()
        )
    })?;
    let mut manager_config: sov_rollup_manager::ManagerConfig = serde_json::from_str(&raw)
        .with_context(|| {
            format!(
                "failed to parse restart manager config {}",
                manager_config_path.display()
            )
        })?;
    let version = manager_config.versions.last_mut().ok_or_else(|| {
        anyhow!(
            "restart manager config {} has no versions",
            manager_config_path.display()
        )
    })?;
    version.stop_height = Some(stop_height);
    fs::write(
        manager_config_path,
        serde_json::to_string_pretty(&manager_config)?,
    )
    .with_context(|| {
        format!(
            "failed to write restart manager config {}",
            manager_config_path.display()
        )
    })?;
    Ok(())
}

async fn grow_nomt_buckets_and_start_next_rollup(
    growth_config: &NomtBucketGrowthConfig,
    directories: &Directories,
    slot_fetcher: &mut SlotFetcher,
    shutdown_rx: &mut ShutdownReceiver,
    completed_stop_height: u64,
    next_stop_height: u64,
) -> anyhow::Result<ManagedRollupProcess> {
    info!(
        completed_stop_height,
        next_stop_height,
        config_path = %growth_config.rollup_config_path.display(),
        "Preparing next rollup segment after NOMT bucket growth stop height"
    );

    let growth = grow_nomt_buckets(
        &growth_config.rollup_config_path,
        growth_config.kernel_bucket_growth_numerator,
        growth_config.kernel_bucket_growth_denominator,
        growth_config.user_bucket_increment,
    )?;
    info!(
        kernel_before = growth.before.kernel,
        kernel_after = growth.after.kernel,
        user_before = growth.before.user,
        user_after = growth.after.user,
        config_path = %growth_config.rollup_config_path.display(),
        "Grew NOMT bucket counts"
    );

    write_manager_config_stop_height(&growth_config.restart_manager_config_path, next_stop_height)?;

    let rollup = spawn_rollup_manager(
        &growth_config.restart_manager_binary,
        &growth_config.restart_manager_config_path,
        directories,
        None,
    )?;

    wait_for_sequencer_ready(shutdown_rx).await?;
    slot_fetcher
        .subscribe_slots(false)
        .await
        .context("failed to resubscribe to slots after NOMT bucket growth restart")?;
    info!(
        next_stop_height,
        "Rollup restarted after NOMT bucket growth"
    );
    Ok(rollup)
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
        nomt_bucket_growth,
    } = options;
    let target_soak_batches = rollup_stop_height.saturating_sub(throughput_start_batch);
    if let Some(growth_config) = &nomt_bucket_growth {
        anyhow::ensure!(
            growth_config.interval_blocks > 0,
            "NOMT bucket growth interval must be greater than zero"
        );
        anyhow::ensure!(
            growth_config.initial_rollup_stop_height <= rollup_stop_height,
            "initial NOMT bucket growth stop height {} exceeds final rollup stop height {}",
            growth_config.initial_rollup_stop_height,
            rollup_stop_height
        );
    }
    let mut segment_stop_height = nomt_bucket_growth
        .as_ref()
        .map(|growth_config| growth_config.initial_rollup_stop_height)
        .unwrap_or(rollup_stop_height);
    let stop_height_override = nomt_bucket_growth.as_ref().map(|_| segment_stop_height);

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, &directories);
    slot_fetcher.subscribe_slots(false).await?;
    let mut background_generation = 0_u32;
    let mut background_tasks = Some(spawn_soak_background_tasks(
        &directories,
        soak_config_for_generation(&soak_config, background_generation, stop_height_override),
        segment_stop_height,
    )?);

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
                if let Some(growth_config) = &nomt_bucket_growth {
                    if segment_stop_height < rollup_stop_height {
                        let stopped_tasks = background_tasks
                            .take()
                            .expect("background tasks must be running before restart");
                        let ignored_errors = stop_background_tasks(stopped_tasks, true).await;
                        for err in ignored_errors {
                            tracing::debug!(
                                "Ignored background task error while preparing NOMT bucket growth restart: {err:#}"
                            );
                        }

                        let next_stop_height = segment_stop_height
                            .saturating_add(growth_config.interval_blocks)
                            .min(rollup_stop_height);
                        rollup = grow_nomt_buckets_and_start_next_rollup(
                            growth_config,
                            &directories,
                            &mut slot_fetcher,
                            &mut shutdown_rx,
                            segment_stop_height,
                            next_stop_height,
                        )
                        .await?;

                        segment_stop_height = next_stop_height;
                        background_generation = background_generation.saturating_add(1);
                        background_tasks = Some(spawn_soak_background_tasks(
                            &directories,
                            soak_config_for_generation(
                                &soak_config,
                                background_generation,
                                Some(segment_stop_height),
                            ),
                            segment_stop_height,
                        )?);
                        tracing::info!(
                            generation = background_generation,
                            segment_stop_height,
                            "Restarted soak background tasks after NOMT bucket growth"
                        );
                        continue;
                    }
                }
                break Ok(());
            }
            // Background task failure
            Some(task_result) = background_tasks
                .as_mut()
                .expect("background tasks must be running during soak")
                .tasks
                .join_next() => {
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

    if let Some(background_tasks) = background_tasks.take() {
        let task_errors = stop_background_tasks(background_tasks, false).await;
        for e in task_errors {
            if is_very_close_to_soak_test_end(num_soak_batches, target_soak_batches) {
                tracing::warn!(
                    "Ignoring background task failure during shutdown very near the end of the test: {e}"
                );
            } else {
                additional_errors.push(e);
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

    #[test]
    fn grow_nomt_buckets_updates_storage_counts() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("rollup.toml");
        fs::write(
            &config_path,
            r#"
[storage]
path = "/tmp/rollup-state"
kernel_hashtable_buckets = 16_000
user_hashtable_buckets = 2_000_000
"#,
        )
        .unwrap();

        let growth = grow_nomt_buckets(&config_path, 3, 2, 1_000_000).unwrap();

        assert_eq!(growth.before.kernel, 16_000);
        assert_eq!(growth.before.user, 2_000_000);
        assert_eq!(growth.after.kernel, 24_000);
        assert_eq!(growth.after.user, 3_000_000);

        let updated = fs::read_to_string(&config_path).unwrap();
        let doc = updated.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["storage"]["kernel_hashtable_buckets"].as_integer(),
            Some(24_000)
        );
        assert_eq!(
            doc["storage"]["user_hashtable_buckets"].as_integer(),
            Some(3_000_000)
        );
    }
}
