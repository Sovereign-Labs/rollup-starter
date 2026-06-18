use acceptance_test::{
    cleanup_rollup_state_dir, copy_persistent_mock_data, extend_last_stop_height,
    generate_postgres_password, prepare_acceptance_run_plan_with_constants, prepare_rollup_state_dir,
    resync_and_verify_slots, run_soak, run_until_shutdown_signal, sleep_or_shutdown,
    spawn_rollup_manager, wait_for_sequencer_ready, write_manager_config, AcceptanceRunPlan,
    CommonArgs, Directories, LocalConstantsManifest, PostgresContainerGuard, ResolvedRunSettings,
    RunProfile, ShutdownReceiver, SoakRunOptions, ThroughputReport, API_URL, SETUP_THROUGHPUT_FILE,
};
use anyhow::Context;
use chrono::Utc;
use clap::Parser;
use std::time::Duration;
use tracing::info;

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
    let manager_config_path = directories
        .output_dir
        .join("acceptance_manager_config.json");
    write_manager_config(&manager_config_path, &manager_versions)?;

    // Start the rollup. Run for 10 seconds
    info!("Starting rollup through sov-rollup-manager");
    let stop_at_height = manager_versions
        .last()
        .and_then(|version| version.stop_height)
        .unwrap_or_default();
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

    let (latest_batch_num, _first_new_slot) =
        resync_and_verify_slots(&directories, expected_setup_batches, &mut shutdown_rx).await?;

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
            rollup_stop_height: stop_at_height,
            full_slot_save_interval: settings.full_slot_save_interval,
            save_slot_snapshots: false,
            snapshot_backfill_start: None,
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
