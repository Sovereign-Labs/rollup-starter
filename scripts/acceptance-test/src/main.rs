use acceptance_test::fetch_and_compare::SlotFetcher;
use acceptance_test::{
    build_rollup, wait_for_sequencer_ready, ThroughputReport, SETUP_THROUGHPUT_FILE,
};
use acceptance_test::{
    cleanup_postgres_container,
    fetch_and_compare::{compare_against_snapshot, load_snapshot_json},
    generate_postgres_password, get_rollup_client, interpolate_config, run_soak,
    start_and_wait_for_postgres_ready, Directories, API_URL, NUM_SOAK_BATCHES,
    POSTGRES_CONTAINER_NAME,
};
use chrono::Utc;
use clap::Parser;
use sov_api_spec::types::{self, GetSlotByIdChildren, Slot};
use std::{process::Command, time::Duration};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Initialize tracing subscriber with RUST_LOG environment variable, fallback to info
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug,hyper=info")),
        )
        .init();

    info!("Starting acceptance test");

    // Run the test
    let result = run_test().await;
    if let Err(e) = &result {
        tracing::error!("Acceptance test failed: {}", e);
    } else {
        info!("Acceptance test completed");
    }
    cleanup_postgres_container(POSTGRES_CONTAINER_NAME)?;

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
    std::fs::remove_file(directories.output_dir.join("mock_da.sqlite"))
        .or_else(ignore_file_not_found)?;
    std::fs::remove_file(directories.output_dir.join("mock_da.sqlite-shm"))
        .or_else(ignore_file_not_found)?;
    std::fs::remove_file(directories.output_dir.join("mock_da.sqlite-wal"))
        .or_else(ignore_file_not_found)?;

    // Then copy the base file, always
    std::fs::copy(
        directories.output_dir.join("persistent_mock_da.sqlite"),
        directories.output_dir.join("mock_da.sqlite"),
    )?;
    // And the dangling wal and shm only if they exist
    std::fs::copy(
        directories.output_dir.join("persistent_mock_da.sqlite-shm"),
        directories.output_dir.join("mock_da.sqlite-shm"),
    )
    .or_else(ignore_file_not_found)?;
    std::fs::copy(
        directories.output_dir.join("persistent_mock_da.sqlite-wal"),
        directories.output_dir.join("mock_da.sqlite-wal"),
    )
    .or_else(ignore_file_not_found)?;

    tracing::info!("Persistent mock data copied back to mock_da.sqlite");
    Ok(())
}

async fn run_test() -> Result<(), anyhow::Error> {
    // Generate a config file with our db password and all paths set relative to the workspace root
    let password = generate_postgres_password()?;
    let directories = Directories::new()?;
    interpolate_config(&password, &directories)?;

    tracing::info!(
        "Removing rollup data path: {}",
        directories.rollup_data_path.display()
    );
    std::fs::remove_dir_all(&directories.rollup_data_path)?;

    // Copy the persistent mock data back to mock_da.sqlite. This way we don't grow our DA files with each run.
    copy_persistent_mock_data(&directories)?;

    // Start the sequencer postgres and wait for it to be ready
    start_and_wait_for_postgres_ready(POSTGRES_CONTAINER_NAME, &password)?;

    info!("Building rollup...");
    if let Err(e) = build_rollup(directories.rollup_root.clone()) {
        cleanup_postgres_container(POSTGRES_CONTAINER_NAME)?;
        anyhow::bail!(e);
    }

    // Start the rollup. Run for 10 seconds
    info!(
        "Starting rollup from rollup workspace root: {}",
        directories.rollup_root.display()
    );

    let stop_at_height = NUM_SOAK_BATCHES * 2 + 10;

    let rollup = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--features",
            "acceptance-testing",
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
            &(stop_at_height.to_string()),
        ])
        .current_dir(directories.rollup_root.clone())
        .env("RUST_LOG", "info")
        .spawn()
        .expect("Failed to start rollup");

    // Wait up to 60s for the rollup to be ready
    for _ in 0..120 {
        if reqwest::get(&format!("{}/ledger/slots/0", API_URL))
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let mut slot_fetcher = SlotFetcher::new(get_rollup_client()?, &directories);
    slot_fetcher.subscribe_slots(false).await?;

    let mut checked = 0;
    let client = get_rollup_client()?;
    let mut latest_batch_num = 0;
    'outer: loop {
        let slot = slot_fetcher.next_slot().await?.unwrap();
        for slot_number in checked..=slot.number {
            let Ok(snapshot) = load_snapshot_json(slot_number, &directories.snapshots_dir) else {
                // We might be missing a few slots at the beginning.
                // If the slot number is less than 10, just ignore the missing snapshot.
                if slot_number < 10 {
                    continue;
                } else if latest_batch_num < NUM_SOAK_BATCHES {
                    panic!("Missing snapshot for slot {}", slot_number);
                } else {
                    // Once we've passed NUM_SOAK_BATCHES, and we find the first missing snapshot, we're done
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
                Some(GetSlotByIdChildren::_1)
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
    wait_for_sequencer_ready().await?;

    let new_throughput_report = run_soak(
        directories.clone(),
        rollup,
        latest_batch_num,
        stop_at_height,
        false,
    )
    .await?;
    let previous_throughput_report: ThroughputReport = serde_json::from_str::<ThroughputReport>(
        &std::fs::read_to_string(directories.throughput_dir.join(SETUP_THROUGHPUT_FILE))?,
    )?;
    let previous_throughput = previous_throughput_report.throughput();
    let new_throughput = new_throughput_report.throughput();
    if new_throughput < (previous_throughput * 0.9) {
        anyhow::bail!("Throughput is less than 90% of the previous throughput. This is likely due to a bug in the rollup. Old throughput: {:.2} txs/slot, new throughput: {:.2} txs/slot", previous_throughput, new_throughput);
    }

    // Save throughput report with timestamp to keep a record of test runs
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let throughput_filename = format!("test_throughput_{}.json", timestamp);
    std::fs::write(
        directories.throughput_dir.join(&throughput_filename),
        serde_json::to_string(&new_throughput_report)?,
    )?;
    info!("Saved throughput report to {}", throughput_filename);
    Ok(())
}

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value = "http://localhost:12346")]
    /// The URL of the rollup node to connect to. Defaults to http://localhost:12346.
    api_url: String,

    #[arg(short, long, default_value = "5")]
    /// The number of workers to spawn - this controls the number of concurrent transactions. Defaults to 5.
    num_workers: u32,

    #[arg(short, long, default_value = "0")]
    /// The salt to use for RNG. Use this value if you're restarting the generator and want to ensure that the generated
    /// transactions don't overlap with the previous run.
    salt: u32,
}
