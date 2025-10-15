use acceptance_test::fetch_and_compare::SlotFetcher;
use acceptance_test::ThroughputReport;
use acceptance_test::{
    cleanup_postgres_container,
    fetch_and_compare::{compare_against_snapshot, load_snapshot_json},
    generate_postgres_password, get_rollup_client, interpolate_config, run_soak,
    start_and_wait_for_postgres_ready, Directories, API_URL, NUM_SOAK_BATCHES,
    POSTGRES_CONTAINER_NAME,
};
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

fn copy_persistent_mock_data(directories: &Directories) -> Result<(), anyhow::Error> {
    tracing::info!("Copying persistent mock data back to mock_da.sqlite");
    std::fs::copy(
        directories.output_dir.join("persistent_mock_da.sqlite"),
        directories.output_dir.join("mock_da.sqlite"),
    )?;
    if let Err(e) = std::fs::copy(
        directories.output_dir.join("persistent_mock_da.sqlite-shm"),
        directories.output_dir.join("mock_da.sqlite-shm"),
    ) {
        if e.kind() != std::io::ErrorKind::NotFound {
            anyhow::bail!(
                "Failed to copy persistent_mock_da.sqlite-shm back to mock_da.sqlite-shm: {}",
                e
            );
        }
        tracing::trace!(
            "No persistent_mock_da.sqlite-shm found: {}. Proceeding anyway.",
            e
        );
    }
    if let Err(e) = std::fs::copy(
        directories.output_dir.join("persistent_mock_da.sqlite-wal"),
        directories.output_dir.join("mock_da.sqlite-wal"),
    ) {
        if e.kind() != std::io::ErrorKind::NotFound {
            anyhow::bail!(
                "Failed to copy persistent_mock_da.sqlite-wal back to mock_da.sqlite-wal: {}",
                e
            );
        }
        tracing::trace!("Failed to copy persistent_mock_da.sqlite-wal back to mock_da.sqlite-wal: {}. Proceeding anyway.", e);
    }
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

    // Start the rollup. Run for 10 seconds
    info!(
        "Starting rollup from rollup workspace root: {}",
        directories.rollup_root.display()
    );

    let rollup = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "--rollup-config-path",
            &directories
                .output_dir
                .join("config.toml")
                .display()
                .to_string(),
            "--stop-at-rollup-height",
            &((NUM_SOAK_BATCHES * 2).to_string()),
        ])
        .current_dir(directories.rollup_root.clone())
        .env("RUST_LOG", "info")
        .spawn()
        .expect("Failed to start rollup");

    // Wait a while, because this often requires compiling the entire rollup
    for _ in 0..2400 {
        if reqwest::get(&format!("{}/ledger/slots/0", API_URL))
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
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

    let new_throughput_report =
        run_soak(directories.clone(), rollup, latest_batch_num, false).await?;
    let previous_throughput_report: ThroughputReport = serde_json::from_str::<ThroughputReport>(
        &std::fs::read_to_string(directories.output_dir.join("throughput_report.json"))?,
    )?;
    let previous_throughput =
        previous_throughput_report.num_txs as f64 / previous_throughput_report.num_slots as f64;
    let new_throughput =
        new_throughput_report.num_txs as f64 / new_throughput_report.num_slots as f64;
    if new_throughput < (previous_throughput * 0.9) {
        anyhow::bail!("Throughput is less than 90% of the previous throughput. This is likely due to a bug in the rollup. Old throughput: {:.2} txs/slot, new throughput: {:.2} txs/slot", previous_throughput, new_throughput);
    }

    // Save throughput report to acceptance test directory
    std::fs::write(
        directories
            .acceptance_test_dir
            .join("accepted_throughput_report.json"),
        serde_json::to_string(&new_throughput_report)?,
    )?;
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
