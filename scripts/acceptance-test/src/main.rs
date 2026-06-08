use acceptance_test::fetch_and_compare::SlotFetcher;
use acceptance_test::{
    cleanup_rollup_state_dir, extend_last_stop_height,
    fetch_and_compare::{compare_against_snapshot, load_snapshot_json},
    generate_postgres_password, get_rollup_client, latest_version_restart_manager_versions,
    prepare_acceptance_run_plan_with_constants, prepare_rollup_state_dir, run_soak,
    run_until_shutdown_signal, shutdown_error, sleep_or_shutdown, spawn_rollup_manager,
    wait_for_shutdown, with_last_stop_height, write_manager_config, AcceptanceRunPlan, CommonArgs,
    Directories, LocalConstantsManifest, NomtBucketGrowthConfig, PostgresContainerGuard,
    ResolvedRunSettings, RunProfile, ShutdownReceiver, SoakRunOptions, API_URL,
};
use acceptance_test::{wait_for_sequencer_ready, ThroughputReport, SETUP_THROUGHPUT_FILE};
use anyhow::Context;
use chrono::Utc;
use clap::Parser;
use sov_api_spec::types::{self, GetSlotByIdChildren, Slot};
use std::time::Duration;
use tracing::info;

const NOMT_BUCKET_GROWTH_INTERVAL_BLOCKS: u64 = 200;
const NOMT_KERNEL_BUCKET_GROWTH_FACTOR: u64 = 2;
const NOMT_USER_BUCKET_GROWTH_INCREMENT: u64 = 1_000_000;

struct PreparedTestRun {
    directories: Directories,
    password: String,
    plan: AcceptanceRunPlan,
    settings: ResolvedRunSettings,
    throughput_check: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    // Initialize tracing subscriber with RUST_LOG environment variable, fallback to info
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug,hyper=info,sov_sequencer::rest_api=off,tower_http::trace=off,alloy_transport_http=warn,alloy_rpc_client=warn")),
        )
        .init();

    info!("Starting acceptance test");

    let prepared = prepare_test_run(args)?;
    let result =
        run_until_shutdown_signal(move |shutdown_rx| run_test(prepared, shutdown_rx)).await;
    if let Err(e) = &result {
        tracing::error!("Acceptance test failed: {}", e);
    } else {
        info!("Acceptance test completed");
    }

    result
}

fn ignore_file_not_found<OK: Default>(e: std::io::Error) -> std::io::Result<OK> {
    if e.kind() == std::io::ErrorKind::NotFound {
        Ok(OK::default())
    } else {
        Err(e)
    }
}

fn copy_persistent_mock_data(directories: &Directories) -> Result<(), anyhow::Error> {
    tracing::info!("Copying persistent mock data back to mock_da.sqlite");
    // Clean up any files from any previous runs. This is needed particularly for the shm and wal
    // files since they may not get overwritten by a copy, but we do all three for consistency.
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

    // Then copy the base file, always
    let persistent_mock_da = directories.output_dir.join("persistent_mock_da.sqlite");
    let mock_da = directories.output_dir.join("mock_da.sqlite");
    std::fs::copy(&persistent_mock_da, &mock_da).with_context(|| {
        format!(
            "failed to copy persistent mock DA database from {} to {}",
            persistent_mock_da.display(),
            mock_da.display()
        )
    })?;
    // And the dangling wal and shm only if they exist
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

fn prepare_test_run(args: Args) -> Result<PreparedTestRun, anyhow::Error> {
    let throughput_check = throughput_check_enabled(&args);
    let settings = ResolvedRunSettings::from_common_args(args.common);
    let password = generate_postgres_password()?;
    let directories = Directories::from_settings(&settings)?;
    prepare_rollup_state_dir(
        &directories.rollup_data_path,
        settings.on_existing_rollup_state,
    )?;
    let plan = prepare_acceptance_run_plan_with_constants(
        &directories,
        &password,
        settings.blocks_per_version,
        LocalConstantsManifest::AcceptanceTest,
    )?;
    Ok(PreparedTestRun {
        directories,
        password,
        plan,
        settings,
        throughput_check,
    })
}

fn next_nomt_bucket_growth_stop_height(current_stop_height: u64, final_stop_height: u64) -> u64 {
    current_stop_height
        .saturating_add(NOMT_BUCKET_GROWTH_INTERVAL_BLOCKS)
        .min(final_stop_height)
}

async fn run_test(
    prepared: PreparedTestRun,
    mut shutdown_rx: ShutdownReceiver,
) -> Result<(), anyhow::Error> {
    let PreparedTestRun {
        directories,
        password,
        plan,
        settings,
        throughput_check,
    } = prepared;

    // Copy the persistent mock data back to mock_da.sqlite. This way we don't grow our DA files with each run.
    copy_persistent_mock_data(&directories)?;

    // Start postgres and keep it alive for the test duration. Drop cleanup runs last.
    let _postgres_guard =
        PostgresContainerGuard::start(&settings.postgres_docker_container_name, &password)?;
    let expected_setup_batches = plan
        .manager_versions
        .last()
        .expect("Acceptance testing must have at least one rollup version")
        .stop_height
        .expect("Acceptance testing last rollup version must have stop height")
        // Genesis doesn't have a batch; this has the result that batch numbers lag 1 behind the
        // rollup height.
        .saturating_sub(1);
    let manager_versions =
        extend_last_stop_height(&plan.manager_versions, settings.blocks_per_version);
    let final_stop_at_height = manager_versions
        .last()
        .and_then(|version| version.stop_height)
        .unwrap_or_default();
    let initial_stop_at_height =
        next_nomt_bucket_growth_stop_height(expected_setup_batches + 1, final_stop_at_height);
    let initial_manager_versions =
        with_last_stop_height(&manager_versions, initial_stop_at_height)?;
    let manager_config_path = directories
        .output_dir
        .join("acceptance_manager_config.json");
    write_manager_config(&manager_config_path, &initial_manager_versions)?;
    let restart_manager_versions = latest_version_restart_manager_versions(&manager_versions)?;
    let nomt_growth_rollup_config_path = restart_manager_versions
        .first()
        .expect("restart manager config must contain one rollup version")
        .config_path
        .clone();
    let restart_manager_config_path = directories
        .output_dir
        .join("acceptance_restart_manager_config.json");
    write_manager_config(&restart_manager_config_path, &restart_manager_versions)?;

    // Start the rollup. Run for 10 seconds
    info!("Starting rollup through sov-rollup-manager");
    let rollup = spawn_rollup_manager(
        &plan.manager_binary,
        &manager_config_path,
        &directories,
        None,
    )?;

    // Wait up to 60s for the rollup to be ready
    for _ in 0..120 {
        if reqwest::get(&format!("{}/ledger/slots/0", API_URL))
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        sleep_or_shutdown(Duration::from_millis(500), &mut shutdown_rx).await?;
    }

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, &directories);
    slot_fetcher.subscribe_slots(false).await?;

    let mut checked = 0;
    let client = get_rollup_client()?;
    let mut latest_batch_num = 0;
    'outer: loop {
        let slot = tokio::select! {
            slot = slot_fetcher.next_slot() => slot?.unwrap(),
            reason = wait_for_shutdown(&mut shutdown_rx) => return Err(shutdown_error(reason)),
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
                    // Once we've passed the setup batch count and we find the first missing snapshot, we're done.
                    tracing::info!(
                        "Missing snapshot found at slot {}. Finished resyncing.",
                        slot_number
                    );
                    break 'outer;
                }
            };
            let slot_snapshot: Slot = serde_json::from_value(snapshot.clone()).unwrap();
            latest_batch_num = slot_snapshot.batch_range.end.saturating_sub(1);
            let include_children = if slot_snapshot.batches.is_empty() {
                None
            } else {
                Some(GetSlotByIdChildren::X1)
            };
            let slot = client
                .get_slot_by_id(&types::IntOrHash::Integer(slot_number), include_children)
                .await?;
            compare_against_snapshot(
                &slot.into_inner(),
                snapshot,
                &format!("slot_{}", slot_number),
                false,
            )?;
        }
        checked = slot.number;
    }

    tracing::info!(
        "Rollup resync complete. All slots match their snapshots. Found {} batches.",
        latest_batch_num
    );

    // Wait for the sequencer to resync to the empty DA slots
    wait_for_sequencer_ready(&mut shutdown_rx).await?;

    let resync_soak_config = plan
        .soak_config
        .for_resync(settings.blocks_per_version)
        .ok_or_else(|| anyhow::anyhow!("failed to create soak resync config"))?;

    let new_throughput_report = run_soak(
        directories.clone(),
        rollup,
        resync_soak_config,
        SoakRunOptions {
            throughput_start_batch: latest_batch_num,
            rollup_stop_height: final_stop_at_height,
            full_slot_save_interval: settings.full_slot_save_interval,
            save_slot_snapshots: false,
            nomt_bucket_growth: Some(NomtBucketGrowthConfig {
                rollup_config_path: nomt_growth_rollup_config_path,
                restart_manager_binary: plan.manager_binary.clone(),
                restart_manager_config_path,
                initial_rollup_stop_height: initial_stop_at_height,
                interval_blocks: NOMT_BUCKET_GROWTH_INTERVAL_BLOCKS,
                kernel_bucket_multiplier: NOMT_KERNEL_BUCKET_GROWTH_FACTOR,
                user_bucket_increment: NOMT_USER_BUCKET_GROWTH_INCREMENT,
            }),
        },
        shutdown_rx.clone(),
    )
    .await?;
    if throughput_check {
        let throughput_baseline_path = directories.throughput_dir.join(SETUP_THROUGHPUT_FILE);
        let previous_throughput_contents =
            std::fs::read_to_string(&throughput_baseline_path).with_context(|| {
                format!(
                    "failed to read setup throughput baseline at {}. Run `setup` with the same profile or pass `--no-throughput-check`",
                    throughput_baseline_path.display()
                )
            })?;
        let previous_throughput_report: ThroughputReport =
            serde_json::from_str::<ThroughputReport>(&previous_throughput_contents).with_context(
                || {
                    format!(
                        "failed to parse setup throughput baseline {}",
                        throughput_baseline_path.display()
                    )
                },
            )?;
        let previous_throughput = previous_throughput_report.throughput();
        let new_throughput = new_throughput_report.throughput();
        let min_throughput_ratio = throughput_regression_min_ratio(settings.profile);
        if new_throughput < (previous_throughput * min_throughput_ratio) {
            anyhow::bail!(
                "Throughput is less than {:.0}% of the previous throughput for the {:?} profile. This is likely due to a bug in the rollup. Old throughput: {:.2} txs/slot, new throughput: {:.2} txs/slot",
                min_throughput_ratio * 100.0,
                settings.profile,
                previous_throughput,
                new_throughput
            );
        }
    }

    // Save throughput report with timestamp to keep a record of test runs
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let throughput_filename = format!("test_throughput_{}.json", timestamp);
    let throughput_path = directories.throughput_dir.join(&throughput_filename);
    std::fs::write(
        &throughput_path,
        serde_json::to_string(&new_throughput_report)?,
    )
    .with_context(|| {
        format!(
            "failed to write throughput report {}",
            throughput_path.display()
        )
    })?;
    info!("Saved throughput report to {}", throughput_filename);
    if settings.cleanup_rollup_state_on_success() {
        cleanup_rollup_state_dir(&directories.rollup_data_path)?;
        info!(
            "Cleaned transient rollup state directory {}",
            directories.rollup_data_path.display()
        );
    }
    Ok(())
}

fn throughput_check_enabled(args: &Args) -> bool {
    !args.no_throughput_check
}

fn throughput_regression_min_ratio(profile: RunProfile) -> f64 {
    match profile {
        RunProfile::Full => 0.9,
        RunProfile::Short => 0.7,
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[command(flatten)]
    common: CommonArgs,

    /// Disable the throughput regression check.
    #[arg(long)]
    no_throughput_check: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throughput_check_defaults_to_enabled() {
        let args = Args::try_parse_from(["acceptance-test"]).unwrap();

        assert!(throughput_check_enabled(&args));
    }

    #[test]
    fn no_throughput_check_disables_check() {
        let args = Args::try_parse_from(["acceptance-test", "--no-throughput-check"]).unwrap();

        assert!(!throughput_check_enabled(&args));
    }

    #[test]
    fn full_profile_uses_strict_throughput_threshold() {
        assert_eq!(throughput_regression_min_ratio(RunProfile::Full), 0.9);
    }

    #[test]
    fn short_profile_uses_lenient_throughput_threshold() {
        assert_eq!(throughput_regression_min_ratio(RunProfile::Short), 0.7);
    }
}
